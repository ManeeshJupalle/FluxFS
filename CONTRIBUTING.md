# Contributing to FluxFS

Thanks for your interest in FluxFS! This project is maintained by [Maneesh Jupalle](mailto:maneeshreddy28@gmail.com).

## Development setup

1. Install [Rust](https://rustup.rs/) (stable toolchain).
2. Clone the repository and enter the directory.
3. On **Windows**, ensure MSVC Build Tools are installed for linking.
4. On **Linux**, install GUI dependencies for `fluxfs-tray` and `fluxfs-settings`:

```bash
sudo apt-get install libgtk-3-dev libxdo-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev
```

```bash
git clone https://github.com/ManeeshJupalle/FluxFS.git
cd FluxFS
cargo build --bins
```

### Binaries

| Binary | Source | Purpose |
|--------|--------|---------|
| `flux` / `fluxfs` | `src/main.rs` | CLI (same code, two names) |
| `fluxfs-tray` | `src/bin/tray.rs` | System tray |
| `fluxfs-settings` | `src/bin/settings.rs` | Settings GUI |

Build all four: `cargo build --release --bins`

## Before opening a PR

Run all checks locally:

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

On Windows PowerShell, `cargo` is typically at `%USERPROFILE%\.cargo\bin\cargo.exe`.

Integration tests use isolated temp directories via the `FLUXFS_CONFIG` environment variable — they do not modify your user config.

## Code style

- Match existing module structure and naming.
- Prefer proper error propagation over `unwrap()` in non-test code.
- User-facing errors should include actionable hints where possible.
- Keep changes focused — one logical fix or feature per PR.

## Tests

- **Unit tests** live alongside modules (`#[cfg(test)]`) — 88 tests across the crate.
- **Integration tests** live in `tests/integration.rs` — 14 end-to-end CLI scenarios.

When fixing a bug, add a regression test if feasible.

## Packaging (maintainers)

Release builds and installers are documented in [docs/INSTALL.md](docs/INSTALL.md#build-installers-from-source):

- Windows: `packaging/windows/build-installer.ps1` (requires NSIS)
- macOS: `packaging/macos/build-dmg.sh`
- Linux: `packaging/linux/build-deb.sh`

GitHub Releases are built by CI on tag publish; see `.github/workflows/ci.yml` and `packaging/github/prepend-download-links.sh`.

Architecture overview: [fluxfs-architecture.md](fluxfs-architecture.md) (Phase 9 = desktop layer).

## Reporting issues

Include:

- OS and Rust version (`rustc --version`)
- Install method (installer, source, crates.io)
- Config (redact personal paths if needed)
- Steps to reproduce
- Expected vs actual behavior

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
