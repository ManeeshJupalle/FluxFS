//! End-to-end CLI integration tests in isolated temp directories.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn path_str(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn write_test_config(config_path: &Path, watch: &Path, data: &Path, dest: &Path) -> PathBuf {
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
strategy = "report"
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
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_path_buf();
    let watch = root.join("Downloads");
    let data = root.join("flux-data");
    let dest = root.join("PDFs");
    let config = root.join("config.toml");

    fs::create_dir_all(&watch).expect("mkdir watch");
    fs::create_dir_all(&dest).expect("mkdir dest");

    write_test_config(&config, &watch, &data, &dest);
    (temp, config, watch, data, dest)
}

fn flux_cmd(config: &Path) -> Command {
    let mut cmd = Command::cargo_bin("fluxfs").unwrap();
    cmd.env("FLUXFS_CONFIG", config).env_remove("RUST_LOG");
    cmd
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
fn pipeline_organize_moves_matching_pdf() {
    let (_temp, config, watch, data, dest) = setup_workspace();
    let source = watch.join("report.pdf");
    fs::write(&source, b"%PDF").expect("write");

    flux_cmd(&config).arg("init").assert().success();
    flux_cmd(&config).arg("organize")
        .assert()
        .success();

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
    flux_cmd(&config).arg("dedup").arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("Duplicates"));

    assert!(data.join("index.bin").exists());
}

#[test]
fn pipeline_find_returns_fuzzy_matches() {
    let (_temp, config, watch, _data, _dest) = setup_workspace();
    fs::write(watch.join("assignment_final.pdf"), b"%PDF").expect("write");
    fs::write(watch.join("readme.txt"), b"hello").expect("write");

    flux_cmd(&config).arg("init").assert().success();
    flux_cmd(&config).arg("find").arg("assignment")
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
    flux_cmd(&config).arg("status")
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
    flux_cmd(&config).arg("log")
        .assert()
        .success()
        .stdout(predicate::str::contains("scan").or(predicate::str::contains("Scan")));
}
