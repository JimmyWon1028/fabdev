$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$outputRoot = Join-Path $repoRoot "distribution/windows/runtime"
$downloadRoot = if ($env:RUNNER_TEMP) {
  Join-Path $env:RUNNER_TEMP "fabdev-windows-runtimes"
} else {
  Join-Path $env:TEMP "fabdev-windows-runtimes"
}

$runtimes = @(
  @{
    Name = "php-7.4.33"
    Url = "https://downloads.php.net/~windows/releases/archives/php-7.4.33-nts-Win32-vc15-x64.zip"
    Sha256 = "14ae3250d4447c8ccfc4c45a70d90adfbcd61e728d85f0be56a7ddf8f9c8aace"
    Destination = "php/7.4.33"
    StripRoot = $false
  },
  @{
    Name = "php-8.2.33"
    Url = "https://downloads.php.net/~windows/releases/archives/php-8.2.33-nts-Win32-vs16-x64.zip"
    Sha256 = "d0bd189522fa50255ee94ed4b340ed4330f5ae33a90a74205275b0f0b221d388"
    Destination = "php/8.2.33"
    StripRoot = $false
  },
  @{
    Name = "nginx-1.30.4"
    Url = "https://nginx.org/download/nginx-1.30.4.zip"
    Sha256 = "159294214d403f34f0bb4ae598801ab1f6a0d8c8da707f8f08748e294a222a01"
    Destination = "nginx/current"
    StripRoot = $true
  }
)

New-Item -ItemType Directory -Force -Path $downloadRoot | Out-Null
if (Test-Path $outputRoot) {
  Remove-Item -Recurse -Force $outputRoot
}
New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null

foreach ($runtime in $runtimes) {
  $archive = Join-Path $downloadRoot "$($runtime.Name).zip"
  if (-not (Test-Path $archive)) {
    Invoke-WebRequest -Uri $runtime.Url -OutFile $archive
  }
  $actualHash = (Get-FileHash -Algorithm SHA256 $archive).Hash.ToLowerInvariant()
  if ($actualHash -ne $runtime.Sha256) {
    throw "SHA-256 mismatch for $($runtime.Name): expected $($runtime.Sha256), got $actualHash"
  }

  $expanded = Join-Path $downloadRoot "$($runtime.Name)-expanded"
  if (Test-Path $expanded) {
    Remove-Item -Recurse -Force $expanded
  }
  Expand-Archive -Path $archive -DestinationPath $expanded
  $source = if ($runtime.StripRoot) {
    Get-ChildItem -Path $expanded -Directory | Select-Object -First 1 -ExpandProperty FullName
  } else {
    $expanded
  }
  $destination = Join-Path $outputRoot $runtime.Destination
  New-Item -ItemType Directory -Force -Path $destination | Out-Null
  Copy-Item -Path (Join-Path $source "*") -Destination $destination -Recurse -Force
  Write-Host "Prepared $($runtime.Name) at $destination"
}

$manifest = @{
  schemaVersion = 1
  platform = "windows"
  architecture = "x64"
  nginx = "1.30.4"
  php = @("7.4.33", "8.2.33")
} | ConvertTo-Json -Depth 3
Set-Content -Path (Join-Path $outputRoot "manifest.json") -Value $manifest -Encoding utf8
