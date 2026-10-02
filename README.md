# CleverShim

CleverShim puts a small console shim on a stable PATH directory for winget portable packages. Winget drops those tools in versioned folders, and the symlink it would add often fails without Developer Mode, so the command disappears after install or moves on upgrade. The shim stays put. When its target is gone, it asks `clevershim` to repair the sidecar once and then starts the new file.

The stub is original Rust. It does not copy Chocolatey shimgen.

## Build

Install a stable Rust toolchain, then:

```bash
cargo test
cargo build --release
```

That produces `clevershim` and `clevershim-shim`. On Windows the release files are `clevershim.exe` and `clevershim-shim.exe`.

Regenerate the embedded catalog from the public WinGet source index (not a clone of winget-pkgs):

```bash
cargo run --release --features scan --bin clevershim-scan -- --out catalog/packages.yaml
```

`catalog/packages.yaml` is compiled into the manager with `include_str!`. `catalog/collisions.yaml` lists commands that more than one package provides. `catalog/overrides.yaml` sets shim-target priority and force-includes `Gyan.FFmpeg`.

## Git commit email guard

Enable the versioned hook in each clone:

```bash
git config --local core.hooksPath .githooks
git config --local user.email "8874908+kcrkor@users.noreply.github.com"
```

Other contributors should use their own GitHub noreply address. The `pre-commit` hook checks the effective author and committer emails, including environment and `--author` overrides, and rejects addresses outside `@users.noreply.github.com`. Both `username@users.noreply.github.com` and `ID+username@users.noreply.github.com` are accepted.

Hook activation is local configuration and does not propagate when pushed or cloned. This guard runs for `git commit`; it is bypassable with `--no-verify` and is not server-side enforcement.

## Install and commands

`clevershim.exe` is both the CLI and the installer.

- `clevershim setup` or `clevershim /install` copies the exe under `%LOCALAPPDATA%\Clevotec\CleverShim\`, appends `%LOCALAPPDATA%\Clevotec\CleverShim\bin` to the user PATH, syncs that user's packages, registers a logon task that runs `clevershim sync`, and writes an HKCU uninstall key.
- An elevated install also creates `%ProgramData%\Clevotec\CleverShim\bin`, appends it to the system PATH, syncs machine-scope packages, and writes an HKLM uninstall key. A per-user install does not change the system PATH.
- `clevershim /uninstall` removes the task, the PATH entry, that scope's shims, and its uninstall key.
- `clevershim sync` refreshes shims. `clevershim repair` rewrites one sidecar. `clevershim list` prints them. `clevershim hook install` and `clevershim hook remove` manage the logon task.

The logon task runs as the user who logged on. It does not run at startup and it does not run on a timer. Machine-bin writes take a file lock. Directories are appended to PATH, never prepended, so an earlier dedicated `ffmpeg.exe` still wins while it exists.

Each shim is `bin\<command>.exe` plus `bin\<command>.shim`. A `.bat` or `.cmd` target is started through `cmd.exe /c`. `.ps1` is not shimmed. A successful shim call prints nothing of its own. Repair details go to stderr only when repair fails.

A user file at `%LOCALAPPDATA%\clevershim\packages.yaml` can add packages. Overrides in `%ProgramData%\Clevotec\CleverShim\overrides.yaml` apply to the machine. `%LOCALAPPDATA%\Clevotec\CleverShim\overrides.yaml` applies only to that user.

## Tests

`cargo test` covers catalog parsing, package-directory matching, sidecar rewrite, one repair attempt, refusal to retarget a shim at itself or at `bin`, shim removal when a package is absent, and user overrides staying with that user.

On Windows, `cargo test --release --test windows_shim` builds a fixture exe that needs a sibling DLL, deletes the original target, and checks that the same launch retargets and loads the DLL.

## Release

Tag `v*` to build `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`, zip each with a SHA256 file, and publish a GitHub Release. The x64 exe is the winget installer (`Clevotec.CleverShim`, silent switch `/install`).

The winget workflow installs [Komac](https://github.com/russellbanks/Komac) and runs `komac sync`, then `komac new` for the first release or `komac update` after that. It needs a classic PAT in the `WINGET_TOKEN` secret (`public_repo`) and a `microsoft/winget-pkgs` fork on the same account. Without the secret the GitHub Release still publishes and the Komac job reports that the token is missing.
