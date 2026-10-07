# Development

## Prerequisites

- Stable Rust toolchain (`rust-toolchain.toml`)
- On Windows: MSVC C++ tools and Windows SDK for release builds and the Windows integration test

## Build and test

```bash
cargo test
cargo build --release
```

Release binaries: `clevershim` / `clevershim-shim` (on Windows, `.exe`).

Full release-mode suite, including the optional SQLite-backed scanner:

```bash
cargo test --release --locked --all-features --no-fail-fast
cargo build --release --locked --all-features
```

Windows shim repair coverage:

```bash
cargo test --release --test windows_shim
```

CI also runs `cargo fmt --check`, `cargo clippy -D warnings`, and `cargo audit`.

## Catalog scan feature

The `scan` feature enables `clevershim-scan` (`rusqlite`, `zip`, `reqwest`).

## Git commit email guard

```bash
git config --local core.hooksPath .githooks
```

The versioned `pre-commit` hook rejects author/committer emails outside `@users.noreply.github.com`. Activation is local and bypassable with `--no-verify`.

## Project layout

| Path | Purpose |
| --- | --- |
| `src/main.rs` | CLI entry |
| `src/bin/shim.rs` | Shim stub |
| `src/bin/scan.rs` | Catalog scanner |
| `src/install.rs` | Setup / uninstall / sync commands |
| `src/sync.rs` | Shim create / retarget / remove |
| `src/resolve.rs` | Find the real target file |
| `src/repair.rs` | One-shot retarget logic |
| `src/platform.rs` | Windows PATH, tasks, uninstall keys |
| `catalog/` | Embedded packages, collisions, overrides |
| `.github/workflows/` | CI, release, WinGet submit |

## Native installer checks

Unit tests do not fully exercise OS install effects. Manually verify:

- Standard-user and elevated setup
- Registry PATH entries and Uninstall keys
- Logon task registration
- Installed-binary uninstall
- `where clevershim` after a new shell
