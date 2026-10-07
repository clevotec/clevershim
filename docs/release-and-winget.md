# Release and WinGet

## GitHub Release

Tag `v*` (or run the Release workflow manually with that tag) to:

1. Scan the public WinGet source and embed `catalog/packages.yaml`
2. Build `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`
3. Zip each build with a SHA256 file, attest provenance, and publish a GitHub Release

The x64 exe is the WinGet installer (`Clevotec.CleverShim`). Windows version metadata sets `FileDescription` to contain `installer` so WinGet treats the asset as an EXE installer. Silent switch: `/install`.

## WinGet submission workflow

After a published release, `.github/workflows/winget.yml` (also runnable via `workflow_dispatch` with a tag):

1. Downloads the public x64 exe and hashes it
2. Syncs the `clevotec1/winget-pkgs` fork to `microsoft/winget-pkgs`
3. Writes manifests under `manifests/c/Clevotec/CleverShim/<version>/`
4. Opens a PR to `microsoft/winget-pkgs` when one is not already open for that version

Requirements:

1. Public repository and public release assets
2. Repository secret `WINGET_TOKEN`: classic PAT for `clevotec1` with `public_repo`
3. Fork `clevotec1/winget-pkgs`

Without those, the GitHub Release still publishes and the winget job exits after reporting what is missing.

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

For CleverShim that usually means:

1. `/install` took too long because sync re-scanned package folders / Uninstall registry once per catalog package (fixed by indexing those once per sync)
2. `Commands: clevershim` while only `bin\` was on PATH and the manager lived in the parent folder (fixed by also installing `clevershim.exe` into `bin\`)

After fixing the installer binary, publish a new tag and update or re-open the `microsoft/winget-pkgs` PR with the new URL and SHA256.
