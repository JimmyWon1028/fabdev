param(
  [Parameter(Mandatory = $true)]
  [string]$OutputDirectory,

  [Parameter(Mandatory = $true)]
  [string]$ManifestPath
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -PathType Leaf $ManifestPath)) {
  throw "Windows Runtime package manifest does not exist: $ManifestPath"
}

$manifest = Get-Content -Raw -Path $ManifestPath | ConvertFrom-Json
if ($manifest.schemaVersion -ne 1 -or $manifest.platform -ne "windows" -or $manifest.architecture -ne "x64") {
  throw "Windows Runtime package manifest has an unsupported schema or target"
}

$phpRuntimes = @($manifest.packages | Where-Object { $_.name -eq "php" })
if ($phpRuntimes.Count -eq 0) {
  throw "Windows Runtime package manifest does not contain PHP packages"
}

$temporaryRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { $env:TEMP }
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

function Build-PhpRuntimePackage {
  param(
    [Parameter(Mandatory = $true)]
    [object]$Runtime
  )

  $phpVersion = [string]$Runtime.version
  if ($phpVersion -notmatch '^\d+\.\d+\.\d+$') {
    throw "Invalid PHP Runtime version in package manifest: $phpVersion"
  }
  if ($Runtime.source.verification.method -ne "official-sha256") {
    throw "PHP $phpVersion package must use official-sha256 verification"
  }

  $phpUrl = [string]$Runtime.source.archiveUrl
  $phpArchiveName = [System.IO.Path]::GetFileName(([Uri]$phpUrl).AbsolutePath)
  $expectedSha256 = ([string]$Runtime.source.archiveSha256).ToLowerInvariant()
  $packageName = "php-$phpVersion-windows-x64-community.tar.gz"
  $buildRoot = Join-Path $temporaryRoot "fabdev-online-php-$phpVersion"
  $downloadPath = Join-Path $buildRoot $phpArchiveName
  $expandedRoot = Join-Path $buildRoot "expanded"
  $runtimeRoot = Join-Path $buildRoot "runtime"
  $runtimePath = Join-Path $runtimeRoot $phpVersion
  $testIniRoot = Join-Path $buildRoot "config-test"
  $testIni = Join-Path $testIniRoot "php.ini"
  $packagePath = Join-Path $OutputDirectory $packageName

  if (Test-Path $buildRoot) {
    Remove-Item -Recurse -Force $buildRoot
  }
  New-Item -ItemType Directory -Force -Path $buildRoot, $expandedRoot, $runtimePath, $testIniRoot | Out-Null

  Invoke-WebRequest -Uri $phpUrl -OutFile $downloadPath
  $actualSha256 = (Get-FileHash -Algorithm SHA256 $downloadPath).Hash.ToLowerInvariant()
  if ($actualSha256 -ne $expectedSha256) {
    throw "PHP $phpVersion source SHA-256 mismatch: expected $expectedSha256, got $actualSha256"
  }

  Expand-Archive -Path $downloadPath -DestinationPath $expandedRoot
  Copy-Item -Path (Join-Path $expandedRoot "*") -Destination $runtimePath -Recurse -Force

  $requiredFiles = @(
    "php.exe",
    "php-cgi.exe",
    "ext/php_mysqli.dll",
    "ext/php_pdo_mysql.dll"
  )
  foreach ($requiredFile in $requiredFiles) {
    $requiredPath = Join-Path $runtimePath $requiredFile
    if (-not (Test-Path -PathType Leaf $requiredPath)) {
      throw "PHP $phpVersion Runtime is missing required file: $requiredFile"
    }
  }

  $phpExe = Join-Path $runtimePath "php.exe"
  $phpCgiExe = Join-Path $runtimePath "php-cgi.exe"
  & $phpExe -n -v
  if ($LASTEXITCODE -ne 0) {
    throw "PHP $phpVersion CLI startup failed"
  }
  & $phpCgiExe -n -v
  if ($LASTEXITCODE -ne 0) {
    throw "PHP $phpVersion CGI startup failed"
  }

  $extensionDirectory = (Join-Path $runtimePath "ext").Replace("\", "/")
  @"
extension_dir = "$extensionDirectory"
extension = mysqli
extension = pdo_mysql
"@ | Set-Content -Path $testIni -Encoding ascii
  & $phpExe -c $testIni -r "exit(extension_loaded('mysqli') && extension_loaded('pdo_mysql') ? 0 : 1);"
  if ($LASTEXITCODE -ne 0) {
    throw "PHP $phpVersion MySQL extensions failed to load"
  }

  if (Test-Path $packagePath) {
    throw "Runtime package already exists: $packagePath"
  }
  & tar.exe -czf $packagePath -C $runtimeRoot $phpVersion
  if ($LASTEXITCODE -ne 0) {
    throw "Unable to create PHP $phpVersion Runtime package"
  }
  if ((Get-Item $packagePath).Length -le 0) {
    throw "PHP $phpVersion Runtime package is empty"
  }

  $entries = @(& tar.exe -tzf $packagePath)
  if ($LASTEXITCODE -ne 0 -or $entries.Count -eq 0) {
    throw "Unable to inspect PHP $phpVersion Runtime package"
  }
  foreach ($entry in $entries) {
    $normalizedEntry = $entry.Replace("\", "/")
    if ($normalizedEntry -ne "$phpVersion/" -and -not $normalizedEntry.StartsWith("$phpVersion/")) {
      throw "Runtime package contains an entry outside $phpVersion/: $normalizedEntry"
    }
  }

  Write-Host "Created $packagePath"
}

foreach ($runtime in $phpRuntimes) {
  Build-PhpRuntimePackage -Runtime $runtime
}
