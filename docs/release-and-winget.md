# Release and WinGet

## GitHub Release

Tag `v*` (or run the Release workflow manually with that tag) to:

1. Scan the public WinGet source and embed `catalog/packages.yaml`
2. Build `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`
3. Zip each build with a SHA256 file, attest provenance, and publish a GitHub Release

The x64 exe is the WinGet installer (`Clevotec.CleverShim`). Windows version metadata sets `FileDescription` to contain `installer` so WinGet treats the asset as an EXE installer. Silent switch: `/install`.

## WinGet submission workflow

WinGet package submissions and all comments/PR actions on `microsoft/winget-pkgs` use the **`clevotec1` account exclusively**. Do not open, comment on, reopen, or close CleverShim WinGet PRs from any other GitHub account.

After a published release, `.github/workflows/winget.yml` (also runnable via `workflow_dispatch` with a tag) authenticates as `clevotec1` via `WINGET_TOKEN` and:

1. Downloads the public x64 exe and hashes it
2. Syncs the `clevotec1/winget-pkgs` fork to `microsoft/winget-pkgs`
3. Writes manifests under `manifests/c/Clevotec/CleverShim/<version>/`
4. Opens or updates a PR to `microsoft/winget-pkgs` for that version
5. Posts CLA agreement only when `Needs-CLA` is present and no prior `clevotec1` agree comment exists
6. Closes older open CleverShim PRs for other versions

Requirements:

1. Public repository and public release assets
2. Repository secret `WINGET_TOKEN`: classic PAT for **`clevotec1` only** (`public_repo`)
3. Fork `clevotec1/winget-pkgs`

Without those, the GitHub Release still publishes and the winget job exits after reporting what is missing.

Local `gh` sessions for other accounts must not be used against CleverShim WinGet PRs. Re-run the `winget` workflow instead.

## Package identity

| Field | Value |
| --- | --- |
| PackageIdentifier | `Clevotec.CleverShim` |
| InstallerType | `exe` |
| Scope | `user` |
| Silent / SilentWithProgress | `/install` |
| Commands | `clevershim` |

## Validating a manifest locally

```powershell
winget validate --manifest path\to\manifests\c\Clevotec\CleverShim\<version>
winget install --manifest path\to\manifests\c\Clevotec\CleverShim\<version>
```

Prefer Windows Sandbox or [SandboxTest.ps1](https://github.com/microsoft/winget-pkgs/blob/master/tools/SandboxTest.md) for an isolated install.

## Known validation failure: unattended install

WinGet labels `Validation-Unattended-Failed` when the installer times out or appears to need user input.

### Root cause on CleverShim 0.1.1 (PR [#448126](https://github.com/microsoft/winget-pkgs/pull/448126))

Instrumented Windows Sandbox (`winget install --manifest` + 30s process watcher) showed:

1. `clevershim-x86_64-pc-windows-msvc.exe` **did** launch under `%LOCALAPPDATA%\Temp\WinGet\...`
2. A modal window titled **`clevershim-x86_64-pc-windows-msvc.exe - System Error`** stayed open (csrss / Hard Error dialog)
3. `winget` waited on that installer process → pipeline/SandboxTest hang for hours

`objdump -p` on the 0.1.1 release asset imports **`VCRUNTIME140.dll`** (dynamic MSVC CRT). Hosts with the VC++ redistributable succeed in ~25s; a clean sandbox/validation agent does not, and the missing-DLL dialog looks like "waiting for user input".

**Fix for 0.1.2:** release builds use static CRT (`-C target-feature=+crt-static` in `.cargo/config.toml` for `*-pc-windows-msvc` targets) so the installer has no `VCRUNTIME140.dll` dependency. Keep `/install` synchronous (same as 0.1.1) so the sandbox rerun has one variable. CI (windows job) and Release both run `dumpbin /dependents` on `clevershim.exe` and `clevershim-shim.exe` and fail on `VCRUNTIME|MSVCP|api-ms-win-crt`, so a later `RUSTFLAGS` override cannot silently restore dynamic CRT linkage. CI also uploads `clevershim-msvc-installer` so SandboxTest can prove the MSVC artifact before cutting a tag.

Also fixed earlier: `Commands: clevershim` while only `bin\` was on PATH (0.1.1 installs `clevershim.exe` into `bin\`). Manifest template for 0.1.2 drops unused `interactive` InstallMode and adds `AppsAndFeaturesEntries` for `Clevotec.CleverShim`.

The submit workflow updates manifests in place on the version branch. It must not force-reset that branch to `master` while a PR exists: an empty PR is labeled `Unexpected-File` and closed. Re-runs reopen a closed PR for the same version and close only older open PRs for other versions.
