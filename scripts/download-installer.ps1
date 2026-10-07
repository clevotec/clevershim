$ErrorActionPreference = 'Stop'
$share = Join-Path $PSScriptRoot 'sandbox-share'
New-Item -ItemType Directory -Force -Path $share | Out-Null
$dest = Join-Path $share 'clevershim-x86_64-pc-windows-msvc.exe'
$url = 'https://github.com/clevotec/clevershim/releases/download/v0.1.1/clevershim-x86_64-pc-windows-msvc.exe'
Write-Host "Downloading $url"
curl.exe -fsSL -o $dest -L $url
$hash = (Get-FileHash -Algorithm SHA256 -Path $dest).Hash.ToLowerInvariant()
Write-Host ("size=" + (Get-Item $dest).Length)
Write-Host ("sha256=" + $hash)
Copy-Item -Force (Join-Path $PSScriptRoot 'sandbox-install-test.ps1') (Join-Path $share 'sandbox-install-test.ps1')
Write-Host ("share=" + $share)
