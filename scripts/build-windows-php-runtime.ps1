param(
  [Parameter(Mandatory = $true)]
  [string]$OutputDirectory
)

$ErrorActionPreference = "Stop"

$phpVersion = "8.4.24"
$phpSha256 = "86470a30cbbaeafb259e727dfa5cd336f2f3f0a462cd6f8e3eac00fdbded13cb"
$phpArchiveName = "php-$phpVersion-nts-Win32-vs17-x64.zip"
$phpUrl = "https://windows.php.net/downloads/releases/archives/$phpArchiveName"
$packageName = "php-$phpVersion-windows-x64-community.tar.gz"
$buildRoot = Join-Path $env:RUNNER_TEMP "fabdev-online-php-$phpVersion"
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
New-Item -ItemType Directory -Force -Path $buildRoot, $expandedRoot, $runtimePath, $testIniRoot, $OutputDirectory | Out-Null

Invoke-WebRequest -Uri $phpUrl -OutFile $downloadPath
$actualSha256 = (Get-FileHash -Algorithm SHA256 $downloadPath).Hash.ToLowerInvariant()
if ($actualSha256 -ne $phpSha256) {
  throw "PHP source SHA-256 mismatch: expected $phpSha256, got $actualSha256"
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
    throw "PHP Runtime is missing required file: $requiredFile"
  }
}

$phpExe = Join-Path $runtimePath "php.exe"
$phpCgiExe = Join-Path $runtimePath "php-cgi.exe"
& $phpExe -n -v
if ($LASTEXITCODE -ne 0) {
  throw "PHP CLI startup failed"
}
& $phpCgiExe -n -v
if ($LASTEXITCODE -ne 0) {
  throw "PHP CGI startup failed"
}

$extensionDirectory = (Join-Path $runtimePath "ext").Replace("\", "/")
@"
extension_dir = "$extensionDirectory"
extension = mysqli
extension = pdo_mysql
"@ | Set-Content -Path $testIni -Encoding ascii
& $phpExe -c $testIni -r "exit(extension_loaded('mysqli') && extension_loaded('pdo_mysql') ? 0 : 1);"
if ($LASTEXITCODE -ne 0) {
  throw "PHP MySQL extensions failed to load"
}

if (Test-Path $packagePath) {
  throw "Runtime package already exists: $packagePath"
}
& tar.exe -czf $packagePath -C $runtimeRoot $phpVersion
if ($LASTEXITCODE -ne 0) {
  throw "Unable to create PHP Runtime package"
}
if ((Get-Item $packagePath).Length -le 0) {
  throw "PHP Runtime package is empty"
}

$entries = @(& tar.exe -tzf $packagePath)
if ($LASTEXITCODE -ne 0 -or $entries.Count -eq 0) {
  throw "Unable to inspect PHP Runtime package"
}
foreach ($entry in $entries) {
  $normalizedEntry = $entry.Replace("\", "/")
  if ($normalizedEntry -ne "$phpVersion/" -and -not $normalizedEntry.StartsWith("$phpVersion/")) {
    throw "Runtime package contains an entry outside $phpVersion/: $normalizedEntry"
  }
}

Write-Host "Created $packagePath"
