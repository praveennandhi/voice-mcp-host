param(
  [Parameter(Mandatory = $true)]
  [string]$Repo,
  [string]$Version = "latest"
)

$ErrorActionPreference = "Stop"

if (-not $Repo.Contains("/")) {
  throw "Repo must look like owner/name. Example: your-github-name/voice-mcp-host"
}

[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$headers = @{
  "User-Agent" = "voice-mcp-host-installer"
  "Accept" = "application/vnd.github+json"
}

if ($Version -eq "latest") {
  $releaseUrl = "https://api.github.com/repos/$Repo/releases/latest"
} else {
  $releaseUrl = "https://api.github.com/repos/$Repo/releases/tags/$Version"
}

Write-Host "Resolving voice-mcp-host release from $releaseUrl"
$release = Invoke-RestMethod -Uri $releaseUrl -Headers $headers

$asset = $release.assets |
  Where-Object { $_.name -match '(?i)(setup|installer).*\.exe$|\.exe$' } |
  Sort-Object name |
  Select-Object -First 1

if (-not $asset) {
  throw "No Windows .exe installer asset was found on release $($release.tag_name)."
}

$tempDir = Join-Path $env:TEMP "voice-mcp-host-install"
New-Item -ItemType Directory -Force -Path $tempDir | Out-Null
$installerPath = Join-Path $tempDir $asset.name

Write-Host "Downloading $($asset.name)"
Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $installerPath -Headers @{ "User-Agent" = "voice-mcp-host-installer" }

Write-Host "Launching installer"
$process = Start-Process -FilePath $installerPath -Wait -PassThru
if ($process.ExitCode -ne 0) {
  throw "Installer exited with code $($process.ExitCode)."
}

Write-Host "voice-mcp-host installed. Launch it from the Start menu."
