# Changelog

All notable changes to FluxFS are documented here.

## [0.2.0-alpha.1] - 2026-05-21

### Added — Phase A (background agent)
- `flux install-service` / `flux uninstall-service` (systemd, LaunchAgent, Windows Run key)
- `flux start` spawns a detached background daemon; `flux start --daemon` for service managers
- Graceful shutdown via `{data_dir}/flux.stop` and daemon logging at `{data_dir}/flux.log`

### Added — Phase B (system tray)
- **`fluxfs-tray`** binary — tray icon (green running / yellow paused / red stopped)
- Menu: Pause/Resume, Run organize, Open data folder, Open log, Quit tray
- Pause IPC via `{data_dir}/paused` (watcher skips organize while paused)
- Tray auto-start registered with `flux install-service` on all platforms

### Changed
- Engine exposed as `fluxfs` library; CLI in `src/cli/runner.rs`
- [docs/ROADMAP-v0.2.md](docs/ROADMAP-v0.2.md) — full v0.2 plan (Phases A–D)

## [0.1.1] - 2026-05-21

### Added
- `flux` and `fluxfs` binary aliases (same CLI, either name works)
- Integration tests: `config`, `stop`, trash dedup, watcher E2E, corrupt index recovery
- Crates.io metadata (`repository`, `readme`, `keywords`, `categories`)
- GitHub Release artifact upload workflow (Linux, macOS, Windows)
- `CONTRIBUTING.md` and expanded publish checklist

### Fixed
- Default config hand-built for production (no runtime dependency on embedded TOML)
- Index persistence, hashing invalidation, and atomic save hardening
- Windows daemon PID detection and cross-volume file moves
- Organize dry-run no longer creates destination directories
- Dedup dry-run persists newly computed hashes

## [0.1.0] - 2026-05-21

Initial release — Phases 1–8:

- CLI: `init`, `start`, `stop`, `find`, `status`, `log`, `dedup`, `organize`, `config`
- File scanner + bincode index
- SHA-256 deduplication (report / trash / delete)
- Rule engine with extension, contains, and older-than patterns
- File watcher daemon with 500ms debounce
- Nucleo fuzzy search
- Activity log (JSONL) and status dashboard
- CI on Linux, macOS, and Windows

[0.1.1]: https://github.com/ManeeshJupalle/FluxFS/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ManeeshJupalle/FluxFS/releases/tag/v0.1.0
