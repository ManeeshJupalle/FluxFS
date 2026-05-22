# Contributing to FluxFS

Thanks for your interest in FluxFS! This project is maintained by [Maneesh Jupalle](mailto:maneeshreddy28@gmail.com).

## Development setup

1. Install [Rust](https://rustup.rs/) (stable toolchain).
2. Clone the repository and enter the directory.
3. On Windows, ensure MSVC Build Tools are installed for linking.

```bash
git clone https://github.com/ManeeshJupalle/FluxFS.git
cd FluxFS
cargo build
```

Both `flux` and `fluxfs` binaries are built from the same entry point.

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

- **Unit tests** live alongside modules (`#[cfg(test)]`).
- **Integration tests** live in `tests/integration.rs` and exercise the full CLI.

When fixing a bug, add a regression test if feasible.

## Reporting issues

Include:

- OS and Rust version (`rustc --version`)
- Config (redact personal paths if needed)
- Steps to reproduce
- Expected vs actual behavior

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
