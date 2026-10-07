# CleverShim

Stable PATH shims for WinGet portable packages.

WinGet drops portable tools in versioned folders, and the symlink it would add often fails without Developer Mode, so the command disappears after install or moves on upgrade. CleverShim puts a small console shim on a stable PATH directory. When the target is gone, the stub asks `clevershim` to repair the sidecar once and then starts the new file.

The stub is original Rust. It does not copy Chocolatey shimgen.

**Package id:** `Clevotec.CleverShim`

## Quick start

```powershell
# From a GitHub Release asset
.\clevershim-x86_64-pc-windows-msvc.exe /install

# After install (new terminal)
clevershim sync
clevershim list
```

Or, once published to the community repository:

```powershell
winget install Clevotec.CleverShim
```

## Documentation

| Guide | Contents |
| --- | --- |
| [Install and uninstall](docs/install.md) | WinGet / release install, layout, verification |
| [CLI reference](docs/cli.md) | `setup`, `sync`, `repair`, `list`, hooks, shim flags |
| [How it works](docs/how-it-works.md) | Stub, sidecar, repair rules, PATH policy, DLLs |
| [Catalog and overrides](docs/catalog-and-overrides.md) | Scan rules, collisions, priority, user files |
| [Development](docs/development.md) | Build, test, hooks, source map |
| [Release and WinGet](docs/release-and-winget.md) | Tags, manifests, submission, validation |
| [Security policy](SECURITY.md) | Vulnerability reporting |

## Behavior in brief

- Two bins: user (`%LOCALAPPDATA%\Clevotec\CleverShim\bin`) and machine (`%ProgramData%\Clevotec\CleverShim\bin`)
- PATH entries are appended, never prepended
- One shim per command per scope; priority picks the provider to launch, not PATH order
- `.bat` / `.cmd` targets run through `cmd.exe /c`; `.ps1` is not shimmed
- Logon task runs `clevershim sync` for the installing user (not startup, not a timer)
- Successful shim launches are silent

## Build

Install a stable Rust toolchain, then:

```bash
cargo test
cargo build --release
```

That produces `clevershim` and `clevershim-shim` (`clevershim.exe` / `clevershim-shim.exe` on Windows).

Regenerate the embedded catalog from the public WinGet source index:

```bash
cargo run --release --features scan --bin clevershim-scan -- --out catalog/packages.yaml
```

`catalog/packages.yaml` is compiled into the manager with `include_str!`. See [catalog and overrides](docs/catalog-and-overrides.md).

## Git commit email guard

```bash
git config --local core.hooksPath .githooks
```

Contributors should use their own GitHub noreply address. The `pre-commit` hook rejects emails outside `@users.noreply.github.com`. Hook activation is local, bypassable with `--no-verify`, and not server-side enforcement.

## License

[MIT](LICENSE)
