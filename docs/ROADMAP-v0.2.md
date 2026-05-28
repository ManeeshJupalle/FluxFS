# FluxFS v0.2 Roadmap — From CLI to Desktop Software

**Status:** **v0.2.0 shipped** (2026-05-21) — [GitHub Release](https://github.com/ManeeshJupalle/FluxFS/releases/tag/v0.2.0)

**Goal (achieved):** Transform FluxFS from a developer CLI into installable background software that runs at login, organizes downloads automatically, and is manageable without a terminal.

**Baseline (v0.1.x):** CLI-only — init, find, dedup, organize, status, log, foreground watcher, 14 integration tests, CI on Linux/macOS/Windows.

**v0.2.0 deliverables:** Background agent + OS auto-start + system tray + installers + settings GUI.

---

## Architecture (v0.2 — implemented)

```text
┌─────────────────────────────────────────────────────────────┐
│                     FluxFS Desktop (v0.2)                   │
├──────────────┬──────────────────────┬───────────────────────┤
│  Tray App    │   Settings GUI       │   CLI (flux / fluxfs) │
│  (Phase B)   │   (Phase D)            │   (existing + service)│
└──────┬───────┴──────────┬───────────┴───────────┬───────────┘
       │                  │                       │
       └──────────────────┼───────────────────────┘
                          ▼
              ┌───────────────────────┐
              │   FluxFS Engine       │  ← already built (v0.1)
              │   index · rules ·     │
              │   dedup · search ·    │
              │   activity log        │
              └───────────┬───────────┘
                          ▼
              ┌───────────────────────┐
              │   FluxFS Agent        │  ← Phase A
              │   OS service / launchd│
              │   / systemd + PID     │
              └───────────────────────┘
```

Shared data (unchanged):

| Path | Purpose |
|------|---------|
| `config.toml` | Rules, watch paths, dedup strategy |
| `{data_dir}/index.bin` | File index |
| `{data_dir}/activity.jsonl` | Audit log |
| `{data_dir}/flux.log` | Daemon log |
| `{data_dir}/paused` | Tray pause flag (Phase B) |
| `{data_dir}/flux.stop` | Graceful shutdown request |
| `{data_dir}/service.installed` | OS integration marker |
| `{data_dir}/trash/` | Dedup trash |

---

## Phase A — Background Agent & OS Integration

**User story:** *"I install FluxFS once. It starts when I log in and organizes my Downloads silently."*

### Milestones

| ID | Deliverable | Acceptance criteria |
|----|-------------|---------------------|
| A1 | Daemon file logging | `{data_dir}/flux.log` receives tracing output when not foreground |
| A2 | Graceful shutdown (all OS) | `flux stop` saves index; no hard `TerminateProcess` on Windows |
| A3 | `flux start --daemon` | Hidden flag for service managers; no console output |
| A4 | Detached `flux start` | `flux start` spawns background process without `--foreground` |
| A5 | `flux install-service` | Registers auto-start per platform |
| A6 | `flux uninstall-service` | Removes registration; optional keep config/data |
| A7 | `flux status` service info | Shows installed / running / mode (service \| detached \| foreground) |
| A8 | Tests | Unit tests for unit/plist/registry content; integration smoke where possible |

### Platform integration

| OS | Mechanism | Install location |
|----|-----------|------------------|
| **Linux** | systemd user unit | `~/.config/systemd/user/fluxfs.service` |
| **macOS** | LaunchAgent | `~/Library/LaunchAgents/com.fluxfs.daemon.plist` |
| **Windows** | Logon Run registry + detached process | `HKCU\...\Run` (Phase A); Windows Service (Phase A+ polish) |

Commands:

```bash
flux install-service      # register + enable + start
flux uninstall-service    # stop + unregister
flux start                # start service if installed, else detached daemon
flux start --foreground   # debug / dev mode (terminal attached)
flux stop                 # graceful shutdown
flux status               # includes service state
```

### Out of scope for Phase A

- Tray icon
- GUI settings
- `.msi` / `.dmg` installers (Phase C)
- Auto-update

**Estimated effort:** 2–3 weeks

---

## Phase B — System Tray App

**User story:** *"I see a small icon showing FluxFS is running. I can pause it or open recent activity without the terminal."*

### Milestones

| ID | Deliverable | Acceptance criteria |
|----|-------------|---------------------|
| B1 | Tray binary | New crate `fluxfs-tray` or feature-gated binary |
| B2 | Icon states | Running (green), Paused (yellow), Error (red) |
| B3 | Menu actions | Open log folder, Pause/Resume watcher, Run organize now, Quit |
| B4 | IPC to daemon | Pause/resume via shared flag file or local socket |
| B5 | Launch at login | Tray starts with session (bundled with service install) |
| B6 | Cross-platform tray | `tray-icon` + `winit` or **Tauri** system tray |

### Tech (implemented)

**`tray-icon` + `winit`** for the tray (`src/bin/tray.rs`). **eframe + egui** for settings (`src/gui/`). File-based IPC via `{data_dir}/paused` — no Tauri/WebView stack.

### Original options considered

| Option | Notes |
|--------|-------|
| Tauri 2 | Deferred — egui chosen for smaller deps and in-crate layout |
| egui + tray | **Chosen** — single Rust stack |

### Out of scope for Phase B

- Full rule editor (Phase D)
- Installers (Phase C)

**Estimated effort:** 2 weeks

---

## Phase C — Installers & Distribution

**User story:** *"I download FluxFS from GitHub or a website, double-click install, done."*

### Milestones

| ID | Deliverable | Acceptance criteria |
|----|-------------|---------------------|
| C1 | Release artifacts | Signed/stamped binaries per OS (extend existing CI) |
| C2 | Windows installer | `.msi` or `.exe` (WiX / NSIS / cargo-wix) — installs binary + service + tray |
| C3 | macOS installer | `.dmg` with drag-to-Applications; optional `.pkg` |
| C4 | Linux packages | `.deb` and/or Flatpak / AppImage |
| C5 | Post-install hook | Runs `flux install-service` + creates default config |
| C6 | Uninstaller | Removes service, tray, binaries; preserves user data option |
| C7 | Code signing | macOS notarization; Windows Authenticode (stretch) |

### CI changes

Extend `.github/workflows/ci.yml` release job:

- Build `flux`, `fluxfs`, `fluxfs-tray`
- Package per OS
- Attach to GitHub Release on tag `v0.2.0`

**Estimated effort:** 2–3 weeks

---

## Phase D — Settings GUI

**User story:** *"I pick folders and rules in a simple window — no TOML editing."*

### Milestones

| ID | Deliverable | Acceptance criteria |
|----|-------------|---------------------|
| D1 | Settings window | Open from tray → "Settings" |
| D2 | Watch folders | Add/remove watch paths (folder picker) |
| D3 | Rule editor | Pattern + destination rows; preview matches |
| D4 | Dedup settings | Strategy, min/max size |
| D5 | Activity viewer | Last N moves from `activity.jsonl` |
| D6 | Status dashboard | File count, daemon uptime, weekly stats (reuse `status.rs` data) |
| D7 | Dry-run preview | "Test rules" on current Downloads |

### Tech (implemented)

**eframe + egui** in-crate GUI (`src/gui/`, binary `fluxfs-settings`):

- Pure Rust — no Node/WebView stack
- Config read/write through existing `config/parser.rs` (`save_user_config`)
- Tray launches settings via `settings_binary_path()` / `flux settings` fallback

### Original recommendation (not used)

**Tauri 2 + React/Svelte** in `crates/fluxfs-app/` — deferred; egui chosen for smaller deps and shared crate layout.

### Out of scope for v0.2.0

- Cloud sync
- Multi-user / team features
- Mobile

**Estimated effort:** 3–4 weeks

---

## Version & release plan

| Version | Contents | Tag target |
|---------|----------|------------|
| **v0.2.0-alpha.1** | Phase A complete | Background service |
| **v0.2.0-alpha.2** | Phase B complete | Tray app |
| **v0.2.0-beta.1** | Phase C complete | Installers |
| **v0.2.0** | Phase D complete | Full desktop product |

---

## Implementation status (v0.2.0)

All phases A–D complete. See [fluxfs-architecture.md](../fluxfs-architecture.md#phase-9-desktop-application-v02) for technical detail.

### Phase A — done

1. ✅ Roadmap document (this file)
2. ✅ Graceful shutdown via shutdown request file (Windows + Unix)
3. ✅ Daemon file logging (`flux.log`)
4. ✅ `flux start --daemon` internal mode
5. ✅ Detached `flux start` (spawn background child)
6. ✅ `src/service/` — systemd, launchd, Windows registry
7. ✅ `flux install-service` / `flux uninstall-service`
8. ✅ `flux status` service section
9. ⬜ Cross-platform service integration tests in CI (manual smoke on each OS — optional follow-up)

### Phase B — done

1. ✅ `fluxfs-tray` binary with tray-icon + winit
2. ✅ IPC: pause/resume flag in `{data_dir}/paused`
3. ✅ Menu: status icon, open folders, organize, quit
4. ✅ Wired into `install-service` (tray starts at login)

### Phase C — done

1. ✅ `flux setup` command + [docs/INSTALL.md](../docs/INSTALL.md)
2. ✅ Windows NSIS setup.exe (`packaging/windows/`)
3. ✅ macOS DMG builder (`packaging/macos/build-dmg.sh`)
4. ✅ Linux `.deb` via cargo-deb + maintainer scripts
5. ✅ GitHub Release CI (deb, dmg, exe, tarballs)

### Phase D — done

1. ✅ **`fluxfs-settings`** binary — egui/eframe settings window (Status, Watch & Rules, Dedup, Activity)
2. ✅ **`flux settings`** CLI + tray **Settings…** menu item
3. ✅ Rule editor with pattern + destination rows; folder pickers (`rfd`)
4. ✅ Activity viewer + status dashboard (reuses `status.rs` / `activity.jsonl`)
5. ✅ **Test rules** — dry-run organize from GUI; config save via `save_user_config()`

---

## Risks & mitigations

| Risk | Mitigation |
|------|------------|
| Moving files mid-download | Debounce + skip `.crdownload`, `.part`, `.tmp` patterns |
| User distrust of auto-move | Default dry-run option in GUI; trash strategy; activity log |
| Windows Defender SmartScreen | Code signing (Phase C); README install instructions |
| macOS notarization | Required for smooth `.dmg` install (Phase C) |
| Service permissions | User-level services only (no admin) in Phase A |

---

## Success criteria (v0.2.0 GA)

| # | Criterion | Status |
|---|-----------|--------|
| 1 | User installs via installer (no Rust required) | ✅ |
| 2 | FluxFS starts at login and runs in background | ✅ |
| 3 | New Downloads files are organized within seconds | ✅ |
| 4 | Tray shows running state; pause/resume works | ✅ |
| 5 | Settings GUI edits rules without TOML | ✅ |
| 6 | CLI remains fully functional for power users | ✅ |
| 7 | All existing integration tests pass | ✅ (88 unit + 14 integration) |
| 8 | CI green on Linux, macOS, Windows | ⚠️ fmt fixed; clippy green on Windows; release packaging succeeds |

**Post-GA polish:** code signing, macOS notarization, dedicated service smoke tests in CI.

---

## Links

- [README](../README.md)
- [CHANGELOG](../CHANGELOG.md)
- [CONTRIBUTING](../CONTRIBUTING.md)
- [GitHub](https://github.com/ManeeshJupalle/FluxFS)
