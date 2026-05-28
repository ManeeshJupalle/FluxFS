# FluxFS — Installation Guide

Download installers from [GitHub Releases](https://github.com/ManeeshJupalle/FluxFS/releases).

## Windows

1. Download `FluxFS-*-windows-x86_64-setup.exe`
2. Run the installer (no admin required — installs to `%LOCALAPPDATA%\Programs\FluxFS`)
3. The installer runs `flux setup` automatically (scans Downloads, registers auto-start + tray)
4. Look for the **FluxFS tray icon** in the system tray after install
5. Open **Settings…** from the tray (or run `flux settings`) to edit watch folders and rules

**Uninstall:** Settings → Apps → FluxFS, or run `%LOCALAPPDATA%\Programs\FluxFS\Uninstall.exe`

## macOS

1. Download `FluxFS-*-macos-x86_64.dmg`
2. Open the DMG and drag **FluxFS** to **Applications**
3. Open Terminal and run:
   ```bash
   /Applications/FluxFS/flux setup
   ```
   Or double-click **Setup.command** inside the FluxFS folder.

**Uninstall:** Delete `/Applications/FluxFS`, then run `flux uninstall-service` if still on PATH.

## Linux (Debian / Ubuntu)

1. Download `fluxfs_*_amd64.deb`
2. Install:
   ```bash
   sudo dpkg -i fluxfs_*_amd64.deb
   sudo apt-get install -f   # resolve dependencies if needed
   ```
3. If post-install did not run as your user:
   ```bash
   flux setup
   ```

**Uninstall:** `sudo apt remove fluxfs` (runs `flux uninstall-service`; keeps config and index)

---

## What `flux setup` does

1. **`flux init`** — creates config, scans watch folders, builds index
2. **`flux install-service`** — registers daemon + tray at login (systemd / LaunchAgent / Run key)

Run manually after any install method:

```bash
flux setup
```

Options:

```bash
flux setup --skip-init      # only register auto-start
flux setup --skip-service   # only scan/index
flux setup --quiet          # installer / script mode
```

---

## Build installers from source

Requires Rust stable.

### Windows

```powershell
.\packaging\windows\build-installer.ps1
# Output: dist\FluxFS-*-windows-x86_64-setup.exe
```

Install [NSIS](https://nsis.sourceforge.io/) first.

### macOS

```bash
chmod +x packaging/macos/build-dmg.sh
./packaging/macos/build-dmg.sh
# Output: dist/FluxFS-*-macos-x86_64.dmg
```

### Linux

```bash
sudo apt-get install libgtk-3-dev libxdo-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev   # tray + settings GUI
chmod +x packaging/linux/build-deb.sh
./packaging/linux/build-deb.sh
# Output: dist/fluxfs_*_amd64.deb
```

---

## See also

- [fluxfs-architecture.md](../fluxfs-architecture.md) — engine + desktop architecture (Phase 9)
- [README.md](../README.md) — command reference and quick start
- [CHANGELOG.md](../CHANGELOG.md) — release history

```bash
git clone https://github.com/ManeeshJupalle/FluxFS.git
cd FluxFS
cargo install --path .
flux setup
```
