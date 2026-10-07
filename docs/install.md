# Install and uninstall

CleverShim is a Windows tool. The release asset is both the CLI and the installer.

## Install with WinGet

Once the community package is published:

```powershell
winget install Clevotec.CleverShim
```

The WinGet manifest uses `InstallerType: exe` with silent switch `/install`.

## Install from a GitHub Release

Download `clevershim-x86_64-pc-windows-msvc.exe` (or the ARM64 build) from the [releases page](https://github.com/clevotec/clevershim/releases), then:

```powershell
.\clevershim-x86_64-pc-windows-msvc.exe /install
# or
.\clevershim-x86_64-pc-windows-msvc.exe setup
```

### Per-user install (default, no elevation)

- Copies the manager to `%LOCALAPPDATA%\Clevotec\CleverShim\clevershim.exe`
- Copies `clevershim.exe` into `%LOCALAPPDATA%\Clevotec\CleverShim\bin\` so the command is on PATH
- Stores the shim stub as `clevershim-shim.exe` next to the manager
- Appends `%LOCALAPPDATA%\Clevotec\CleverShim\bin` to the **user** PATH (never prepends)
- Runs `sync` for user-scope WinGet packages
- Registers a logon scheduled task that runs `clevershim sync`
- Writes an HKCU Uninstall key (`Clevotec.CleverShim`)

### Elevated install

When the installer is elevated, it also:

- Creates `%ProgramData%\Clevotec\CleverShim\bin`
- Appends that directory to the **system** PATH
- Syncs machine-scope WinGet packages
- Writes an HKLM Uninstall key

A per-user install never changes the system PATH.

## Uninstall

```powershell
clevershim /uninstall
# or
clevershim uninstall
```

That removes the logon task, PATH entry, shims, and Uninstall key for the scopes the process can write. When uninstall runs from the installed manager, a hidden cleanup process deletes the locked executable after exit.

You can also uninstall from Windows Settings / Apps using the Uninstall entry CleverShim registers.

## Verify

Open a **new** terminal after install (PATH changes need a new process):

```powershell
where.exe clevershim
clevershim list
clevershim sync
```

## Layout

| Path | Purpose |
| --- | --- |
| `%LOCALAPPDATA%\Clevotec\CleverShim\` | Per-user manager root |
| `%LOCALAPPDATA%\Clevotec\CleverShim\bin\` | Per-user shims + `clevershim.exe` on PATH |
| `%ProgramData%\Clevotec\CleverShim\` | Machine manager root (elevated) |
| `%ProgramData%\Clevotec\CleverShim\bin\` | Machine shims |
| `%LOCALAPPDATA%\clevershim\packages.yaml` | Optional user-extra catalog |
| `%LOCALAPPDATA%\Clevotec\CleverShim\overrides.yaml` | Per-user overrides |
| `%ProgramData%\Clevotec\CleverShim\overrides.yaml` | Machine overrides |
