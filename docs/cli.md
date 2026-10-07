# CLI reference

```text
clevershim <command>
```

`/install` and `/uninstall` are accepted as Windows-style switches (any argument position) and map to `setup` / `uninstall`.

## `setup`

Copy CleverShim into place, append PATH, sync shims, register the logon task, and write the Uninstall key.

## `uninstall`

Remove the logon task, PATH entry, shims, and Uninstall key for writable scopes.

## `sync`

Refresh shims for every catalog command in each writable scope (user always; machine when the process can write ProgramData).

- Creates or updates `bin\<command>.exe` + `bin\<command>.shim` when an installed package provides the command
- Retargets when a higher-priority provider is installed
- Removes a shim only when no installed provider in that scope remains

## `list`

Print shims found in the user and machine bins:

```text
<scope>  <command>  <package-id>  <target>  ok|missing
```

## `repair`

Rewrite one shim whose target moved, or remove it when nothing provides the command.

| Flag | Meaning |
| --- | --- |
| `--name <command>` | Repair by command name (for example `ffmpeg`) |
| `--shim <path>` | Repair a specific shim exe path |
| `--explain` | Print the repair report and still apply unless `--dry-run` |
| `--dry-run` | Print the report without changing the sidecar |
| `--from-shim` | Called by the stub; success stays silent |

## `hook install` / `hook remove`

Register or remove the per-user logon scheduled task (`Clevotec CleverShim Sync`) that runs `clevershim sync` at logon.

## Shim stub flags

Shims accept diagnostic flags that print the mapping and do not launch the target:

- `--clevershim-noop`
- `--clevershim-test`

A successful normal shim launch prints nothing of its own. Repair diagnostics go to stderr only when repair fails.
