<#
.SYNOPSIS
  Install sight (sightline) from the latest GitHub release.

.EXAMPLE
  irm https://raw.githubusercontent.com/sao-coding/sightline/main/install.ps1 | iex

.EXAMPLE
  $env:SIGHT_VERSION = "0.1.0"; irm https://raw.githubusercontent.com/sao-coding/sightline/main/install.ps1 | iex

.PARAMETER Version
  Release to install, for example 0.1.0. Default: the latest. Env: SIGHT_VERSION.

.PARAMETER InstallDir
  Where to put sight.exe. Default: %LOCALAPPDATA%\Programs\sight. Env: SIGHT_INSTALL_DIR.

.PARAMETER NoPathUpdate
  Do not add InstallDir to the user PATH. Env: SIGHT_NO_PATH_UPDATE=1.

.PARAMETER Uninstall
  Remove sight.exe and its PATH entry. Env: SIGHT_UNINSTALL=1.
#>
[CmdletBinding()]
param(
    [string]$Version = $env:SIGHT_VERSION,
    [string]$InstallDir = $env:SIGHT_INSTALL_DIR,
    [switch]$NoPathUpdate,
    [switch]$Uninstall
)

if ($env:SIGHT_UNINSTALL) { $Uninstall = $true }
if ($env:SIGHT_NO_PATH_UPDATE) { $NoPathUpdate = $true }

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'   # Invoke-WebRequest is very slow with the progress bar
$repo = 'sao-coding/sightline'

if (-not $InstallDir) { $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\sight' }
$exe = Join-Path $InstallDir 'sight.exe'

function Get-UserPath { [Environment]::GetEnvironmentVariable('Path', 'User') }

function Set-UserPath([string]$value) {
    [Environment]::SetEnvironmentVariable('Path', $value, 'User')
}

function Split-PathList([string]$p) {
    @($p -split ';' | Where-Object { $_ -ne '' })
}

function Test-InPath([string]$dir) {
    $d = $dir.TrimEnd('\')
    [bool](Split-PathList (Get-UserPath) | Where-Object { $_.TrimEnd('\') -ieq $d })
}

if ($Uninstall) {
    if (Test-Path $exe) { Remove-Item $exe -Force; Write-Host "Removed $exe" }
    if (Test-InPath $InstallDir) {
        $d = $InstallDir.TrimEnd('\')
        Set-UserPath ((Split-PathList (Get-UserPath) | Where-Object { $_.TrimEnd('\') -ine $d }) -join ';')
        Write-Host "Removed $InstallDir from the user PATH"
    }
    if ((Test-Path $InstallDir) -and -not (Get-ChildItem $InstallDir -Force)) { Remove-Item $InstallDir -Force }
    return
}

if ($env:OS -ne 'Windows_NT') { throw 'sight only runs on Windows.' }
if (-not [Environment]::Is64BitOperatingSystem) { throw 'sight needs 64-bit Windows.' }

# TLS 1.2 for Windows PowerShell 5.1
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$base = if ($Version) {
    "https://github.com/$repo/releases/download/v$($Version.TrimStart('v'))"
} else {
    "https://github.com/$repo/releases/latest/download"
}

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("sight-install-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    Write-Host "Downloading sight.exe ($(if ($Version) { $Version } else { 'latest' }))..."
    $dl = Join-Path $tmp 'sight.exe'
    try {
        Invoke-WebRequest -Uri "$base/sight.exe" -OutFile $dl -UseBasicParsing
        Invoke-WebRequest -Uri "$base/sight.exe.sha256" -OutFile "$dl.sha256" -UseBasicParsing
    } catch {
        throw "Download failed ($base): $($_.Exception.Message)"
    }

    $want = ((Get-Content "$dl.sha256" -Raw).Trim() -split '\s+')[0].ToLower()
    $got = (Get-FileHash $dl -Algorithm SHA256).Hash.ToLower()
    if ($want -ne $got) { throw "Checksum mismatch: expected $want, got $got" }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    try {
        Move-Item $dl $exe -Force
    } catch {
        throw "Cannot write $exe (is sight running?): $($_.Exception.Message)"
    }
} finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

if (-not $NoPathUpdate -and -not (Test-InPath $InstallDir)) {
    $cur = Get-UserPath
    Set-UserPath ((@(Split-PathList $cur) + $InstallDir.TrimEnd('\')) -join ';')
    Write-Host "Added $InstallDir to the user PATH"
}
# Make it usable in this session too.
if (-not (Split-PathList $env:Path | Where-Object { $_.TrimEnd('\') -ieq $InstallDir.TrimEnd('\') })) {
    $env:Path = "$env:Path;$InstallDir"
}

Write-Host ''
& $exe doctor | Out-Null
if ($LASTEXITCODE -ne 0) { throw "Installed $exe but 'sight doctor' failed." }
Write-Host "Installed $exe"
Write-Host 'Open a new terminal, then run: sight doctor'
