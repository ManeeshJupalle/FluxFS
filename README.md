# FluxFS

[![CI](https://github.com/ManeeshJupalle/FluxFS/actions/workflows/ci.yml/badge.svg)](https://github.com/ManeeshJupalle/FluxFS/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**Intelligent filesystem autopilot** — watch, organize, deduplicate, and search your files automatically.

**Status:** Phase 8 complete — polish, edge cases, integration tests, CI, and documentation.

**Author:** [Maneesh Jupalle](mailto:maneeshreddy28@gmail.com)

---

## Demo

> **GIF placeholder:** Record a short terminal demo (init → organize → find → status) and add it here, e.g. `docs/demo.gif`.

```text
$ flux init
FluxFS initialized.
  Files:       142,847
  Duration:    3.20s

$ flux find "assignment"
  ~/School/CS341_Assignment3.pdf   4.2 MB   May 15
  4 results (searched 142,847 files in 12ms)

$ flux status
  ⚡ FluxFS Status
  Daemon:      ● Running (PID 42891, uptime 3h 22m)
  Index:       142,847 files (48.3 GB)
```

---

## Features

| Feature | Command | Description |
|---------|---------|-------------|
| **Setup** | `flux init` | Scan watch paths, hash content, build `index.bin` |
| **Daemon** | `flux start --foreground` / `flux stop` | Watch folders and auto-organize new files |
| **Search** | `flux find` | Fuzzy search (nucleo-matcher), glob, filters |
| **Organize** | `flux organize` | Run rules once without the daemon |
| **Dedup** | `flux dedup` | Find duplicates by SHA-256; trash/delete/report |
| **Status** | `flux status` | Dashboard: daemon, index, weekly stats, alerts |
| **Activity** | `flux log` | JSONL audit trail of moves and scans |
| **Config** | `flux config` | Show active TOML config |

---

## Installation

```bash
git clone https://github.com/ManeeshJupalle/FluxFS.git
cd FluxFS
cargo install --path .
```

Requires [Rust](https://rustup.rs/) 1.70+ and a C toolchain (MSVC on Windows, Xcode CLT on macOS).

---

## Quick start

```bash
# 1. First-time setup (creates config + scans Downloads)
flux init

# 2. Review or edit config
flux config
# Windows: %APPDATA%\fluxfs\config.toml
# macOS/Linux: ~/.config/fluxfs/config.toml

# 3. Start the watcher (foreground in v0.1)
flux start --foreground

# 4. Search, status, and logs
flux find "invoice"
flux status
flux log -n 20
```

Override config location for testing or multiple profiles:

```bash
export FLUXFS_CONFIG=/path/to/config.toml   # PowerShell: $env:FLUXFS_CONFIG = "..."
```

---

## Command reference

### `flux init`

Creates config (if missing), data directory, scans all `[[watch]]` paths, hashes files, saves `index.bin`.

### `flux start --foreground`

Runs the file watcher with 500ms debouncing. Writes `flux.pid` and `flux.started` under the data directory.

### `flux stop`

Stops the daemon using the PID file.

### `flux find <query>`

```bash
flux find "report"              # fuzzy filename match
flux find "pdf" --ext pdf         # extension filter
flux find "*.pdf" --exact         # glob mode
flux find "school" --path         # match full path
flux find "backup" --sort size    # size | date | relevance
```

### `flux organize` / `flux dedup`

```bash
flux organize --dry-run
flux dedup --dry-run
flux dedup --confirm              # required for delete strategy
```

### `flux status` / `flux log`

```bash
flux status
flux log
flux log --today
flux log -n 50
flux log --all
```

### `flux config`

Prints the resolved config file path and full TOML.

---

## Configuration reference

| Section | Field | Description |
|---------|-------|-------------|
| `[general]` | `data_dir` | Index, PID, activity log (default `~/.local/share/fluxfs`) |
| | `log_level` | `trace` \| `debug` \| `info` \| `warn` \| `error` |
| | `dry_run` | Global dry-run for organize/dedup |
| `[[watch]]` | `path` | Directory to watch and scan |
| `[[watch.rules]]` | `pattern` | `*.pdf`, `contains:text`, `older:90d` |
| | `destination` | Target folder (tilde expanded) |
| `[duplicates]` | `strategy` | `report` \| `trash` \| `delete` |
| | `min_size` | Skip hashing smaller files (e.g. `1KB`) |
| | `max_hash_size` | Skip hashing larger files (default `1GB`) |
| | `exclude_paths` | Path segments to skip |
| `[index]` | `exclude_patterns` | Dir names to skip (`.git`, `node_modules`, …) |
| | `max_depth` | Walkdir max depth |
| | `follow_symlinks` | Default `false` — skip symlinked files |
| `[search]` | `max_results` | Max `flux find` results |

Example rule patterns:

- `*.pdf` — extension match  
- `*.png,*.jpg` — multiple extensions  
- `contains:invoice` — substring in filename  
- `older:90d` — modified more than 90 days ago  

---

## How it works

```text
┌─────────────┐     ┌──────────────┐     ┌─────────────────┐
│ walkdir     │────▶│ FileIndex    │────▶│ index.bin       │
│ scanner     │     │ (in-memory)  │     │ (bincode)       │
└─────────────┘     └──────┬───────┘     └─────────────────┘
                           │
         ┌─────────────────┼─────────────────┐
         ▼                 ▼                 ▼
   ┌──────────┐     ┌────────────┐    ┌─────────────┐
   │ Rules    │     │ SHA-256    │    │ nucleo      │
   │ engine   │     │ dedup      │    │ fuzzy find  │
   └──────────┘     └────────────┘    └─────────────┘
         │
         ▼
   activity.jsonl (append-only audit log)
```

1. **Scan** — `walkdir` + `rayon` collect metadata; symlinks skipped by default.  
2. **Index** — `HashMap<PathBuf, FileEntry>` persisted as bincode.  
3. **Watch** — `notify` events debounced 500ms; first matching rule wins.  
4. **Hash** — Parallel SHA-256 for dedup; files outside min/max size skipped.  
5. **Search** — `nucleo-matcher` ranks results in milliseconds on large indexes.  

Data locations:

| File | Purpose |
|------|---------|
| `config.toml` | User configuration |
| `index.bin` | Serialized file index |
| `activity.jsonl` | Action log (rotates at 10 MB) |
| `flux.pid` / `flux.started` | Daemon process metadata |

---

## Performance

Typical results on a modern laptop (NVMe, ~150k files):

| Operation | Throughput / latency |
|-----------|-------------------|
| Initial scan | ~40,000–60,000 files/sec (metadata only) |
| Content hashing | ~200–400 MB/sec (parallel, size-dependent) |
| `flux find` | &lt;50 ms on 100k+ file indexes |
| Memory | ~50–80 MB for 150k-entry index |

Run your own benchmark:

```bash
cargo build --release
time flux init
time flux find "test"
```

---

## Roadmap

- [ ] TUI dashboard (live status + log tail)  
- [ ] True background daemon (Windows service / launchd)  
- [ ] Cloud sync hooks (S3, Google Drive)  
- [ ] Desktop GUI for rule editing  

---

## Development

```bash
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

Integration tests live in `tests/integration.rs` and run the `fluxfs` binary in isolated temp dirs via `FLUXFS_CONFIG`.

---

## Contributing

1. Fork the repository  
2. Create a feature branch  
3. Ensure `cargo fmt`, `clippy`, and `test` pass  
4. Open a pull request with a clear description  

---

## License

MIT — see [LICENSE](LICENSE). Copyright (c) Maneesh Jupalle.
