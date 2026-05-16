param(
  [string]$Configuration = "release",
  [switch]$SkipTests,
  [switch]$SkipFasterWhisperBundle
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
  throw "npm was not found. Install Node.js 20+ before building."
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
  throw "cargo was not found. Install Rust stable before building."
}

if (-not (Test-Path "node_modules")) {
  npm ci
}

if (-not $SkipTests) {
  npm run check
  cargo check --manifest-path src-tauri/Cargo.toml
}

if (-not $SkipFasterWhisperBundle) {
  powershell -ExecutionPolicy Bypass -File scripts/prepare-faster-whisper-windows.ps1
}

npm run tauri build -- --bundles nsis

$bundleDir = Join-Path $repoRoot "src-tauri\target\$Configuration\bundle\nsis"
if (-not (Test-Path $bundleDir)) {
  throw "NSIS bundle directory was not created: $bundleDir"
}

$artifacts = Get-ChildItem $bundleDir -File | Where-Object { $_.Extension -in ".exe", ".zip", ".msi" }
if (-not $artifacts) {
  throw "No Windows installer artifacts found in $bundleDir"
}

$checksumFile = Join-Path $bundleDir "SHA256SUMS.txt"
Remove-Item $checksumFile -ErrorAction SilentlyContinue
foreach ($artifact in $artifacts) {
  $hash = Get-FileHash -Algorithm SHA256 -LiteralPath $artifact.FullName
  "$($hash.Hash.ToLowerInvariant())  $($artifact.Name)" | Add-Content -Encoding ASCII $checksumFile
}

Write-Host ""
Write-Host "Windows release artifacts:"
$artifacts | ForEach-Object { Write-Host "  $($_.FullName)" }
Write-Host "  $checksumFile"
