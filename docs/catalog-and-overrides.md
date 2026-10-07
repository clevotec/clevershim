# Catalog and overrides

## Embedded catalog

`catalog/packages.yaml` is generated from the public WinGet source index (not a full clone of `winget-pkgs`) and compiled into the manager with `include_str!`.

Regenerate:

```bash
cargo run --release --features scan --bin clevershim-scan -- --out catalog/packages.yaml
```

### Scan rules

- Latest manifest per package
- Include when an installer is `portable`, or a zip whose nested installer is `portable`, and `Commands` is non-empty
- Each command becomes a shim name; the PATH file is always `<command>.exe`
- Resolve looks for `<command>.exe`, then `.bat`, then `.cmd`, unless the command already names an extension
- `Clevotec.CleverShim` is excluded so the tool does not shim itself
- Packages with `ArchiveBinariesDependOnPath: true` are kept; the flag is recorded and used at launch

`catalog/collisions.yaml` lists commands that more than one package provides.

## User extra packages

`%LOCALAPPDATA%\clevershim\packages.yaml` can add packages. `sync` still creates a shim only when that package is installed.

## Overrides

`catalog/overrides.yaml` (embedded) sets:

- Shim-target priority per command (for example `ffmpeg`: `yt-dlp.FFmpeg`, then `Gyan.FFmpeg`, then others)
- Force-includes (for example Gyan FFmpeg tools when the heuristic misses them)
- Force-excludes

Layering:

1. Embedded overrides
2. `%ProgramData%\Clevotec\CleverShim\overrides.yaml` (machine)
3. `%LOCALAPPDATA%\Clevotec\CleverShim\overrides.yaml` (user only, on top of machine)

Priority picks which installed provider a shim launches. It does **not** move CleverShim ahead of an earlier dedicated PATH entry.
