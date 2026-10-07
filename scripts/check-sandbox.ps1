$ErrorActionPreference = 'Continue'
Write-Host ("OS=" + [Environment]::OSVersion.VersionString)
$feature = Get-WindowsOptionalFeature -Online -FeatureName Containers-DisposableClientVM -ErrorAction SilentlyContinue
if ($null -ne $feature) {
    Write-Host ("SandboxFeature=" + $feature.State)
} else {
    Write-Host 'SandboxFeature=unavailable'
}
$sandboxExe = Join-Path $env:WINDIR 'System32\WindowsSandbox.exe'
Write-Host ("WindowsSandboxPath=" + $sandboxExe)
Write-Host ("WindowsSandboxExists=" + (Test-Path $sandboxExe))
Get-Command WindowsSandbox.exe -ErrorAction SilentlyContinue | Format-List Source
$winget = Get-Command winget -ErrorAction SilentlyContinue
if ($winget) { Write-Host ("winget=" + $winget.Source) } else { Write-Host 'winget=missing' }
Write-Host ("Admin=" + ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator))
