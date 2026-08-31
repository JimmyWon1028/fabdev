param(
  [Parameter(Mandatory = $true)]
  [string]$OutputDirectory
)

$ErrorActionPreference = "Stop"

$phpRuntimes = @(
  @{
    Version = "7.4.33"
    Toolset = "vc15"
    Sha256 = "14ae3250d4447c8ccfc4c45a70d90adfbcd61e728d85f0be56a7ddf8f9c8aace"
  },
  @{
    Version = "8.2.33"
    Toolset = "vs16"
    Sha256 = "d0bd189522fa50255ee94ed4b340ed4330f5ae33a90a74205275b0f0b221d388"
  },
  @{
    Version = "8.4.24"
    Toolset = "vs17"
    Sha256 = "86470a30cbbaeafb259e727dfa5cd336f2f3f0a462cd6f8e3eac00fdbded13cb"
  }
)

$temporaryRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { $env:TEMP }
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

function Build-PhpRuntimePackage {
  param(
    [Parameter(Mandatory = $true)]
    [hashtable]$Runtime
  )

  $phpVersion = $Runtime.Version
  $phpArchiveName = "php-$phpVersion-nts-Win32-$($Runtime.Toolset)-x64.zip"
  $phpUrl = "https://windows.php.net/downloads/releases/archives/$phpArchiveName"
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
  if ($actualSha256 -ne $Runtime.Sha256) {
    throw "PHP $phpVersion source SHA-256 mismatch: expected $($Runtime.Sha256), got $actualSha256"
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
