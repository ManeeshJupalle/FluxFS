# CLAUDE CODE — FluxFS Build Prompt

> **What this file is**: Step-by-step instructions for Claude Code to build FluxFS, a Rust-powered intelligent filesystem autopilot.
> **Companion file**: `fluxfs-architecture.md` — the full technical specification. This prompt tells you *how* to build it; the architecture doc tells you *what* to build.
> **Language**: Rust (2021 edition)
> **Build tool**: Cargo

---

## CRITICAL — READ BEFORE WRITING ANY CODE

### Rust-Specific Rules

1. **No `unwrap()` in production code.** Use `?` operator with `anyhow::Result` for application code. Use `thiserror` for library-level errors. `unwrap()` is only acceptable in tests.

2. **No `clone()` unless necessary.** Prefer borrowing. If you clone, leave a comment explaining why ownership transfer won't work.

3. **`clippy` is law.** Run `cargo clippy -- -D warnings` after every step. Zero warnings, zero exceptions. Fix them before moving on.

4. **`cargo fmt` always.** Run `cargo fmt` after every file change. Non-negotiable.

5. **Tests for every module.** Each module gets a `#[cfg(test)] mod tests` block with unit tests. Integration tests go in `tests/`. Run `cargo test` after every step.

6. **Documentation.** Every public function and struct gets a `///` doc comment. Module-level `//!` doc comments for each `mod.rs`.

7. **Error messages are user-facing.** Every error a user might see should be clear, actionable, and friendly. No raw Rust error dumps.

### Process Rules

1. **Read the architecture doc first.** Before writing any code for a phase, read that phase's section in `fluxfs-architecture.md` completely. The architecture doc is the source of truth for data structures, method signatures, and behavior.

2. **One step at a time.** Each phase has substeps (1.1, 1.2, etc.). Complete each substep fully — including tests — before moving to the next.

3. **Test after every substep.** Run `cargo test` and `cargo clippy -- -D warnings` after completing each substep. Fix issues immediately.

4. **Don't anticipate future phases.** Build only what the current phase requires. Don't add "helpful" abstractions for later phases. Keep it lean.

5. **Commit messages.** After each phase, suggest a commit message in conventional commit format: `feat(scanner): add parallel directory walker with exclusion support`

### Code Style

- **Module organization**: Each module has a `mod.rs` that re-exports the public API. Internal implementation goes in sibling files.
- **Naming**: snake_case for functions/variables, PascalCase for types, SCREAMING_SNAKE for constants. Follow Rust conventions exactly.
- **Imports**: Group imports — std first, external crates second, internal modules third. Separated by blank lines.
- **Constants**: No magic numbers. Define constants with clear names.
- **Logging**: Use `tracing` macros (`info!`, `debug!`, `warn!`, `error!`). Debug-level for internal operations, info-level for user-visible actions, warn for recoverable issues, error for failures.

---

## BUILD SEQUENCE

### Phase 1: Foundation + CLI Skeleton

```
Step 1.1: "Read fluxfs-architecture.md Phase 1 completely. Then create the
project scaffold:
- Initialize with cargo init (binary project)
- Set up Cargo.toml with ALL dependencies listed in the architecture doc
- Create the full directory structure (all folders under src/)
- Create empty mod.rs files with module declarations
- Create .gitignore (target/, *.swp, .DS_Store)
- Run cargo build to verify everything compiles."

Step 1.2: "Build the CLI with clap derive in src/cli/commands.rs and wire
it into src/main.rs. Define all subcommands listed in the architecture doc
(init, start, stop, find, status, log, dedup, organize, config). Each
handler should just print 'Not implemented yet: <command>'. Verify with
cargo run -- --help and cargo run -- init."

Step 1.3: "Build the config system in src/config/. Define all config
structs in types.rs matching the TOML structure in the architecture doc.
Build the parser in parser.rs — load from ~/.config/fluxfs/config.toml,
fall back to embedded defaults if file doesn't exist. Add a
config/default.toml with the sensible defaults from the architecture doc.
Write tests for parsing valid config, missing config (defaults), and
invalid config (clear error message)."

Step 1.4: "Define error types in src/errors.rs using thiserror, matching
the architecture doc. Then set up tracing in main.rs — initialize
tracing-subscriber with the log level from config. Add a debug log on
startup showing loaded config path and watch directories."

Step 1.5: "Wire flux init to: create config dir + data dir if missing,
generate default config if missing, print the config path. Wire flux config
to load and pretty-print the current config. Run cargo test and
cargo clippy -- -D warnings. Fix all issues."
```

### Phase 2: File Scanner + Index

```
Step 2.1: "Read fluxfs-architecture.md Phase 2 completely. Build the
FileEntry struct in src/scanner/metadata.rs with all fields from the
architecture doc. Implement FileEntry::from_path(path: &Path) that extracts
all metadata from a real file. Write tests with tempfile — create a temp
file, build a FileEntry, verify all fields."

Step 2.2: "Build the directory walker in src/scanner/walker.rs. Use walkdir
with max_depth from config and exclusion patterns. Return Vec<FileEntry>.
Use rayon par_bridge for parallel metadata extraction. Write tests: scan
empty dir, scan dir with files, verify exclusions work, verify depth limit."

Step 2.3: "Build the FileIndex in src/index/store.rs with the HashMap-based
structure from the architecture doc. Implement insert, remove, get, stats
methods. Write unit tests for all methods."

Step 2.4: "Build index persistence in src/index/persistence.rs using
bincode. Implement save and load. Write tests: build index, save, load,
verify contents match. Test loading corrupted/missing file (should return
empty index, not crash)."

Step 2.5: "Wire everything into flux init: scan all watch directories,
build index, save to disk, print summary (file count, total size, scan
duration, directories scanned). Run cargo test and cargo clippy."
```

### Phase 3: Content Hashing + Duplicate Detection

```
Step 3.1: "Read fluxfs-architecture.md Phase 3 completely. Build the
content hasher in src/hasher/content.rs. Implement hash_file() with 8KB
buffer reads and SHA-256. Implement hash_all() using rayon for parallel
hashing. Skip files below min_size config. Write tests: hash known content,
verify deterministic output, verify parallel hashing produces same results
as sequential."

Step 3.2: "Build the duplicate detector in src/dedup/detector.rs. Implement
find_duplicates() that groups files by hash and returns DuplicateGroup
structs sorted by size. Implement resolve_duplicates() with the three
strategies (report, trash, delete). For trash: create trash dir at
~/.local/share/fluxfs/trash/, move file, log action. Write tests: plant
known duplicates in temp dir, verify detection, verify trash moves files."

Step 3.3: "Wire into CLI: flux dedup loads index, hashes unhashed files,
finds duplicates, prints report, applies strategy. Support --dry-run flag.
Also integrate hashing into flux init — hash everything after scanning.
Run cargo test and cargo clippy."
```

### Phase 4: Rule Engine + Auto-Organization

```
Step 4.1: "Read fluxfs-architecture.md Phase 4 completely. Build rule
types in src/rules/engine.rs — Rule, RulePattern (Extension, Contains,
OlderThan), RuleAction. Build the pattern matcher in src/rules/matcher.rs.
Write exhaustive tests: each pattern type with matching and non-matching
inputs."

Step 4.2: "Build file operations in src/rules/actions.rs. Implement
organize_file() with all safety checks from the architecture doc: create
destination dir, handle conflicts with suffix, skip no-ops, support
dry-run. Write tests with tempfile: move file, verify at destination,
verify conflict resolution, verify dry-run doesn't move."

Step 4.3: "Extend config parser to handle rule patterns from TOML
(extension glob, 'contains:X', 'older:Xd'). Parse [[watch.rules]] sections
into Vec<Rule> per watch path. Write tests for each pattern format."

Step 4.4: "Wire into CLI: flux organize loads index, iterates watched
directories, matches files against rules (first match wins), executes
moves, updates index, prints summary. Support --dry-run. Run cargo test
and cargo clippy."
```

### Phase 5: File Watcher Daemon

```
Step 5.1: "Read fluxfs-architecture.md Phase 5 completely. Build the
watcher in src/watcher/handler.rs using notify v6. Set up
RecommendedWatcher with a channel receiver. Handle Create, Remove, and
Rename events. On Create: build FileEntry, match rules, organize if
matched, add to index. On Remove: remove from index. Write tests with
tempfile: create watcher on temp dir, create a file, verify event fires."

Step 5.2: "Add event debouncing — collect events for 500ms, deduplicate
by path, process only the latest event per path. This prevents processing
partially-written files. Write tests: rapid-fire multiple events for same
path, verify only one processing call."

Step 5.3: "Build daemon management in src/watcher/daemon.rs. Implement:
PID file write/read/check at ~/.local/share/fluxfs/flux.pid, start
(write PID, begin watch loop), stop (read PID, send SIGTERM), is_running
check. Use tokio for the event loop and signal handling (SIGTERM/SIGINT).
On shutdown: save index, remove PID file, log."

Step 5.4: "Wire into CLI: flux start launches daemon (foreground with
--foreground flag, otherwise print instructions that foreground is default
for v0.1 — true daemonization is a stretch goal). flux stop reads PID and
terminates. Guard against duplicate daemons. Run cargo test and
cargo clippy."
```

### Phase 6: Fuzzy Search

```
Step 6.1: "Read fluxfs-architecture.md Phase 6 completely. Build search
in src/index/search.rs using nucleo-matcher. Implement search() that
fuzzy matches against filenames, scores results, sorts by relevance,
truncates to max_results. Write tests: index with known files, search
for partial names, verify ranking (exact > prefix > substring > fuzzy)."

Step 6.2: "Build search result display — formatted output with path
(colored), file size (human-readable KB/MB/GB), modified date, and search
timing. Handle zero results gracefully."

Step 6.3: "Wire into CLI: flux find <query> with flags --path (match full
path), --exact (glob mode), --ext (filter extension), --sort (size/date/
relevance). Run cargo test and cargo clippy."
```

### Phase 7: Status Dashboard + Activity Logging

```
Step 7.1: "Read fluxfs-architecture.md Phase 7 completely. Build the
activity log in src/reporting/activity.rs. Define ActivityEntry and
ActivityAction enums from the architecture doc. Implement append-only
JSONL writer at ~/.local/share/fluxfs/activity.jsonl. Implement reader
with filtering (last N, today, all). Write tests: append entries, read
back, filter by count and date."

Step 7.2: "Integrate activity logging into all existing operations: rule
engine (file moved), dedup (duplicate found/removed), scanner (scan
completed), watcher (file events). Every action that changes the
filesystem or index should log."

Step 7.3: "Build flux log command in src/reporting/activity.rs. Formatted
output with timestamps, emoji indicators, and action descriptions. Support
--all, --today, -n <count> flags."

Step 7.4: "Build flux status command in src/reporting/status.rs. Show
daemon state (running/stopped, PID, uptime), index stats (file count,
total size, last scan), weekly activity summary (files organized,
duplicates caught, space saved), attention items (remaining duplicates,
old downloads, empty dirs), watched directory breakdown. Use colored
terminal output."

Step 7.5: "Run cargo test and cargo clippy across the entire project.
Fix everything."
```

### Phase 8: Polish, Testing, README

```
Step 8.1: "Audit every file for unwrap() calls in non-test code — replace
with proper error handling. Audit every user-facing error message — make
them clear and actionable with hints where appropriate. Run cargo clippy
-- -D warnings one final time."

Step 8.2: "Handle edge cases listed in the architecture doc Phase 8:
symlinks (skip by default), permission denied (warn and skip), large files
(skip hashing above 1GB by default), filesystem full (graceful error),
watched directory deleted (log and continue), missing config (regenerate).
Write tests for each."

Step 8.3: "Write integration tests in tests/integration/ that test the
full pipeline in isolated temp directories: create structure → init →
verify index → add files → verify rules → add duplicates → verify dedup →
search → verify results. At least 5 integration tests covering the core
workflows."

Step 8.4: "Create a comprehensive README.md with:
- Project name, one-line description, badges (build status, license)
- Demo section (placeholder for GIF, plus text examples of all commands)
- Features list with brief descriptions
- Installation: cargo install fluxfs
- Quick Start: flux init → edit config → flux start
- Full command reference (every subcommand with examples)
- Configuration reference (every TOML field with descriptions)
- How It Works section (architecture overview for technical readers)
- Performance section (benchmarks: scan speed, search latency, memory)
- Roadmap section (TUI dashboard, cloud sync, GUI)
- License (MIT)"

Step 8.5: "Create .github/workflows/ci.yml — GitHub Actions workflow that
runs on push and PR: cargo fmt --check, cargo clippy -- -D warnings,
cargo test. Matrix: ubuntu-latest and macos-latest. Then do a final
cargo build --release and report binary size."

Step 8.6: "Final check: cargo fmt, cargo clippy -- -D warnings, cargo test.
All must pass. Print final summary: file count, line count (via
find src -name '*.rs' | xargs wc -l), test count, binary size."
```

---

## QUICK START COMMAND

After Claude Code reads both files, start with:

```
"Read fluxfs-architecture.md and CLAUDE-CODE-PROMPT.md completely.
Then build Phase 1: Foundation + CLI Skeleton.
Start with Step 1.1, then proceed through Steps 1.2-1.5 sequentially.
Run cargo test and cargo clippy -- -D warnings after each step."
```

For subsequent phases:
```
"Phase 1 is complete. Build Phase 2: File Scanner + Index.
Reference the architecture doc Phase 2 section for all specifications."
```

---

## TROUBLESHOOTING

### Common Rust Issues

1. **Borrow checker fights**: If the borrow checker blocks you, prefer `Arc<Mutex<T>>` for shared mutable state (especially the index in the daemon). Don't fight the borrow checker with `unsafe`.

2. **`notify` crate version confusion**: Use notify v6 (not v5). The API changed significantly. v6 uses `Event` with `EventKind`, not the old `DebouncedEvent`.

3. **`nucleo-matcher` API**: The API is lower-level than `fuzzy-matcher`. You'll need to convert strings to `Utf32Str` before matching. Check the docs if the examples in the architecture doc don't compile exactly.

4. **Cross-platform paths**: Use `PathBuf` and `Path` everywhere, never `String` for paths. Use `dirs::home_dir()` instead of hardcoding `~`. Expand `~` in config paths with a helper function.

5. **Large file hashing**: For files >100MB, consider using `mmap` (memmap2 crate) instead of buffered reads. But only if performance testing shows it matters — buffered reads are simpler and usually fast enough.

6. **Daemon on macOS**: `fork()` is discouraged on macOS. For v0.1, run in foreground (user can background with `&` or use a launch agent). True daemonization is a stretch goal.

### If Tests Fail

- Run `cargo test -- --nocapture` to see println/log output
- Run `cargo test <test_name>` to isolate a specific test
- Temp directory tests: make sure each test creates its own `tempdir()` — don't share state between tests

---

## SUCCESS CRITERIA

When FluxFS is complete, it should:

1. **Install cleanly**: `cargo install --path .` works with no errors
2. **Initialize fast**: `flux init` scans 100K+ files in <5 seconds
3. **Search instantly**: `flux find` returns results in <50ms
4. **Organize reliably**: Rules fire correctly with no data loss
5. **Watch silently**: Daemon idles at <10MB RAM, near-zero CPU
6. **Detect duplicates**: Content hashing finds true duplicates, not false positives
7. **Report clearly**: Status and log output is informative and actionable
8. **Handle errors gracefully**: No panics, no cryptic errors, no data loss
9. **Pass all checks**: `cargo fmt`, `cargo clippy`, `cargo test` all clean
10. **Look professional**: README, CI, clean code, comprehensive tests

This project should make a recruiter think "this person understands systems programming."
