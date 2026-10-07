$ErrorActionPreference = 'Continue'
$share = 'C:\Users\WDAGUtilityAccount\Desktop\Shared'
$logDir = 'C:\Users\WDAGUtilityAccount\Desktop\CleverShimSandbox'
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
New-Item -ItemType Directory -Force -Path $share | Out-Null
# Host-visible log is under Shared; keep a Desktop copy for in-sandbox inspection.
$log = Join-Path $share 'install-test.log'
$localLog = Join-Path $logDir 'install-test.log'
$transcript = Join-Path $share 'transcript.txt'
Start-Transcript -Path $transcript -Force | Out-Null

function Log([string]$message) {
    $line = "{0:u} {1}" -f (Get-Date).ToUniversalTime(), $message
    $line | Tee-Object -FilePath $log -Append | Tee-Object -FilePath $localLog -Append | Out-Null
    Write-Host $line
}

try {
    Log '=== CleverShim Windows Sandbox install proving ground ==='
    Log ("Computer=" + $env:COMPUTERNAME)
    Log ("User=" + $env:USERNAME)
    Log ("OS=" + [Environment]::OSVersion.VersionString)

    $installer = Join-Path $share 'clevershim-x86_64-pc-windows-msvc.exe'
    if (-not (Test-Path $installer)) {
        Log "Installer missing at $installer; downloading v0.1.1 release..."
        $url = 'https://github.com/clevotec/clevershim/releases/download/v0.1.1/clevershim-x86_64-pc-windows-msvc.exe'
        New-Item -ItemType Directory -Force -Path $share | Out-Null
        curl.exe -fsSL -o $installer -L $url
    }

    $size = (Get-Item $installer).Length
    Log ("InstallerSize=$size")
    $hash = (Get-FileHash -Algorithm SHA256 -Path $installer).Hash.ToLowerInvariant()
    Log ("InstallerSha256=$hash")

    Log 'Starting /install...'
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process -FilePath $installer -ArgumentList '/install' -PassThru -Wait -WindowStyle Hidden
    $sw.Stop()
    Log ("InstallExitCode=$($p.ExitCode)")
    Log ("InstallElapsedMs=$($sw.ElapsedMilliseconds)")

    $root = Join-Path $env:LOCALAPPDATA 'Clevotec\CleverShim'
    $binExe = Join-Path $root 'bin\clevershim.exe'
    $manager = Join-Path $root 'clevershim.exe'
    Log ("ManagerExists=" + (Test-Path $manager))
    Log ("BinExeExists=" + (Test-Path $binExe))

    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $pathHit = ($userPath -split ';' | Where-Object { $_ -match 'CleverShim' }) -join '|'
    Log ("UserPathHit=$pathHit")

    $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')
    $whereOut = cmd /c 'where clevershim 2>&1'
    Log ("WhereCleverShim=$whereOut")

    if (Test-Path $binExe) {
        $list = & $binExe list 2>&1 | Out-String
        Log ("ListOutput=`n$list")
        $syncSw = [Diagnostics.Stopwatch]::StartNew()
        $sync = Start-Process -FilePath $binExe -ArgumentList 'sync' -PassThru -Wait -WindowStyle Hidden
        $syncSw.Stop()
        Log ("SyncExitCode=$($sync.ExitCode)")
        Log ("SyncElapsedMs=$($syncSw.ElapsedMilliseconds)")
    }

    $task = Get-ScheduledTask -TaskName 'Clevotec CleverShim Sync' -ErrorAction SilentlyContinue
    if ($task) {
        Log ("LogonTaskState=" + $task.State)
    } else {
        Log 'LogonTaskState=missing'
    }

    $uninstall = Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall' -ErrorAction SilentlyContinue |
        ForEach-Object { Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue } |
        Where-Object { $_.DisplayName -match 'CleverShim' }
    if ($uninstall) {
        Log ("UninstallDisplayName=" + $uninstall.DisplayName)
        Log ("UninstallString=" + $uninstall.UninstallString)
        Log ("QuietUninstallString=" + $uninstall.QuietUninstallString)
    } else {
        Log 'UninstallKey=missing'
    }

    if ($p.ExitCode -eq 0 -and (Test-Path $binExe) -and $pathHit -and ($whereOut -match 'clevershim.exe')) {
        Log 'RESULT=PASS'
    } else {
        Log 'RESULT=FAIL'
    }
}
catch {
    Log ("EXCEPTION=" + $_.Exception.Message)
    Log 'RESULT=FAIL'
}
finally {
    Stop-Transcript | Out-Null
    # Keep the sandbox window open so results can be inspected.
    Write-Host ''
    Write-Host "Log written to $log"
    Write-Host 'Press Enter to close this window (sandbox stays open until you close it).'
    try { [void](Read-Host) } catch { Start-Sleep -Seconds 30 }
}
