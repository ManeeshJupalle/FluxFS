//! Debounce filesystem events by path before processing.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Kind of filesystem change after debouncing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebouncedKind {
    Created,
    Removed,
    Renamed { to: PathBuf },
    Modified,
}

/// A debounced event for a single path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebouncedEvent {
    pub path: PathBuf,
    pub kind: DebouncedKind,
    pub updated_at: Instant,
}

/// Collects rapid events and flushes the latest per path after a quiet window.
#[derive(Debug)]
pub struct EventDebouncer {
    debounce: Duration,
    pending: HashMap<PathBuf, DebouncedEvent>,
}

impl EventDebouncer {
    /// Create a debouncer with the given quiet period in milliseconds.
    pub fn new(debounce_ms: u64) -> Self {
        Self {
            debounce: Duration::from_millis(debounce_ms),
            pending: HashMap::new(),
        }
    }

    /// Record or update an event for a path (latest kind wins).
    pub fn push(&mut self, path: PathBuf, kind: DebouncedKind) {
        let updated_at = Instant::now();
        self.pending.insert(
            path.clone(),
            DebouncedEvent {
                path,
                kind,
                updated_at,
            },
        );
    }

    /// Returns events that have been quiet for at least `debounce`.
    pub fn flush_ready(&mut self, now: Instant) -> Vec<DebouncedEvent> {
        let ready: Vec<PathBuf> = self
            .pending
            .iter()
            .filter(|(_, event)| now.duration_since(event.updated_at) >= self.debounce)
            .map(|(path, _)| path.clone())
            .collect();

        ready
            .into_iter()
            .filter_map(|path| self.pending.remove(&path))
            .collect()
    }

    /// Force-flush all pending events (for shutdown).
    pub fn flush_all(&mut self) -> Vec<DebouncedEvent> {
        self.pending.drain().map(|(_, event)| event).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration as StdDuration;

    #[test]
    fn keeps_only_latest_kind_per_path() {
        let mut debouncer = EventDebouncer::new(50);
        let path = PathBuf::from("/tmp/file.txt");
        debouncer.push(path.clone(), DebouncedKind::Created);
        debouncer.push(path.clone(), DebouncedKind::Modified);

        thread::sleep(StdDuration::from_millis(60));
        let flushed = debouncer.flush_ready(Instant::now());
        assert_eq!(flushed.len(), 1);
        assert_eq!(flushed[0].kind, DebouncedKind::Modified);
    }

    #[test]
    fn deduplicates_rapid_events_for_same_path() {
        let mut debouncer = EventDebouncer::new(50);
        let path = PathBuf::from("/tmp/a.txt");

        for _ in 0..5 {
            debouncer.push(path.clone(), DebouncedKind::Created);
        }

        thread::sleep(StdDuration::from_millis(60));
        let flushed = debouncer.flush_ready(Instant::now());
        assert_eq!(flushed.len(), 1);
    }

    #[test]
    fn does_not_flush_before_window_elapses() {
        let mut debouncer = EventDebouncer::new(200);
        debouncer.push(PathBuf::from("/tmp/b.txt"), DebouncedKind::Created);

        let flushed = debouncer.flush_ready(Instant::now());
        assert!(flushed.is_empty());
    }
}
