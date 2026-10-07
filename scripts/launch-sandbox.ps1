$ErrorActionPreference = 'Stop'
$wsb = Join-Path $PSScriptRoot 'clevershim-sandbox.wsb'
$share = Join-Path $PSScriptRoot 'sandbox-share'
$result = Join-Path $share 'install-test.log'
if (Test-Path $result) {
    Remove-Item $result -Force
}
Write-Host "Launching Windows Sandbox with $wsb"
Start-Process -FilePath "$env:WINDIR\System32\WindowsSandbox.exe" -ArgumentList "`"$wsb`""
Write-Host 'Waiting for sandbox install-test.log (up to 4 minutes)...'
$deadline = (Get-Date).AddMinutes(4)
while ((Get-Date) -lt $deadline) {
    if (Test-Path $result) {
        $content = Get-Content $result -Raw -ErrorAction SilentlyContinue
        if ($content -match 'RESULT=(PASS|FAIL)') {
            Write-Host '=== sandbox install-test.log ==='
            Get-Content $result
            exit 0
        }
    }
    Start-Sleep -Seconds 5
}
Write-Host 'Timed out waiting for sandbox log.'
if (Test-Path $result) {
    Write-Host 'Partial log:'
    Get-Content $result
}
Get-Process WindowsSandbox*,WindowsSandboxClient*,WindowsSandboxRemoteSession -ErrorAction SilentlyContinue |
    Format-Table Id, ProcessName, StartTime -AutoSize
exit 2
