<#
.SYNOPSIS
  Speq installer for Windows (x86_64).

.EXAMPLE
  irm https://github.com/dayatech-labs/speq-cli/releases/latest/download/install.ps1 | iex

.EXAMPLE
  & ([scriptblock]::Create((irm https://github.com/dayatech-labs/speq-cli/releases/latest/download/install.ps1))) -Version 1.4.0

.DESCRIPTION
  Installs speq.exe into %LOCALAPPDATA%\Speq\bin without administrator rights. The download is
  verified against SHA256SUMS (fetched over HTTPS) before it is installed. The installer never asks
  for credentials or workspace URLs; run `speq login` afterwards.

  speq runs Git for you. When Git 2.28+ is not found, the installer offers to put a private copy of
  MinGit (Git for Windows' portable build) in %LOCALAPPDATA%\Speq\git, again without administrator
  rights, verified against the SHA-256 GitHub publishes for that release asset. -Yes installs it
  without asking; -NoGit skips the check.
#>
[CmdletBinding()]
param([string]$Version = "", [switch]$Yes, [switch]$NoGit)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$Releases = if ($env:SPEQ_RELEASE_URL) { $env:SPEQ_RELEASE_URL } else { "https://github.com/dayatech-labs/speq-cli/releases" }
if ($Releases -notmatch '^(https://|http://127\.0\.0\.1|http://localhost)') { throw "the release URL must use https" }

function Test-Git([string]$Exe) {
  try { $out = & $Exe --version 2>$null } catch { return $false }
  if ($LASTEXITCODE -ne 0 -or $out -notmatch '^git version (\d+)\.(\d+)') { return $false }
  $major = [int]$Matches[1]; $minor = [int]$Matches[2]
  return ($major -gt 2) -or ($major -eq 2 -and $minor -ge 28)
}

# A private MinGit for speq only: speq finds it at this path when Git is not on PATH.
function Install-GitIfMissing {
  if ($NoGit) { return }
  $gitDir = Join-Path $env:LOCALAPPDATA "Speq\git"
  $privateGit = Join-Path $gitDir "cmd\git.exe"
  if ((Get-Command git -ErrorAction SilentlyContinue) -and (Test-Git "git")) { return }
  if ((Test-Path $privateGit) -and (Test-Git $privateGit)) { return }

  Write-Host ""
  Write-Host "speq runs Git for you (status, commit, pull, push, ...), and Git 2.28 or newer was not found."
  if (-not $Yes) {
    $answer = Read-Host "Install a private copy of Git (MinGit) for speq in $gitDir? [Y/n]"
    if ($answer -and $answer -notmatch '^(y|yes)$') {
      Write-Host "Skipped. Install Git for Windows later: https://git-scm.com/download/win"
      return
    }
  }
  $gitTmp = Join-Path ([System.IO.Path]::GetTempPath()) ("speq-git-" + [guid]::NewGuid().ToString("N"))
  New-Item -ItemType Directory -Path $gitTmp | Out-Null
  try {
    $release = Invoke-RestMethod -UseBasicParsing -Uri "https://api.github.com/repos/git-for-windows/git/releases/latest" -Headers @{ "User-Agent" = "speq-installer" }
    $asset = $release.assets | Where-Object { $_.name -match '^MinGit-[0-9.]+-64-bit\.zip$' } | Select-Object -First 1
    if (-not $asset -or "$($asset.digest)" -notmatch '^sha256:([0-9a-f]{64})$') { throw "no verifiable MinGit download was found" }
    $expectedGit = $Matches[1]
    $zip = Join-Path $gitTmp $asset.name
    Write-Host "Downloading $($asset.name)..."
    Invoke-WebRequest -UseBasicParsing -Uri $asset.browser_download_url -OutFile $zip
    if ((Get-FileHash -Algorithm SHA256 -Path $zip).Hash.ToLowerInvariant() -ne $expectedGit) { throw "MinGit checksum mismatch" }
    $unpacked = Join-Path $gitTmp "mingit"
    Expand-Archive -Path $zip -DestinationPath $unpacked
    if (-not (Test-Git (Join-Path $unpacked "cmd\git.exe"))) { throw "the downloaded Git does not run" }
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $gitDir
    Move-Item $unpacked $gitDir
    Write-Host "Git installed for speq in $gitDir"
  } catch {
    Write-Host "Git was not installed ($($_.Exception.Message)). Install Git for Windows: https://git-scm.com/download/win"
  } finally {
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $gitTmp
  }
}

$Version = $Version.TrimStart("v")
if ($Version -and $Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+([0-9A-Za-z.+-]*)$') { throw "invalid version: $Version" }

$arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
if ($arch -ne [System.Runtime.InteropServices.Architecture]::X64) {
  throw "unsupported architecture: $arch. Download a build for your system from $Releases and install it manually."
}
$target = "x86_64-pc-windows-msvc"

$base = if ($Version) { "$Releases/download/v$Version" } else { "$Releases/latest/download" }
$binDir = Join-Path $env:LOCALAPPDATA "Speq\bin"
New-Item -ItemType Directory -Force -Path $binDir | Out-Null
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("speq-install-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tmp | Out-Null
$staged = Join-Path $binDir (".speq-install-" + [guid]::NewGuid().ToString("N") + ".exe")

try {
  Write-Host "Fetching the release manifest..."
  $sumsPath = Join-Path $tmp "SHA256SUMS"
  Invoke-WebRequest -UseBasicParsing -Uri "$base/SHA256SUMS" -OutFile $sumsPath

  $line = Get-Content $sumsPath | Where-Object { $_ -match "^[0-9a-f]{64}\s+speq-[0-9][^\s]*-$target\.exe$" } | Select-Object -First 1
  if (-not $line) { throw "this release has no build for $target" }
  $expected, $name = $line -split '\s+', 2
  $found = $name -replace "^speq-", "" -replace "-$target\.exe$", ""
  if ($Version -and $found -ne $Version) { throw "release mismatch: wanted $Version, found $found" }

  Write-Host "Downloading speq $found for $target..."
  $download = Join-Path $tmp $name
  Invoke-WebRequest -UseBasicParsing -Uri "$Releases/download/v$found/$name" -OutFile $download

  $actual = (Get-FileHash -Algorithm SHA256 -Path $download).Hash.ToLowerInvariant()
  if ($actual -ne $expected) { throw "checksum mismatch; nothing was installed" }

  Copy-Item $download $staged
  & $staged --version *> $null
  if ($LASTEXITCODE -ne 0) { throw "the downloaded binary does not run on this system; nothing was installed" }

  $dest = Join-Path $binDir "speq.exe"
  if (Test-Path $dest) {
    # A running speq.exe cannot be overwritten, but it can be renamed away.
    $old = "$dest.old"
    Remove-Item -Force -ErrorAction SilentlyContinue $old
    Move-Item -Force $dest $old
    try { Move-Item -Force $staged $dest } catch { Move-Item -Force $old $dest; throw }
    Remove-Item -Force -ErrorAction SilentlyContinue $old
  } else {
    Move-Item -Force $staged $dest
  }
  # Git runs speq under these names (remote helper and credential helper). Copies, as Windows symlinks need rights.
  foreach ($helper in @("git-remote-speq.exe", "git-credential-speq.exe")) {
    Copy-Item -Force $dest (Join-Path $binDir $helper)
  }
  Write-Host "Installed speq $found to $dest"
  # The same logo as `speq help`.
  Write-Host ""
  Write-Host @'
 ____  ____  _____  ___      ____ _     ___ 
/ ___||  _ \| ____|/ _ \    / ___| |   |_ _|
\___ \| |_) |  _| | | | |  | |   | |    | | 
 ___) |  __/| |___| |_| |  | |___| |___ | | 
|____/|_|   |_____|\__\_\   \____|_____|___|
'@ -ForegroundColor Cyan
  Write-Host ""

  Install-GitIfMissing

  # User PATH: add once, never duplicate.
  $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
  $entries = if ($userPath) { $userPath -split ';' | Where-Object { $_ } } else { @() }
  if ($entries -notcontains $binDir) {
    [Environment]::SetEnvironmentVariable("Path", (($entries + $binDir) -join ';'), "User")
    Write-Host ""
    Write-Host "$binDir was added to your user PATH. Open a new terminal, then run: speq login"
  } else {
    Write-Host "Next: run ``speq login``, then ``speq clone <repo>``."
  }
} finally {
  Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $tmp
  if (Test-Path $staged) { Remove-Item -Force -ErrorAction SilentlyContinue $staged }
}
