# FluxFS — Architecture Document

> **One-liner**: A Rust-powered intelligent filesystem autopilot that watches, organizes, deduplicates, and indexes your files automatically.

---

## Table of Contents

1. [Overview](#overview)
2. [Tech Stack](#tech-stack)
3. [Project Structure](#project-structure)
4. [Phase 1: Foundation + CLI Skeleton](#phase-1-foundation--cli-skeleton)
5. [Phase 2: File Scanner + Index](#phase-2-file-scanner--index)
6. [Phase 3: Content Hashing + Duplicate Detection](#phase-3-content-hashing--duplicate-detection)
7. [Phase 4: Rule Engine + Auto-Organization](#phase-4-rule-engine--auto-organization)
8. [Phase 5: File Watcher Daemon](#phase-5-file-watcher-daemon)
9. [Phase 6: Fuzzy Search](#phase-6-fuzzy-search)
10. [Phase 7: Status Dashboard + Activity Logging](#phase-7-status-dashboard--activity-logging)
11. [Phase 8: Polish, Testing, README](#phase-8-polish-testing-readme)

---

## Overview

### What FluxFS Does

FluxFS is a background daemon + CLI tool that makes your filesystem self-organizing:

- **Watches directories** in real-time for new/changed files using OS-level APIs
- **Auto-organizes files** based on user-defined rules (extension, filename pattern, age)
- **Detects duplicates** via SHA-256 content hashing across your entire filesystem
- **Indexes everything** for instant fuzzy search across hundreds of thousands of files
- **Reports filesystem health** — duplicates, stale files, unmatched files, activity stats

### Core Design Principles

1. **Zero-config useful, full-config powerful.** `flux init` should do something useful out of the box with sensible default rules. Power users customize via TOML.
2. **Never lose data.** All destructive operations (duplicate removal, file moves) are logged, reversible (trash, not delete), and optionally dry-run first.
3. **Minimal resource footprint.** The daemon should idle at <10MB RAM and near-zero CPU. Rust makes this achievable.
4. **Cross-platform.** Linux and macOS at minimum. Windows is a stretch goal.

---

## Tech Stack

| Component | Crate | Purpose |
|-----------|-------|---------|
| CLI framework | `clap` (v4, derive) | Subcommand parsing, help generation |
| File watching | `notify` (v6) | Cross-platform filesystem event notifications |
| Directory walking | `walkdir` | Recursive directory traversal |
| Parallelism | `rayon` | Parallel file hashing and scanning |
| Hashing | `sha2` | SHA-256 content fingerprinting |
| Config parsing | `serde` + `toml` | TOML config deserialization |
| Serialization | `serde` + `bincode` | Index persistence to disk |
| Fuzzy matching | `nucleo-matcher` | Fast fuzzy string matching for search |
| Error handling | `thiserror` + `anyhow` | Library errors + application errors |
| Logging | `tracing` + `tracing-subscriber` | Structured logging with levels |
| Terminal output | `colored` | Colored CLI output |
| Glob patterns | `glob` | File pattern matching in rules |
| Date/time | `chrono` | Timestamps, file age calculations |
| Platform dirs | `dirs` | Cross-platform home/config/data directories |
| PID management | `std::fs` + `nix` (Linux/macOS) | Daemon PID file management |
| Async runtime | `tokio` (minimal features) | Daemon event loop |

### Why These Choices

- **`notify` over manual polling**: Uses `inotify` (Linux) / `FSEvents` (macOS) / `ReadDirectoryChangesW` (Windows) under the hood. Kernel-level, zero CPU when idle.
- **`rayon` over manual threading**: Parallel iterators make concurrent hashing trivial and safe. No manual thread management.
- **`nucleo-matcher` over `fuzzy-matcher`**: Same engine that powers the Helix editor's fuzzy finder. Faster, better ranking.
- **`bincode` over SQLite**: The index is a simple `HashMap` + metadata. Binary serialization is simpler, faster, and has no C dependency. SQLite would be overkill here.
- **`tokio` with minimal features**: Only need the event loop for the daemon, not a full async runtime. Use `features = ["rt", "signal", "macros"]`.

---

## Project Structure

```
fluxfs/
├── Cargo.toml
├── README.md
├── LICENSE
├── .gitignore
├── config/
│   └── default.toml              # Default config with sensible rules
├── src/
│   ├── main.rs                   # Entry point — CLI dispatch
│   ├── cli/
│   │   ├── mod.rs                # CLI module root
│   │   └── commands.rs           # Clap command definitions
│   ├── config/
│   │   ├── mod.rs                # Config module root
│   │   ├── parser.rs             # TOML config loading + validation
│   │   └── types.rs              # Config structs (serde)
│   ├── scanner/
│   │   ├── mod.rs                # Scanner module root
│   │   ├── walker.rs             # Directory traversal with walkdir
│   │   └── metadata.rs           # File metadata extraction
│   ├── index/
│   │   ├── mod.rs                # Index module root
│   │   ├── store.rs              # In-memory index (HashMap<PathBuf, FileEntry>)
│   │   ├── persistence.rs        # Serialize/deserialize index to disk
│   │   └── search.rs             # Fuzzy search over indexed paths
│   ├── hasher/
│   │   ├── mod.rs                # Hasher module root
│   │   └── content.rs            # SHA-256 hashing, parallel with rayon
│   ├── dedup/
│   │   ├── mod.rs                # Dedup module root
│   │   └── detector.rs           # Group files by hash, find duplicates
│   ├── rules/
│   │   ├── mod.rs                # Rules module root
│   │   ├── engine.rs             # Rule matching logic
│   │   ├── matcher.rs            # Pattern matching (glob, contains, regex)
│   │   └── actions.rs            # File move/copy/trash operations
│   ├── watcher/
│   │   ├── mod.rs                # Watcher module root
│   │   ├── daemon.rs             # Background process management
│   │   └── handler.rs            # Event handler (new file → rules → organize)
│   ├── reporting/
│   │   ├── mod.rs                # Reporting module root
│   │   ├── status.rs             # `flux status` output
│   │   └── activity.rs           # Activity log tracking + `flux log` output
│   └── errors.rs                 # Error types with thiserror
└── tests/
    ├── integration/
    │   ├── test_scanner.rs       # Scanner integration tests
    │   ├── test_rules.rs         # Rule engine integration tests
    │   ├── test_dedup.rs         # Dedup integration tests
    │   ├── test_watcher.rs       # Watcher integration tests
    │   └── test_search.rs        # Search integration tests
    └── fixtures/
        └── test_tree/            # Test directory structure for integration tests
```

---

## Phase 1: Foundation + CLI Skeleton

### Goal
Project compiles, CLI parses all subcommands, config loads from TOML, error types defined, logging works.

### Step 1.1 — Cargo scaffold

```toml
# Cargo.toml
[package]
name = "fluxfs"
version = "0.1.0"
edition = "2021"
description = "Intelligent filesystem autopilot"
license = "MIT"
authors = ["Maneesh Jupalle <maneeshreddy28@gmail.com>"]

[dependencies]
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
toml = "0.8"
bincode = "1"
notify = "6"
walkdir = "2"
rayon = "1"
sha2 = "0.10"
glob = "0.3"
nucleo-matcher = "0.3"
thiserror = "2"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
colored = "2"
chrono = { version = "0.4", features = ["serde"] }
dirs = "5"
tokio = { version = "1", features = ["rt", "signal", "macros"] }

[dev-dependencies]
tempfile = "3"
assert_cmd = "2"
predicates = "3"
```

### Step 1.2 — CLI commands (clap derive)

All subcommands defined, even if handlers are stubs:

```
flux init              # First-time scan + index build
flux start             # Start the daemon
flux stop              # Stop the daemon
flux find <query>      # Fuzzy search files
flux status            # Filesystem health overview
flux log               # Recent activity log
flux dedup             # Find and handle duplicates
flux organize          # Run rules once (no daemon)
flux config            # Print current config location + contents
```

### Step 1.3 — Config types + parser

Config file location: `~/.config/fluxfs/config.toml`
Data directory: `~/.local/share/fluxfs/` (index, logs, PID file)

Config structure:

```toml
[general]
data_dir = "~/.local/share/fluxfs"   # Where index + logs live
log_level = "info"                    # trace, debug, info, warn, error
dry_run = false                       # Global dry-run mode

[[watch]]
path = "~/Downloads"

[[watch.rules]]
pattern = "*.pdf"
destination = "~/Documents/PDFs/"

[[watch.rules]]
pattern = "*.png,*.jpg,*.jpeg,*.gif,*.webp"
destination = "~/Pictures/Organized/"

[[watch.rules]]
pattern = "*.dmg,*.exe,*.msi,*.pkg"
destination = "~/Installers/"

[[watch.rules]]
pattern = "*.zip,*.tar.gz,*.rar,*.7z"
destination = "~/Archives/"

[duplicates]
strategy = "trash"        # "report", "trash", "delete"
min_size = "1KB"          # Ignore duplicates smaller than this
exclude_paths = [".git", "node_modules", ".venv"]

[index]
exclude_patterns = [".git", "node_modules", ".venv", "__pycache__", ".DS_Store"]
max_depth = 20            # Max directory depth to scan

[search]
max_results = 20
```

### Step 1.4 — Error types

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FluxError {
    #[error("Config error: {0}")]
    Config(String),

    #[error("Index error: {0}")]
    Index(String),

    #[error("Scanner error: {0}")]
    Scanner(String),

    #[error("Watcher error: {0}")]
    Watcher(String),

    #[error("Rule error: {0}")]
    Rule(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),
}
```

### Step 1.5 — Logging setup

Initialize `tracing-subscriber` with env filter from config log level. All modules use `tracing::{info, debug, warn, error}`.

### Step 1.6 — Default config generation

`flux init` should create `~/.config/fluxfs/config.toml` with sensible defaults if it doesn't exist. Include a `config/default.toml` in the repo as a reference.

### Completion Criteria
- `cargo build` succeeds with zero warnings
- `cargo test` passes (basic config parsing tests)
- `flux --help` shows all subcommands
- `flux init` creates config file + data directory
- `flux config` prints the loaded config

---

## Phase 2: File Scanner + Index

### Goal
Full recursive scan of watched directories, file metadata extraction, in-memory index with disk persistence.

### Step 2.1 — FileEntry struct

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub filename: String,         // Just the filename for fast search
    pub extension: Option<String>,
    pub size_bytes: u64,
    pub modified: DateTime<Utc>,
    pub created: Option<DateTime<Utc>>,
    pub content_hash: Option<String>,  // Populated later by hasher
    pub is_dir: bool,
}
```

### Step 2.2 — Directory walker

Use `walkdir::WalkDir` with:
- Respect `max_depth` from config
- Skip `exclude_patterns` directories (use `.filter_entry()`)
- Collect `FileEntry` for every file (skip dirs in entries, but walk into them)
- Use `rayon::par_bridge()` to parallelize metadata extraction

### Step 2.3 — In-memory index

```rust
pub struct FileIndex {
    entries: HashMap<PathBuf, FileEntry>,
    hash_groups: HashMap<String, Vec<PathBuf>>,  // For dedup lookup
    stats: IndexStats,
}

pub struct IndexStats {
    pub total_files: usize,
    pub total_size: u64,
    pub last_scan: DateTime<Utc>,
    pub scan_duration_ms: u64,
}
```

Methods:
- `insert(entry: FileEntry)`
- `remove(path: &Path)`
- `get(path: &Path) -> Option<&FileEntry>`
- `search(query: &str) -> Vec<&FileEntry>` (placeholder — Phase 6)
- `duplicates() -> Vec<Vec<&FileEntry>>` (groups with >1 entry sharing a hash)

### Step 2.4 — Index persistence

Serialize the entire `FileIndex` to `~/.local/share/fluxfs/index.bin` using `bincode`.
- `save(&self, path: &Path) -> Result<()>`
- `load(path: &Path) -> Result<Self>`
- Auto-save after every scan
- Auto-load on startup

### Step 2.5 — Wire into `flux init`

`flux init` should:
1. Create config if missing
2. Create data directory if missing
3. Run full scan of all `[[watch]]` paths
4. Build index
5. Save index to disk
6. Print summary: file count, total size, scan duration

### Completion Criteria
- `flux init` scans all configured directories and reports stats
- Index persists to disk and reloads on next run
- Scanning 100K+ files completes in <5 seconds
- Excluded patterns are properly skipped
- Tests: scan empty dir, scan with exclusions, index insert/remove/get, persistence round-trip

---

## Phase 3: Content Hashing + Duplicate Detection

### Goal
SHA-256 hash every indexed file in parallel, group by hash, detect and report duplicates.

### Step 3.1 — Content hasher

```rust
pub fn hash_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];  // 8KB read buffer
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 { break; }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
```

### Step 3.2 — Parallel hashing

Use `rayon` to hash all files in the index concurrently:

```rust
pub fn hash_all(index: &mut FileIndex) -> Result<HashStats> {
    let entries: Vec<PathBuf> = index.entries.keys().cloned().collect();
    let results: Vec<(PathBuf, String)> = entries
        .par_iter()
        .filter_map(|path| {
            hash_file(path).ok().map(|hash| (path.clone(), hash))
        })
        .collect();
    // Update index entries with hashes
    // Rebuild hash_groups
}
```

- Skip files smaller than `duplicates.min_size`
- Skip files in `duplicates.exclude_paths`
- Report progress for large scans (file count, % complete)

### Step 3.3 — Duplicate detector

```rust
pub struct DuplicateGroup {
    pub hash: String,
    pub size: u64,
    pub files: Vec<PathBuf>,
}

pub fn find_duplicates(index: &FileIndex) -> Vec<DuplicateGroup> {
    index.hash_groups.iter()
        .filter(|(_, paths)| paths.len() > 1)
        .map(|(hash, paths)| DuplicateGroup { ... })
        .sorted_by(|a, b| b.size.cmp(&a.size))  // Largest first
        .collect()
}
```

### Step 3.4 — Duplicate resolution

Based on `duplicates.strategy` config:
- `"report"` — Print duplicates, take no action
- `"trash"` — Move duplicates to `~/.local/share/fluxfs/trash/` (keep the oldest copy)
- `"delete"` — Permanent delete (require `--confirm` flag)

For all strategies, log every action to the activity log.

### Step 3.5 — Wire into CLI

`flux dedup` command:
- Load index
- Hash any unhashed files
- Find duplicates
- Print report: N groups, M duplicate files, X bytes reclaimable
- Apply strategy (or `--dry-run` to preview)

Also integrate into `flux init` — after first scan, hash everything and report duplicates.

### Completion Criteria
- Hashing 10K files completes in <10 seconds (parallel)
- Duplicates detected correctly (verified with test fixtures)
- `flux dedup` prints clear, actionable output
- `flux dedup --dry-run` previews without touching files
- Trash strategy moves files and logs actions
- Tests: hash known files, detect planted duplicates, verify trash behavior

---

## Phase 4: Rule Engine + Auto-Organization

### Goal
Parse rules from config, match files against rules, execute file moves with safety checks.

### Step 4.1 — Rule types

```rust
#[derive(Debug, Clone)]
pub struct Rule {
    pub pattern: RulePattern,
    pub destination: PathBuf,
    pub action: RuleAction,      // Move (default) or Copy
}

#[derive(Debug, Clone)]
pub enum RulePattern {
    Extension(Vec<String>),       // "*.pdf" or "*.png,*.jpg"
    Contains(String),             // filename contains "CS341"
    Regex(regex::Regex),          // Advanced: regex match
    OlderThan(Duration),          // Files older than X days
}

#[derive(Debug, Clone)]
pub enum RuleAction {
    Move,
    Copy,
}
```

### Step 4.2 — Rule matcher

```rust
pub fn matches(rule: &Rule, entry: &FileEntry) -> bool {
    match &rule.pattern {
        RulePattern::Extension(exts) => {
            entry.extension.as_ref()
                .map(|e| exts.contains(&e.to_lowercase()))
                .unwrap_or(false)
        }
        RulePattern::Contains(substring) => {
            entry.filename.to_lowercase().contains(&substring.to_lowercase())
        }
        // ... etc
    }
}
```

Rules are evaluated in order — first match wins. This is important so users can set specific rules above general ones.

### Step 4.3 — File operations with safety

```rust
pub fn organize_file(entry: &FileEntry, rule: &Rule, dry_run: bool) -> Result<OrganizeResult> {
    let dest_dir = &rule.destination;
    let dest_path = dest_dir.join(&entry.filename);

    // Safety checks:
    // 1. Destination directory exists (create if not)
    // 2. No overwrite — if dest_path exists, append _1, _2, etc.
    // 3. Don't move a file to where it already is
    // 4. Log the operation

    if dry_run {
        return Ok(OrganizeResult::DryRun { from, to });
    }

    match rule.action {
        RuleAction::Move => std::fs::rename(&entry.path, &dest_path)?,
        RuleAction::Copy => { std::fs::copy(&entry.path, &dest_path)?; }
    }

    Ok(OrganizeResult::Moved { from, to })
}
```

### Step 4.4 — Organize command

`flux organize` — Run all rules against all indexed files once:
- Load index
- For each watched directory, iterate files
- Match against rules in order
- Execute moves/copies
- Update index with new paths
- Print summary: N files organized, listed by rule

`flux organize --dry-run` — Preview only.

### Step 4.5 — Config parsing for rules

Extend the TOML parser to handle the `[[watch.rules]]` sections. Support these pattern formats:

```toml
# Extension match (most common)
pattern = "*.pdf"
pattern = "*.png,*.jpg,*.jpeg"

# Filename contains
pattern = "contains:CS341"

# Older than
pattern = "older:90d"
```

### Completion Criteria
- Rules parsed correctly from TOML config
- Pattern matching works for extensions, contains, older-than
- Files moved to correct destinations
- No data loss — conflict resolution appends suffix
- Dry-run mode shows what would happen without acting
- `flux organize` prints clear summary
- Tests: each pattern type, conflict resolution, dry-run, rule ordering

---

## Phase 5: File Watcher Daemon

### Goal
Background daemon watches configured directories in real-time, triggers rules on new files, updates index.

### Step 5.1 — File watcher setup

```rust
use notify::{Watcher, RecursiveMode, Event, EventKind};

pub struct FluxWatcher {
    watcher: RecommendedWatcher,
    index: Arc<Mutex<FileIndex>>,
    rules: Vec<(PathBuf, Vec<Rule>)>,  // (watch_path, rules)
    activity_log: Arc<Mutex<ActivityLog>>,
}
```

Watch all `[[watch]]` paths with `RecursiveMode::Recursive`.

### Step 5.2 — Event handler

```rust
fn handle_event(&self, event: Event) -> Result<()> {
    match event.kind {
        EventKind::Create(_) => {
            // New file: extract metadata, match rules, organize, update index
            for path in &event.paths {
                let entry = FileEntry::from_path(path)?;
                self.try_organize(&entry)?;
                self.index.lock().unwrap().insert(entry);
            }
        }
        EventKind::Remove(_) => {
            // Deleted file: remove from index
            for path in &event.paths {
                self.index.lock().unwrap().remove(path);
            }
        }
        EventKind::Modify(ModifyKind::Name(_)) => {
            // Renamed: update index path
        }
        _ => {}  // Ignore other events
    }
    Ok(())
}
```

### Step 5.3 — Event debouncing

Many editors and download managers trigger multiple events for a single file operation (create → modify → modify → close). Use a debounce window:

- Collect events for 500ms before processing
- Deduplicate by path — only process the latest event per path
- This prevents trying to move a file that's still being written

### Step 5.4 — Daemon management

**`flux start`:**
1. Check for existing PID file at `~/.local/share/fluxfs/flux.pid`
2. If running, print error and exit
3. Fork to background (or run in foreground with `--foreground` flag)
4. Write PID file
5. Load index from disk
6. Start watcher on all configured paths
7. Enter event loop
8. Periodically save index (every 5 minutes or on graceful shutdown)

**`flux stop`:**
1. Read PID file
2. Send SIGTERM to process
3. Process handles SIGTERM: save index, clean up, remove PID file

### Step 5.5 — Graceful shutdown

Use `tokio::signal` to catch SIGTERM/SIGINT:
- Save index to disk
- Remove PID file
- Log shutdown

### Completion Criteria
- Daemon starts and watches configured directories
- New files are detected within 1 second
- Rules fire correctly on new files
- Index updates in real-time
- `flux start` / `flux stop` work reliably
- PID file prevents duplicate daemons
- Graceful shutdown saves state
- Tests: create file → verify rule fires, delete file → verify removed from index, debouncing

---

## Phase 6: Fuzzy Search

### Goal
Instant fuzzy search across all indexed file paths and names.

### Step 6.1 — Search engine

```rust
use nucleo_matcher::{Matcher, Config, pattern::{Pattern, CaseMatching, Normalization}};

pub fn search(index: &FileIndex, query: &str, max_results: usize) -> Vec<SearchResult> {
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);

    let mut results: Vec<SearchResult> = index.entries.values()
        .filter_map(|entry| {
            let score = pattern.score(
                Utf32Str::from(&entry.filename),
                &mut matcher,
            )?;
            Some(SearchResult {
                entry: entry.clone(),
                score,
            })
        })
        .collect();

    results.sort_by(|a, b| b.score.cmp(&a.score));
    results.truncate(max_results);
    results
}
```

### Step 6.2 — Search result display

```
$ flux find "assignment"

  ~/School/Fall2026/CS341/CS341_Assignment3.pdf      4.2 MB   May 15
  ~/School/Fall2026/CS342/CS342_Assignment1.pdf      1.8 MB   Apr 22
  ~/Documents/PDFs/Assignment_Guidelines.pdf         230 KB   Sep 3
  ~/Downloads/old_assignment_draft.docx              45 KB    Mar 1

  4 results (searched 142,847 files in 12ms)
```

Format: path (colored by directory depth), size (human-readable), modified date. Show search time.

### Step 6.3 — Search options

- `flux find "query"` — fuzzy match on filename
- `flux find "query" --path` — fuzzy match on full path
- `flux find "*.pdf" --exact` — glob match instead of fuzzy
- `flux find "query" --ext pdf` — filter by extension
- `flux find "query" --sort size` — sort by size instead of relevance

### Completion Criteria
- Search across 100K+ files returns in <50ms
- Fuzzy matching is intuitive (typo-tolerant, substring-aware)
- Results ranked by relevance score
- Output is clean, colored, and informative
- Tests: fuzzy match accuracy, empty results, exact mode, extension filter

---

## Phase 7: Status Dashboard + Activity Logging

### Goal
Rich `flux status` output and persistent activity log.

### Step 7.1 — Activity log

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct ActivityEntry {
    pub timestamp: DateTime<Utc>,
    pub action: ActivityAction,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ActivityAction {
    FileMoved { from: PathBuf, to: PathBuf, rule: String },
    DuplicateFound { original: PathBuf, duplicate: PathBuf, size: u64 },
    DuplicateRemoved { path: PathBuf, size: u64 },
    FileIndexed { path: PathBuf },
    FileRemoved { path: PathBuf },
}
```

Store as append-only JSON lines file at `~/.local/share/fluxfs/activity.jsonl`. Rotate when >10MB.

### Step 7.2 — `flux log` command

```
$ flux log

  [May 21 14:02] 📂 Moved CS341_HW4.pdf → ~/School/Fall2026/CS341/
  [May 21 14:02] 🗑  Duplicate removed: lecture_notes (1).pdf (1.2 MB saved)
  [May 21 13:45] 📂 Moved screenshot_2026-05-21.png → ~/Pictures/Screenshots/2026-05/
  [May 21 12:30] 📂 Moved bank_export.csv → ~/CashPulse/data/imports/
  [May 21 09:15] 🔍 Full scan completed: 142,847 files indexed in 3.2s

  Showing last 10 entries. Use --all for full log.
```

Options: `flux log --all`, `flux log --today`, `flux log -n 50`

### Step 7.3 — `flux status` command

```
$ flux status

  ⚡ FluxFS Status
  ────────────────────────────────────

  Daemon:      ● Running (PID 42891, uptime 3h 22m)
  Index:       142,847 files (48.3 GB)
  Last scan:   Today at 09:15 (3.2s)
  Watching:    6 directories

  📊 This Week
     Files organized:     34
     Duplicates caught:   11
     Space saved:         2.1 GB

  ⚠️  Attention
     47 duplicates remaining (380 MB reclaimable)
     12 empty directories found
     23 files in ~/Downloads older than 90 days

  📁 Watched Directories
     ~/Downloads          1,247 files    Rule hits: 89%
     ~/Desktop            43 files       Rule hits: 62%
     ~/Documents          8,421 files    Rule hits: N/A
```

### Completion Criteria
- Activity log persists across daemon restarts
- `flux log` shows recent activity with clear formatting
- `flux status` shows daemon state, stats, and actionable suggestions
- Colored terminal output
- Tests: log write/read, status with no daemon, status with daemon

---

## Phase 8: Polish, Testing, README

### Step 8.1 — Error handling audit

Review every `unwrap()` and replace with proper error propagation. Every user-facing error should be helpful:

```
Error: Cannot watch ~/Downloads — directory does not exist.
Hint: Create the directory or update your config at ~/.config/fluxfs/config.toml
```

### Step 8.2 — Edge cases

- Symlinks: follow or skip (configurable, default skip)
- Permission denied: skip file, log warning, don't crash
- Very large files (>1GB): skip hashing by default, configurable threshold
- Filesystem full: graceful error on move operations
- Watched directory deleted: log error, continue watching others
- Config file missing: regenerate default with warning

### Step 8.3 — Integration tests

Test the full pipeline in isolated temp directories:
1. Create temp dir with known structure
2. Run `flux init` → verify index
3. Add files → verify rules fire
4. Add duplicates → verify detection
5. Search → verify results
6. Check status output

### Step 8.4 — README

Professional README with:
- One-line description + hero demo GIF
- Feature list with examples
- Installation instructions (`cargo install fluxfs`)
- Quick start (init → configure → start)
- Full config reference
- Performance benchmarks (files/sec scanned, search latency)
- Architecture overview for technical readers
- Contributing guidelines

### Step 8.5 — CI setup

GitHub Actions workflow:
- `cargo test` on push
- `cargo clippy -- -D warnings` (zero warnings policy)
- `cargo fmt --check` (formatting)
- Build on Linux + macOS

### Completion Criteria
- Zero `clippy` warnings
- All tests pass on Linux and macOS
- README is polished and complete
- `cargo install --path .` works cleanly
- Demo GIF recorded and embedded in README
