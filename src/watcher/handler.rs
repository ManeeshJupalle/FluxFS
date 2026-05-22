//! Event handler — new file → rules → organize → index update.

use crate::errors::{FluxError, Result};
use crate::index::store::FileIndex;
use crate::reporting::activity::log_file_removed;
use crate::rules::engine::{process_new_file, WatchRuleset};
use crate::watcher::debounce::{DebouncedEvent, DebouncedKind, EventDebouncer};
use notify::event::ModifyKind;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, warn};

/// Default debounce window for filesystem events (milliseconds).
pub const DEBOUNCE_MS: u64 = 500;

/// Shared watcher state.
pub struct FluxWatcher {
    index: Arc<Mutex<FileIndex>>,
    rulesets: Vec<WatchRuleset>,
    activity_log: PathBuf,
    dry_run: bool,
    debounce_ms: u64,
}

impl FluxWatcher {
    /// Create a watcher context.
    pub fn new(
        index: Arc<Mutex<FileIndex>>,
        rulesets: Vec<WatchRuleset>,
        activity_log: PathBuf,
        dry_run: bool,
    ) -> Self {
        Self {
            index,
            rulesets,
            activity_log,
            dry_run,
            debounce_ms: DEBOUNCE_MS,
        }
    }

    /// Set debounce period (useful in tests).
    #[allow(dead_code)]
    pub fn with_debounce_ms(mut self, debounce_ms: u64) -> Self {
        self.debounce_ms = debounce_ms;
        self
    }

    /// Create a watcher wired to the provided sender.
    pub fn build_watcher(
        tx: std::sync::mpsc::Sender<notify::Result<Event>>,
        paths: &[PathBuf],
    ) -> Result<RecommendedWatcher> {
        let mut watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            notify::Config::default(),
        )
        .map_err(|e| FluxError::Watcher(format!("Cannot create file watcher: {e}")))?;

        for path in paths {
            if !path.exists() {
                warn!(
                    path = %path.display(),
                    "Watch directory does not exist, skipping"
                );
                continue;
            }
            watcher.watch(path, RecursiveMode::Recursive).map_err(|e| {
                FluxError::Watcher(format!("Cannot watch {} — {e}", path.display()))
            })?;
            debug!(path = %path.display(), "Watching directory");
        }

        Ok(watcher)
    }

    /// Process notify events for a bounded duration (used by the daemon and tests).
    #[allow(dead_code)]
    pub fn run_for_duration(
        &self,
        rx: std::sync::mpsc::Receiver<notify::Result<Event>>,
        watcher: RecommendedWatcher,
        duration: Duration,
    ) -> Result<()> {
        let _keep_alive = watcher;
        let mut debouncer = EventDebouncer::new(self.debounce_ms);
        let deadline = Instant::now() + duration;

        while Instant::now() < deadline {
            while let Ok(event) = rx.try_recv() {
                self.enqueue_event(&mut debouncer, event);
            }

            let now = Instant::now();
            for debounced in debouncer.flush_ready(now) {
                if let Err(err) = self.handle_debounced(debounced) {
                    warn!(error = %err, "Failed to handle debounced event");
                }
            }

            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(event) => self.enqueue_event(&mut debouncer, event),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        for debounced in debouncer.flush_all() {
            let _ = self.handle_debounced(debounced);
        }

        Ok(())
    }

    /// Process events until the channel disconnects.
    pub fn run_event_loop(
        &self,
        rx: std::sync::mpsc::Receiver<notify::Result<Event>>,
        watcher: RecommendedWatcher,
    ) -> Result<()> {
        let _keep_alive = watcher;
        let mut debouncer = EventDebouncer::new(self.debounce_ms);

        loop {
            while let Ok(event) = rx.try_recv() {
                self.enqueue_event(&mut debouncer, event);
            }

            let now = Instant::now();
            for debounced in debouncer.flush_ready(now) {
                if let Err(err) = self.handle_debounced(debounced) {
                    warn!(error = %err, "Failed to handle debounced event");
                }
            }

            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(event) => self.enqueue_event(&mut debouncer, event),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        for debounced in debouncer.flush_all() {
            let _ = self.handle_debounced(debounced);
        }

        Ok(())
    }

    fn enqueue_event(&self, debouncer: &mut EventDebouncer, event: notify::Result<Event>) {
        let Ok(event) = event else {
            warn!("Watcher error");
            return;
        };

        match event.kind {
            EventKind::Create(_) => {
                for path in event.paths {
                    debouncer.push(path, DebouncedKind::Created);
                }
            }
            EventKind::Remove(_) => {
                for path in event.paths {
                    debouncer.push(path, DebouncedKind::Removed);
                }
            }
            EventKind::Modify(ModifyKind::Name(_)) => {
                if event.paths.len() >= 2 {
                    let from = event.paths[0].clone();
                    let to = event.paths[1].clone();
                    debouncer.push(from, DebouncedKind::Renamed { to });
                } else if let Some(path) = event.paths.first() {
                    debouncer.push(path.clone(), DebouncedKind::Modified);
                }
            }
            EventKind::Modify(ModifyKind::Data(_)) => {
                for path in event.paths {
                    debouncer.push(path, DebouncedKind::Modified);
                }
            }
            _ => {}
        }
    }

    fn handle_debounced(&self, event: DebouncedEvent) -> Result<()> {
        let mut index = self
            .index
            .lock()
            .map_err(|_| FluxError::Watcher("Index lock poisoned.".to_string()))?;

        match event.kind {
            DebouncedKind::Created | DebouncedKind::Modified => {
                process_new_file(
                    &mut index,
                    &event.path,
                    &self.rulesets,
                    self.dry_run,
                    &self.activity_log,
                )?;
            }
            DebouncedKind::Removed => {
                index.remove(&event.path);
                if !self.dry_run {
                    log_file_removed(&self.activity_log, &event.path)?;
                }
            }
            DebouncedKind::Renamed { to } => {
                index.remove(&event.path);
                process_new_file(
                    &mut index,
                    &to,
                    &self.rulesets,
                    self.dry_run,
                    &self.activity_log,
                )?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::rules::build_rule;
    use crate::config::types::WatchRule;
    use crate::index::store::FileIndex;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn watcher_detects_new_file_and_updates_index() {
        let dir = tempdir().expect("tempdir");
        let watch = dir.path().join("watch");
        std::fs::create_dir_all(&watch).expect("mkdir");

        let index = Arc::new(Mutex::new(FileIndex::new()));
        let rulesets = vec![WatchRuleset {
            watch_path: watch.clone(),
            rules: vec![],
        }];

        let flux = FluxWatcher::new(
            Arc::clone(&index),
            rulesets,
            dir.path().join("activity.jsonl"),
            false,
        )
        .with_debounce_ms(50);

        let (tx, rx) = mpsc::channel();
        let watcher = FluxWatcher::build_watcher(tx, std::slice::from_ref(&watch)).expect("watcher");

        thread::sleep(Duration::from_millis(100));
        std::fs::write(watch.join("new_file.txt"), b"hello").expect("write");

        flux.run_for_duration(rx, watcher, Duration::from_millis(800))
            .expect("run");

        assert_eq!(index.lock().expect("lock").len(), 1);
    }

    #[test]
    fn watcher_organizes_matching_file() {
        let dir = tempdir().expect("tempdir");
        let watch = dir.path().join("watch");
        let dest = dir.path().join("pdfs");
        std::fs::create_dir_all(&watch).expect("mkdir");

        let index = Arc::new(Mutex::new(FileIndex::new()));
        let rulesets = vec![WatchRuleset {
            watch_path: watch.clone(),
            rules: vec![build_rule(&WatchRule {
                pattern: "*.pdf".to_string(),
                destination: dest.to_str().expect("utf8").to_string(),
            })
            .expect("rule")],
        }];

        let flux = FluxWatcher::new(
            Arc::clone(&index),
            rulesets,
            dir.path().join("activity.jsonl"),
            false,
        )
        .with_debounce_ms(50);

        let (tx, rx) = mpsc::channel();
        let watcher = FluxWatcher::build_watcher(tx, std::slice::from_ref(&watch)).expect("watcher");

        thread::sleep(Duration::from_millis(100));
        let source = watch.join("doc.pdf");
        std::fs::write(&source, b"%PDF").expect("write");

        flux.run_for_duration(rx, watcher, Duration::from_millis(800))
            .expect("run");

        assert!(!source.exists());
        assert!(dest.join("doc.pdf").exists());
    }
}
