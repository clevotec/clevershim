# Local helper scripts

These helpers are for local Windows Sandbox experiments.

**Prefer** `microsoft/winget-pkgs` `Tools/SandboxTest.ps1` against the submitted manifests (see [docs/sandbox-testing.md](../docs/sandbox-testing.md)). That runs `winget install --manifest` inside Sandbox, which matches WinGet validation much more closely than launching the release exe with `/install`.

| Script | Notes |
| --- | --- |
| `check-sandbox.ps1` | Confirms Windows Sandbox is available |
| `download-installer.ps1` / `launch-sandbox.ps1` / `clevershim-sandbox.wsb` | Older direct `/install` harness; useful only as a smoke test, not as WinGet validation |
| `sandbox-share/` | Scratch share for Sandbox (gitignored) |
