<# 
.SYNOPSIS
    Lux installer for Windows.
.DESCRIPTION
    Downloads the latest Lux release from GitHub and installs it to ~/.lux/bin.
    Adds the install directory to the user PATH if not already present.
.LINK
    https://github.com/Lux-Python/Lux
#>

$ErrorActionPreference = "Stop"

$LuxRepo  = "Lux-Python/Lux"
$LuxBin   = "lux.exe"
$InstallDir = Join-Path $env:USERPROFILE ".lux" "bin"

Write-Host ""
Write-Host "  Installing Lux from github.com/$LuxRepo ..." -ForegroundColor Cyan
Write-Host ""

# --- Resolve latest release tag ---------------------------------------------------
$ApiUrl = "https://api.github.com/repos/$LuxRepo/releases/latest"
try {
    $Release = Invoke-RestMethod -Uri $ApiUrl -Headers @{ Accept = "application/vnd.github+json" } -UseBasicParsing
    $Tag = $Release.tag_name
} catch {
    Write-Host "  Could not query GitHub releases. Falling back to 'latest'." -ForegroundColor Yellow
    $Tag = "latest"
}

# --- Build download URL -----------------------------------------------------------
$Arch = if ([System.Environment]::Is64BitOperatingSystem) { "x86_64" } else { "i686" }
$AssetName = "lux-$Arch-pc-windows-msvc.zip"

if ($Tag -eq "latest") {
    $DownloadUrl = "https://github.com/$LuxRepo/releases/latest/download/$AssetName"
} else {
    $DownloadUrl = "https://github.com/$LuxRepo/releases/download/$Tag/$AssetName"
}

# --- Download to temp dir ---------------------------------------------------------
$TempDir  = Join-Path ([System.IO.Path]::GetTempPath()) "lux-install"
$ZipPath  = Join-Path $TempDir $AssetName

if (Test-Path $TempDir) { Remove-Item $TempDir -Recurse -Force }
New-Item -ItemType Directory -Path $TempDir -Force | Out-Null

Write-Host "  Downloading $AssetName ($Tag) ..." -ForegroundColor White
try {
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $ZipPath -UseBasicParsing
} catch {
    Write-Host ""
    Write-Host "  Download failed." -ForegroundColor Red
    Write-Host "  URL: $DownloadUrl" -ForegroundColor DarkGray
    Write-Host "  Make sure a release with asset '$AssetName' exists." -ForegroundColor DarkGray
    Write-Host ""
    exit 1
}

# --- Extract ----------------------------------------------------------------------
Write-Host "  Extracting ..." -ForegroundColor White
Expand-Archive -Path $ZipPath -DestinationPath $TempDir -Force

# --- Install to ~/.lux/bin --------------------------------------------------------
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

$ExeSrc = Get-ChildItem -Path $TempDir -Filter $LuxBin -Recurse | Select-Object -First 1
if (-not $ExeSrc) {
    Write-Host "  Could not find $LuxBin in the downloaded archive." -ForegroundColor Red
    exit 1
}

Copy-Item -Path $ExeSrc.FullName -Destination (Join-Path $InstallDir $LuxBin) -Force

# --- Add to PATH ------------------------------------------------------------------
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$InstallDir;$UserPath", "User")
    Write-Host "  Added $InstallDir to user PATH." -ForegroundColor Green
}

# Also update the current session so the user can use lux right away
if ($env:Path -notlike "*$InstallDir*") {
    $env:Path = "$InstallDir;$env:Path"
}

# --- Cleanup ----------------------------------------------------------------------
Remove-Item $TempDir -Recurse -Force -ErrorAction SilentlyContinue

# --- Done -------------------------------------------------------------------------
Write-Host ""
Write-Host "  Lux installed successfully." -ForegroundColor Green
Write-Host ""
Write-Host "    Location:  $InstallDir\$LuxBin" -ForegroundColor White
Write-Host "    Version:   $Tag" -ForegroundColor White
Write-Host ""
Write-Host "  Run 'lux --help' to get started." -ForegroundColor Cyan
Write-Host ""
