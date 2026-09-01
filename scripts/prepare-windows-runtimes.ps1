param(
  [string]$ManifestPath = "$(Split-Path -Parent $PSScriptRoot)/resources/runtime-packages/windows-x64-bundled.json"
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$outputRoot = Join-Path $repoRoot "distribution/windows/runtime"
$downloadRoot = if ($env:RUNNER_TEMP) {
  Join-Path $env:RUNNER_TEMP "fabdev-windows-runtimes"
} else {
  Join-Path $env:TEMP "fabdev-windows-runtimes"
}

if (-not (Test-Path -PathType Leaf $ManifestPath)) {
  throw "Bundled Windows Runtime manifest does not exist: $ManifestPath"
}
$manifest = Get-Content -Raw -Path $ManifestPath | ConvertFrom-Json
if ($manifest.schemaVersion -ne 1 -or $manifest.platform -ne "windows" -or $manifest.architecture -ne "x64") {
  throw "Bundled Windows Runtime manifest has an unsupported schema or target"
}
$runtimes = @($manifest.packages)
if ($runtimes.Count -eq 0) {
  throw "Bundled Windows Runtime manifest does not contain packages"
}

$identities = @{}
foreach ($runtime in $runtimes) {
  $name = [string]$runtime.name
  $version = [string]$runtime.version
  $destination = ([string]$runtime.destination).Replace("\", "/")
  if ($name -notin @("php", "nginx") -or $version -notmatch '^\d+\.\d+\.\d+$') {
    throw "Invalid bundled Windows Runtime identity: $name $version"
  }
  if ($runtime.archiveUrl -notmatch '^https://') {
    throw "Bundled Windows Runtime source must use HTTPS: $name $version"
  }
  if ($runtime.archiveSha256 -notmatch '^[0-9a-f]{64}$') {
    throw "Invalid bundled Windows Runtime SHA-256: $name $version"
  }
  $expectedDestination = if ($name -eq "php") { "php/$version" } else { "nginx/current" }
  if ($destination -ne $expectedDestination) {
    throw "Invalid bundled Windows Runtime destination: $destination"
  }
  $identity = "$name@$version"
  if ($identities.ContainsKey($identity)) {
    throw "Duplicate bundled Windows Runtime identity: $identity"
  }
  $identities[$identity] = $true
}

$defaultPhpRuntimes = @($runtimes | Where-Object { $_.name -eq "php" -and $_.default -eq $true })
if ($defaultPhpRuntimes.Count -ne 1) {
  throw "Bundled Windows Runtime manifest must select exactly one default PHP package"
}

New-Item -ItemType Directory -Force -Path $downloadRoot | Out-Null
if (Test-Path $outputRoot) {
  Remove-Item -Recurse -Force $outputRoot
}
New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null

foreach ($runtime in $runtimes) {
  $name = [string]$runtime.name
  $version = [string]$runtime.version
  $identity = "$name-$version"
  $archive = Join-Path $downloadRoot "$identity.zip"
  if (-not (Test-Path $archive)) {
    Invoke-WebRequest -Uri $runtime.archiveUrl -OutFile $archive
  }
  $actualHash = (Get-FileHash -Algorithm SHA256 $archive).Hash.ToLowerInvariant()
  if ($actualHash -ne $runtime.archiveSha256) {
    throw "SHA-256 mismatch for $identity`: expected $($runtime.archiveSha256), got $actualHash"
  }

  $expanded = Join-Path $downloadRoot "$identity-expanded"
  if (Test-Path $expanded) {
    Remove-Item -Recurse -Force $expanded
  }
  Expand-Archive -Path $archive -DestinationPath $expanded
  $source = if ($runtime.stripRoot -eq $true) {
    Get-ChildItem -Path $expanded -Directory | Select-Object -First 1 -ExpandProperty FullName
  } else {
    $expanded
  }
  if (-not $source) {
    throw "Bundled Windows Runtime archive is empty: $identity"
  }
  $destination = Join-Path $outputRoot ([string]$runtime.destination)
  New-Item -ItemType Directory -Force -Path $destination | Out-Null
  Copy-Item -Path (Join-Path $source "*") -Destination $destination -Recurse -Force
  Write-Host "Prepared $identity at $destination"
}

$outputManifest = @{
  schemaVersion = 1
  platform = "windows"
  architecture = "x64"
  defaultPhpVersion = [string]$defaultPhpRuntimes[0].version
  packages = @($runtimes | ForEach-Object {
    @{
      name = [string]$_.name
      version = [string]$_.version
    }
  })
} | ConvertTo-Json -Depth 4
Set-Content -Path (Join-Path $outputRoot "manifest.json") -Value $outputManifest -Encoding utf8
