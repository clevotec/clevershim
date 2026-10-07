# Windows Sandbox proving ground

Direct `/install` of the release exe in a clean sandbox is **not** a good proxy for WinGet validation. A fresh sandbox has almost no Uninstall keys or WinGet packages, so CleverShim's sync finishes quickly and timing looks better than the validator's machine.

Use the official [SandboxTest.ps1](https://github.com/microsoft/winget-pkgs/blob/master/Tools/SandboxTest.ps1) from `microsoft/winget-pkgs` against the submitted manifests. That path installs WinGet in the sandbox and runs `winget install --manifest`, which is what the community validator exercises.

## Prerequisites

- Windows Sandbox enabled (`WindowsSandbox.exe`)
- Host `winget` available (for optional local `winget validate`)
- Prefer PowerShell 7 (`pwsh`) for upstream SandboxTest; on Windows PowerShell 5.1 the script needs small compatibility patches (`Out-File -FilePath`, HEAD null handling, Bearer auth)

## Recommended flow

1. Fetch the PR manifests from the `clevotec1/winget-pkgs` branch (for example `clevotec-clevershim-0.1.1`).
2. Validate on the host:

```powershell
winget validate --manifest path\to\manifests\c\Clevotec\CleverShim\0.1.1
```

3. From a checkout of `microsoft/winget-pkgs` `Tools` (or a PS 5.1-patched local copy):

```powershell
.\SandboxTest.ps1 -Manifest path\to\0.1.1 -MapFolder path\to\results -WarningAction Continue -Script {
  # write host-visible RESULT=PASS/FAIL under Desktop\results
}
```

SandboxTest will:

- mount the manifest + WinGet bootstrap payload
- install WinGet inside the sandbox
- run `winget install -m <manifest> --accept-package-agreements ...`
- refresh PATH / compare ARP
- run the optional `-Script`

## Local helpers in this repo

| Path | Purpose |
| --- | --- |
| `scripts/check-sandbox.ps1` | Confirm Sandbox feature / binary |
| `scripts/sandbox-share/` | Host↔sandbox scratch (installer copies, old direct-install logs). **gitignored** |
| `docs/sandbox-testing.md` | This guide |

Scratch copies used for the 0.1.1 SandboxTest run live outside the repo under `%USERPROFILE%\tmp\winget-sandbox\` so large WinGet bootstrap downloads stay out of git.

## What a PASS proves

- Manifest + installer URL/hash work through WinGet
- Silent switches are accepted without UI
- `Commands: clevershim` resolves after install
- Uninstall key / PATH registration happened

## What it still may not prove

- Exact Azure validation timeout thresholds
- Elevated / machine-scope behavior
- Sync cost on a machine that already has many WinGet packages / Uninstall keys
