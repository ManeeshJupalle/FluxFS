# FluxFS

[![CI](https://github.com/ManeeshJupalle/FluxFS/actions/workflows/ci.yml/badge.svg)](https://github.com/ManeeshJupalle/FluxFS/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/badge/crates.io-pending-lightgrey)](https://crates.io/crates/fluxfs)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**Intelligent filesystem autopilot** — watch, organize, deduplicate, and search your files automatically.

**Status:** **v0.2.0** on [GitHub](https://github.com/ManeeshJupalle/FluxFS) — background daemon, system tray, installers, and settings GUI. Install from [Releases](https://github.com/ManeeshJupalle/FluxFS/releases) or source; crates.io publish is ready (`cargo publish --dry-run` passes). Binaries: **`flux`**, **`fluxfs`**, **`fluxfs-tray`**, **`fluxfs-settings`**.

**Author:** [Maneesh Jupalle](mailto:maneeshreddy28@gmail.com)

---

## Demo

> **GIF placeholder:** Record a short demo (installer → tray icon → settings GUI → `flux status`) and add it here, e.g. `docs/demo.gif`.

```text
$ flux setup
FluxFS initialized.
  Service registered (auto-start + tray)

$ flux status
  ⚡ FluxFS Status
  Daemon:      ● Running (uptime 3h 22m)
  Service:     ● Registered (Windows Run key)
  Index:       142,847 files (48.3 GB)

$ flux find "assignment"
  ~/School/CS341_Assignment3.pdf   4.2 MB   May 15
  4 results (searched 142,847 files in 12ms)
```

Open **Settings…** from the tray (or `flux settings`) to edit watch folders and rules without TOML.

---

## Features

| Feature | Command / app | Description |
|---------|---------------|-------------|
| **Desktop setup** | `flux setup` | Scan watch paths, build index, register auto-start + tray |
| **Installers** | [Releases](https://github.com/ManeeshJupalle/FluxFS/releases) | Windows `.exe`, macOS `.dmg`, Linux `.deb` — no Rust required |
| **Background agent** | `flux start` / `flux stop` | Detached daemon; logs to `{data_dir}/flux.log` |
| **Auto-start** | `flux install-service` | systemd / LaunchAgent / Windows Run key at login |
| **System tray** | `fluxfs-tray` | Pause/resume, organize, open folders, launch settings |
| **Settings GUI** | `flux settings` | Watch folders, rules, dedup, activity — no TOML required |
| **Setup / scan** | `flux init` | Scan watch paths, hash content, build `index.bin` |
| **Search** | `flux find` | Fuzzy search (nucleo-matcher), glob, filters |
| **Organize** | `flux organize` | Run rules once without the daemon |
| **Dedup** | `flux dedup` | Find duplicates by SHA-256; trash/delete/report |
| **Status** | `flux status` | Dashboard: daemon, service, index, weekly stats |
| **Activity** | `flux log` | JSONL audit trail of moves and scans |
| **Config** | `flux config` / settings GUI | Show or edit TOML config |

---

## Installation

See **[docs/INSTALL.md](docs/INSTALL.md)** for one-click installers (Windows `.exe`, macOS `.dmg`, Linux `.deb`).

### From GitHub Releases (recommended for desktop)

Download the installer for your OS from [Releases](https://github.com/ManeeshJupalle/FluxFS/releases), run it, then verify:

```bash
flux status
```

Installers run `flux setup` automatically (scan Downloads + register auto-start + tray).

### From source (recommended until crates.io is live)

```bash
git clone https://github.com/ManeeshJupalle/FluxFS.git
cd FluxFS
cargo install --path .
```

Requires [Rust](https://rustup.rs/) and a C toolchain (MSVC on Windows, Xcode CLT on macOS).

```bash
flux --version
flux init
```

### From crates.io

Once published:

```bash
cargo install fluxfs
```

Installs **`flux`**, **`fluxfs`**, **`fluxfs-tray`**, and **`fluxfs-settings`**. Tray and settings require a display server (not headless servers).

After the first publish, swap the crates.io badge in this README for:

`[![crates.io](https://img.shields.io/crates/v/fluxfs.svg)](https://crates.io/crates/fluxfs)`

---

## Quick start

### Desktop (recommended)

1. Download the installer for your OS from [Releases](https://github.com/ManeeshJupalle/FluxFS/releases) — see [docs/INSTALL.md](docs/INSTALL.md).
2. Windows runs `flux setup` automatically; on macOS/Linux run `flux setup` after install.
3. Look for the **FluxFS tray icon**. Open **Settings…** to adjust watch folders and rules.

### From source or CLI

```bash
# Full desktop onboarding (init + auto-start + tray)
flux setup

# Or step by step:
flux init
flux install-service    # register auto-start + tray
flux start              # start daemon now (if not already running)

# Search, status, logs
flux find "invoice"
flux status
flux log -n 20
flux settings           # open settings GUI
```

Config paths: Windows `%APPDATA%\fluxfs\config.toml` · macOS/Linux `~/.config/fluxfs/config.toml`

**Debug mode:** `flux start --foreground` runs the watcher in the terminal (Ctrl+C to stop).

Any command that loads config (`init`, `start`, `find`, etc.) **auto-creates** `config.toml` with sensible defaults if it is missing.

Override config location for testing or multiple profiles:

```bash
export FLUXFS_CONFIG=/path/to/config.toml   # PowerShell: $env:FLUXFS_CONFIG = "..."
```

---

## Default configuration

The bundled defaults watch **`~/Downloads`** and organize new files with these rules (from `config/default.toml`):

| Matches | Destination |
|---------|-------------|
| `*.pdf` | `~/Documents/PDFs/` |
| `*.png`, `*.jpg`, `*.jpeg`, `*.gif`, `*.webp` | `~/Pictures/Organized/` |
| `*.dmg`, `*.exe`, `*.msi`, `*.pkg` | `~/Installers/` |
| `*.zip`, `*.tar.gz`, `*.rar`, `*.7z` | `~/Archives/` |

Other defaults:

- **Duplicates:** `strategy = "trash"` — duplicates move to `{data_dir}/trash` (not deleted)
- **Hashing:** skip files smaller than `1KB` or larger than `1GB`
- **Index:** skip `.git`, `node_modules`, `.venv`, etc.; symlinks not followed
- **Search:** up to 20 results per query

Run `flux config` to see your active file, edit via **`flux settings`**, or edit the TOML directly.

---

## Command reference

### `flux init`

Creates config (if missing), data directory, scans all `[[watch]]` paths, hashes files, saves `index.bin`.

### `flux setup`

Full desktop onboarding: runs `flux init` then `flux install-service` (auto-start + tray). Used by installers; supports `--quiet`, `--skip-init`, `--skip-service`.

### `flux start`

Starts the file watcher daemon in the **background** (default). Logs go to `{data_dir}/flux.log`.

```bash
flux start                  # detached background daemon
flux start --foreground     # run in terminal (Ctrl+C to stop)
flux start --daemon         # for systemd / LaunchAgent / service managers
```

### `flux settings`

Opens the **FluxFS Settings** window (`fluxfs-settings`) — edit watch folders, rules, dedup options, and view activity without editing TOML. Also available from the tray menu (**Settings…**).

### `flux install-service` / `flux uninstall-service`

Register or remove auto-start at login (systemd user unit on Linux, LaunchAgent on macOS, Run key on Windows). Also starts the tray app. Config and index are kept on uninstall.

```bash
flux install-service
flux uninstall-service
```

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

With `strategy = "trash"`, confirmed duplicates are moved to `{data_dir}/trash`.

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

### Error hints

When a command fails, FluxFS prints an actionable **`Hint:`** line when possible (e.g. run `flux init` for an empty index, `flux stop` before restarting the daemon, fix watch paths in config).

---

## Configuration reference

| Section | Field | Description |
|---------|-------|-------------|
| `[general]` | `data_dir` | Index, PID, activity log, trash (default `~/.local/share/fluxfs`) |
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
┌──────────────────────────────────────────────────────────────┐
│  Desktop (v0.2)                                              │
│  fluxfs-tray · fluxfs-settings · flux install-service        │
└────────────────────────────┬─────────────────────────────────┘
                             │ file IPC (paused, flux.stop)
                             ▼
┌─────────────┐     ┌──────────────┐     ┌─────────────────┐
│ notify      │────▶│ FileIndex    │────▶│ index.bin       │
│ watcher     │     │ (in-memory)  │     │ (bincode)       │
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
   activity.jsonl · flux.log (daemon)
```

1. **Scan** — `walkdir` + `rayon` collect metadata; symlinks skipped by default.  
2. **Index** — `HashMap<PathBuf, FileEntry>` persisted as bincode.  
3. **Watch** — `notify` events debounced 500ms; first matching rule wins; skipped when `{data_dir}/paused` exists.  
4. **Hash** — Parallel SHA-256 for dedup; files outside min/max size skipped.  
5. **Search** — `nucleo-matcher` ranks results in milliseconds on large indexes.  
6. **Moves** — Cross-volume moves use copy-then-delete when rename is not possible (common on Windows).  
7. **Desktop** — Agent runs at login; tray controls pause/organize; settings GUI edits config via `save_user_config()`.

Full architecture: [fluxfs-architecture.md](fluxfs-architecture.md) (Phases 1–8 engine + Phase 9 desktop).

Data locations (under `data_dir`, default `~/.local/share/fluxfs`):

| File / directory | Purpose |
|------------------|---------|
| `config.toml` | User configuration (in platform config dir, not data dir) |
| `index.bin` | Serialized file index |
| `activity.jsonl` | Action log (rotates at 10 MB) |
| `flux.pid` / `flux.started` | Daemon process metadata |
| `flux.log` | Daemon log (background / `--daemon` mode) |
| `flux.stop` | Graceful shutdown request (created by `flux stop`) |
| `paused` | When present, watcher skips organize (tray pause) |
| `service.installed` | OS auto-start registration marker |
| `trash/` | Duplicates moved here when `strategy = "trash"` |

Config file paths:

| Platform | Config path |
|----------|-------------|
| Windows | `%APPDATA%\fluxfs\config.toml` |
| macOS / Linux | `~/.config/fluxfs/config.toml` |
| Override | `FLUXFS_CONFIG` environment variable |

---

## Performance

Illustrative design targets on a modern laptop (NVMe, ~150k files). **Not benchmarked in CI** — run your own measurements below.

| Operation | Throughput / latency |
|-----------|-------------------|
| Initial scan | ~40,000–60,000 files/sec (metadata only) |
| Content hashing | ~200–400 MB/sec (parallel, size-dependent) |
| `flux find` | &lt;50 ms on 100k+ file indexes |
| Memory | ~50–80 MB for 150k-entry index |

```bash
cargo build --release
time flux init
time flux find "test"
```

---

## Roadmap

**v0.2.0 is complete** — background agent, tray, installers, and settings GUI. See **[docs/ROADMAP-v0.2.md](docs/ROADMAP-v0.2.md)** for the full plan and **[fluxfs-architecture.md](fluxfs-architecture.md)** for Phase 9 desktop architecture.

**Shipped in v0.2.0:**

**Phase A — background agent:**

- [x] `flux install-service` / `flux uninstall-service`
- [x] Background daemon + graceful shutdown + `flux.log`

**Phase B — system tray:**

- [x] `fluxfs-tray` system tray (pause/resume, organize, open folders)
- [x] Pause IPC via `{data_dir}/paused`
- [x] Tray auto-start bundled with service install

**Phase C — installers:**

- [x] Windows setup.exe, macOS `.dmg`, Linux `.deb` ([docs/INSTALL.md](docs/INSTALL.md))
- [x] `flux setup` post-install hook + GitHub Release CI

**Phase D — settings GUI:**

- [x] **`fluxfs-settings`** GUI — watch folders, rules, dedup, activity, status
- [x] **`flux settings`** + tray **Settings…** menu item
- [x] Save/reload config without editing TOML; dry-run **Test rules**

**Future (post v0.2):** code signing / notarization, auto-update, service integration tests in CI.

---

## Development

CI runs on **Linux, macOS, and Windows** (fmt, clippy, tests).

```bash
cargo build --release --bins
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

On **Linux**, install GUI dependencies before building tray/settings:

```bash
sudo apt-get install libgtk-3-dev libxdo-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev
```

Includes **14 integration tests** in `tests/integration.rs` (init, organize, dedup, find, status, log, watcher E2E, trash dedup, config, stop error, corrupt index recovery, dry-run regressions) plus **88 unit tests** across the crate.

Integration tests run the CLI in isolated temp dirs via `FLUXFS_CONFIG`.

See [CONTRIBUTING.md](CONTRIBUTING.md) and [CHANGELOG.md](CHANGELOG.md) for release history and PR guidelines.

### Publishing to crates.io

```bash
cargo login          # one-time API token from https://crates.io/settings/tokens
cargo publish --dry-run
cargo publish
```

Then update the crates.io badge in this README to the versioned shield.

---

## Contributing

1. Fork the repository  
2. Create a feature branch  
3. Ensure `cargo fmt`, `clippy`, and `test` pass  
4. Open a pull request with a clear description  

---

## License

MIT — see [LICENSE](LICENSE). Copyright (c) Maneesh Jupalle.
