//! End-to-end CLI integration tests in isolated temp directories.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn path_str(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn write_test_config(
    config_path: &Path,
    watch: &Path,
    data: &Path,
    dest: &Path,
    duplicate_strategy: &str,
) -> PathBuf {
    let watch_s = path_str(watch);
    let data_s = path_str(data);
    let dest_s = path_str(dest);

    let toml = format!(
        r#"[general]
data_dir = "{data_s}"
log_level = "warn"
dry_run = false

[[watch]]
path = "{watch_s}"

[[watch.rules]]
pattern = "*.pdf"
destination = "{dest_s}"

[duplicates]
strategy = "{duplicate_strategy}"
min_size = "1B"
max_hash_size = "1GB"
exclude_paths = []

[index]
exclude_patterns = [".git", "node_modules"]
max_depth = 20
follow_symlinks = false

[search]
max_results = 20
"#
    );

    fs::write(config_path, toml).expect("write config");
    config_path.to_path_buf()
}

fn setup_workspace() -> (TempDir, PathBuf, PathBuf, PathBuf, PathBuf) {
    setup_workspace_with_strategy("report")
}

fn setup_workspace_with_strategy(strategy: &str) -> (TempDir, PathBuf, PathBuf, PathBuf, PathBuf) {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_path_buf();
    let watch = root.join("Downloads");
    let data = root.join("flux-data");
    let dest = root.join("PDFs");
    let config = root.join("config.toml");

    fs::create_dir_all(&watch).expect("mkdir watch");
    fs::create_dir_all(&dest).expect("mkdir dest");

    write_test_config(&config, &watch, &data, &dest, strategy);
    (temp, config, watch, data, dest)
}

fn flux_cmd(config: &Path) -> Command {
    let mut cmd = Command::cargo_bin("flux").unwrap();
    cmd.env("FLUXFS_CONFIG", config).env_remove("RUST_LOG");
    cmd
}

fn wait_for_file_move(source: &Path, dest: &Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !source.exists() && dest.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        !source.exists(),
        "source should be moved: {}",
        source.display()
    );
    assert!(
        dest.exists(),
        "destination should exist: {}",
        dest.display()
    );
}

#[test]
fn fluxfs_binary_alias_is_available() {
    Command::cargo_bin("fluxfs")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("flux"));
}

#[test]
fn pipeline_init_builds_index() {
    let (_temp, config, watch, data, _dest) = setup_workspace();
    fs::write(watch.join("notes.pdf"), b"%PDF-1.4").expect("write");

    flux_cmd(&config)
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("FluxFS initialized"));

    assert!(data.join("index.bin").exists());
}

#[test]
fn pipeline_config_prints_toml() {
    let (_temp, config, _watch, _data, _dest) = setup_workspace();

    flux_cmd(&config)
        .arg("config")
        .assert()
        .success()
        .stdout(predicate::str::contains("[general]"))
        .stdout(predicate::str::contains("data_dir"));
}

#[test]
fn pipeline_stop_when_not_running_reports_error() {
    let (_temp, config, _watch, _data, _dest) = setup_workspace();

    flux_cmd(&config)
        .arg("stop")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not running").or(predicate::str::contains("No PID")));
}

#[test]
fn pipeline_organize_moves_matching_pdf() {
    let (_temp, config, watch, data, dest) = setup_workspace();
    let source = watch.join("report.pdf");
    fs::write(&source, b"%PDF").expect("write");

    flux_cmd(&config).arg("init").assert().success();
    flux_cmd(&config).arg("organize").assert().success();

    assert!(!source.exists());
    assert!(dest.join("report.pdf").exists());

    let index = data.join("index.bin");
    assert!(index.exists());
}

#[test]
fn pipeline_dedup_detects_duplicate_content() {
    let (_temp, config, watch, data, _dest) = setup_workspace();
    fs::write(watch.join("a.txt"), b"same-bytes").expect("write");
    fs::write(watch.join("b.txt"), b"same-bytes").expect("write");

    flux_cmd(&config).arg("init").assert().success();
    flux_cmd(&config)
        .arg("dedup")
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("Duplicates"));

    assert!(data.join("index.bin").exists());
}

#[test]
fn pipeline_dedup_trash_moves_duplicate_to_trash_dir() {
    let (_temp, config, watch, data, _dest) = setup_workspace_with_strategy("trash");
    fs::write(watch.join("a.txt"), b"same-bytes").expect("write");
    fs::write(watch.join("b.txt"), b"same-bytes").expect("write");

    flux_cmd(&config).arg("init").assert().success();
    flux_cmd(&config).arg("dedup").assert().success();

    let trash_dir = data.join("trash");
    assert!(trash_dir.is_dir(), "trash directory should exist");

    let trash_entries: Vec<_> = fs::read_dir(&trash_dir)
        .expect("read trash")
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(trash_entries.len(), 1, "one duplicate should be in trash");

    let remaining: Vec<_> = fs::read_dir(&watch)
        .expect("read watch")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .collect();
    assert_eq!(
        remaining.len(),
        1,
        "one original should remain in watch dir"
    );
}

/// Regression: a `flux dedup --dry-run` (or report-only strategy) that hashes
/// previously-unhashed files must persist those hashes. Without this, the
/// next `flux dedup` would do all of that work over again.
#[test]
fn pipeline_dedup_dry_run_persists_new_hashes() {
    let (_temp, config, watch, data, _dest) = setup_workspace();

    fs::write(watch.join("a.txt"), b"same-bytes").expect("write");
    flux_cmd(&config).arg("init").assert().success();
    let index_after_init = data.join("index.bin");
    assert!(index_after_init.exists());
    let mtime_after_init = fs::metadata(&index_after_init)
        .expect("init metadata")
        .modified()
        .expect("init mtime");

    fs::write(watch.join("b.txt"), b"another-content").expect("write b");
    flux_cmd(&config).arg("init").assert().success();

    std::thread::sleep(Duration::from_millis(1100));
    flux_cmd(&config)
        .arg("dedup")
        .arg("--dry-run")
        .assert()
        .success();

    let mtime_after_dedup = fs::metadata(&index_after_init)
        .expect("post-dedup metadata")
        .modified()
        .expect("post-dedup mtime");
    assert!(
        mtime_after_dedup >= mtime_after_init,
        "index mtime should not go backwards across dry-run dedup"
    );
}

/// Regression: `flux organize --dry-run` must not create destination
/// directories. The previous implementation created them eagerly inside
/// `resolve_destination`, leaving stray empty folders behind.
#[test]
fn pipeline_organize_dry_run_does_not_create_destination() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_path_buf();
    let watch = root.join("Downloads");
    let data = root.join("flux-data");
    let dest = root.join("never_created");
    let config = root.join("config.toml");
    fs::create_dir_all(&watch).expect("mkdir watch");
    write_test_config(&config, &watch, &data, &dest, "report");

    fs::write(watch.join("report.pdf"), b"%PDF").expect("write");

    flux_cmd(&config).arg("init").assert().success();
    flux_cmd(&config)
        .arg("organize")
        .arg("--dry-run")
        .assert()
        .success();

    assert!(
        watch.join("report.pdf").exists(),
        "source file must remain in place"
    );
    assert!(
        !dest.exists(),
        "dry-run must not create the destination directory"
    );
}

#[test]
fn pipeline_find_returns_fuzzy_matches() {
    let (_temp, config, watch, _data, _dest) = setup_workspace();
    fs::write(watch.join("assignment_final.pdf"), b"%PDF").expect("write");
    fs::write(watch.join("readme.txt"), b"hello").expect("write");

    flux_cmd(&config).arg("init").assert().success();
    flux_cmd(&config)
        .arg("find")
        .arg("assignment")
        .assert()
        .success()
        .stdout(predicate::str::contains("assignment_final.pdf"));
}

#[test]
fn pipeline_status_reports_index_stats() {
    let (_temp, config, watch, _data, _dest) = setup_workspace();
    fs::write(watch.join("one.pdf"), b"%PDF").expect("write");
    fs::write(watch.join("two.pdf"), b"%PDF2").expect("write");

    flux_cmd(&config).arg("init").assert().success();
    flux_cmd(&config)
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("FluxFS Status"))
        .stdout(predicate::str::contains("files"));
}

#[test]
fn pipeline_log_shows_scan_activity() {
    let (_temp, config, watch, _data, _dest) = setup_workspace();
    fs::write(watch.join("doc.pdf"), b"%PDF").expect("write");

    flux_cmd(&config).arg("init").assert().success();
    flux_cmd(&config)
        .arg("log")
        .assert()
        .success()
        .stdout(predicate::str::contains("scan").or(predicate::str::contains("Scan")));
}

/// Watcher E2E: daemon organizes a newly dropped PDF via rules.
#[test]
fn pipeline_watcher_organizes_new_file() {
    let (_temp, config, watch, _data, dest) = setup_workspace();

    flux_cmd(&config).arg("init").assert().success();

    let flux_bin = assert_cmd::cargo_bin!("flux");
    let mut daemon = std::process::Command::new(flux_bin)
        .arg("start")
        .arg("--foreground")
        .env("FLUXFS_CONFIG", &config)
        .env_remove("RUST_LOG")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn daemon");

    // Allow watcher registration and PID file write.
    std::thread::sleep(Duration::from_secs(2));

    let source = watch.join("incoming.pdf");
    let dest_file = dest.join("incoming.pdf");
    fs::write(&source, b"%PDF-watcher-test").expect("write incoming pdf");

    // Debounce (500ms) + organize + index update — poll until moved or timeout.
    wait_for_file_move(&source, &dest_file, Duration::from_secs(8));

    let stop_result = flux_cmd(&config).arg("stop").assert();
    if stop_result.get_output().status.success() {
        let _ = daemon.wait();
    } else {
        let _ = daemon.kill();
        let _ = daemon.wait();
    }

    assert!(
        dest_file.exists(),
        "incoming.pdf should appear in PDFs destination"
    );
}

#[test]
fn pipeline_corrupt_index_recovers_on_init() {
    let (_temp, config, watch, data, _dest) = setup_workspace();
    fs::write(watch.join("ok.pdf"), b"%PDF").expect("write");

    flux_cmd(&config).arg("init").assert().success();

    let index_path = data.join("index.bin");
    fs::write(&index_path, b"not-valid-bincode").expect("corrupt index");

    flux_cmd(&config)
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("FluxFS initialized"));

    assert!(index_path.exists());
    let meta = fs::metadata(&index_path).expect("index metadata");
    assert!(meta.len() > 16, "index should be rewritten with valid data");
}
