param(
  [string]$RuntimeRoot = "$(Split-Path -Parent $PSScriptRoot)/distribution/windows/runtime"
)

$ErrorActionPreference = "Stop"
$workerCount = 4
$requestDelayMilliseconds = 1500
$maximumParallelElapsedSeconds = 4.5
$phpProcess = $null
$nginxProcess = $null
$phpProcessIds = @()
$previousChildren = $env:PHP_FCGI_CHILDREN
$previousMaxRequests = $env:PHP_FCGI_MAX_REQUESTS
$temporaryRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { $env:TEMP }
$testRoot = Join-Path $temporaryRoot "fabdev-fastcgi-pool-$([guid]::NewGuid())"

function Get-FreeTcpPort {
  $listener = [System.Net.Sockets.TcpListener]::new(
    [System.Net.IPAddress]::Loopback,
    0
  )
  $listener.Start()
  try {
    return ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
  } finally {
    $listener.Stop()
  }
}

function Get-ProcessIdsForExecutable {
  param([string]$ExecutablePath)

  $expectedPath = [System.IO.Path]::GetFullPath($ExecutablePath)
  return @(Get-CimInstance Win32_Process -Filter "Name = 'php-cgi.exe'" |
    Where-Object {
      $_.ExecutablePath -and
      [string]::Equals(
        [System.IO.Path]::GetFullPath([string]$_.ExecutablePath),
        $expectedPath,
        [System.StringComparison]::OrdinalIgnoreCase
      )
    } |
    ForEach-Object { [int]$_.ProcessId })
}

function Wait-Until {
  param(
    [scriptblock]$Condition,
    [int]$TimeoutSeconds,
    [string]$FailureMessage
  )

  $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
  do {
    if (& $Condition) {
      return
    }
    Start-Sleep -Milliseconds 100
  } while ([DateTime]::UtcNow -lt $deadline)
  throw $FailureMessage
}

$runtimeManifestPath = Join-Path $RuntimeRoot "manifest.json"
if (-not (Test-Path -PathType Leaf $runtimeManifestPath)) {
  throw "Prepared Windows Runtime manifest does not exist: $runtimeManifestPath"
}
$runtimeManifest = Get-Content -Raw -Path $runtimeManifestPath | ConvertFrom-Json
$phpVersion = [string]$runtimeManifest.defaultPhpVersion
$phpCgi = Join-Path $RuntimeRoot "php/$phpVersion/php-cgi.exe"
$nginx = Join-Path $RuntimeRoot "nginx/current/nginx.exe"
foreach ($executable in @($phpCgi, $nginx)) {
  if (-not (Test-Path -PathType Leaf $executable)) {
    throw "Bundled Windows Runtime executable does not exist: $executable"
  }
}
$existingPhpProcessIds = @(Get-ProcessIdsForExecutable -ExecutablePath $phpCgi)

New-Item -ItemType Directory -Force -Path $testRoot | Out-Null
$documentRoot = Join-Path $testRoot "site"
New-Item -ItemType Directory -Force -Path $documentRoot | Out-Null
$phpIni = Join-Path $testRoot "php.ini"
$phpScript = Join-Path $documentRoot "index.php"
$healthFile = Join-Path $documentRoot "health.txt"
$nginxConfig = Join-Path $testRoot "nginx.conf"
$fastCgiPort = Get-FreeTcpPort
$httpPort = Get-FreeTcpPort

Set-Content -Path $phpIni -Encoding utf8 -Value @"
[PHP]
display_errors=1
log_errors=1
"@
Set-Content -Path $phpScript -Encoding utf8 -Value @"
<?php
usleep($($requestDelayMilliseconds * 1000));
echo "ok";
"@
Set-Content -Path $healthFile -Encoding ascii -Value "ready"

$normalizedRoot = $documentRoot.Replace("\", "/")
$normalizedTestRoot = $testRoot.Replace("\", "/")
$nginxTemplate = @'
worker_processes 1;
error_log "@TEST_ROOT@/nginx-error.log";
pid "@TEST_ROOT@/nginx.pid";

events {
  worker_connections 64;
}

http {
  access_log off;
  server {
    listen 127.0.0.1:@HTTP_PORT@;
    root "@DOCUMENT_ROOT@";

    location / {
      try_files $uri =404;
    }

    location ~ \.php$ {
      fastcgi_pass 127.0.0.1:@FASTCGI_PORT@;
      fastcgi_param QUERY_STRING $query_string;
      fastcgi_param REQUEST_METHOD $request_method;
      fastcgi_param CONTENT_TYPE $content_type;
      fastcgi_param CONTENT_LENGTH $content_length;
      fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
      fastcgi_param SCRIPT_NAME $fastcgi_script_name;
      fastcgi_param REQUEST_URI $request_uri;
      fastcgi_param DOCUMENT_URI $document_uri;
      fastcgi_param DOCUMENT_ROOT $document_root;
      fastcgi_param SERVER_PROTOCOL $server_protocol;
      fastcgi_param GATEWAY_INTERFACE CGI/1.1;
      fastcgi_param SERVER_SOFTWARE nginx;
      fastcgi_param REMOTE_ADDR $remote_addr;
      fastcgi_param REMOTE_PORT $remote_port;
      fastcgi_param SERVER_ADDR $server_addr;
      fastcgi_param SERVER_PORT $server_port;
      fastcgi_param SERVER_NAME $server_name;
    }
  }
}
'@
$renderedNginxConfig = $nginxTemplate.Replace("@TEST_ROOT@", $normalizedTestRoot)
$renderedNginxConfig = $renderedNginxConfig.Replace("@DOCUMENT_ROOT@", $normalizedRoot)
$renderedNginxConfig = $renderedNginxConfig.Replace("@HTTP_PORT@", [string]$httpPort)
$renderedNginxConfig = $renderedNginxConfig.Replace("@FASTCGI_PORT@", [string]$fastCgiPort)
Set-Content -Path $nginxConfig -Encoding utf8 -Value $renderedNginxConfig

try {
  $env:PHP_FCGI_CHILDREN = [string]$workerCount
  $env:PHP_FCGI_MAX_REQUESTS = "500"
  $phpProcess = Start-Process -FilePath $phpCgi -ArgumentList @(
    "-b",
    "127.0.0.1:$fastCgiPort",
    "-c",
    $phpIni
  ) -PassThru -WindowStyle Hidden

  try {
    Wait-Until -TimeoutSeconds 10 -FailureMessage "PHP FastCGI did not create one master and $workerCount workers" -Condition {
      $script:phpProcessIds = @(Get-ProcessIdsForExecutable -ExecutablePath $phpCgi |
        Where-Object { $existingPhpProcessIds -notcontains $_ })
      $script:phpProcessIds.Count -eq ($workerCount + 1) -and
        $script:phpProcessIds -contains $phpProcess.Id
    }
  } catch {
    Get-CimInstance Win32_Process -Filter "Name = 'php-cgi.exe'" |
      Select-Object ProcessId, ParentProcessId, ExecutablePath, CommandLine |
      Format-Table -AutoSize
    throw
  }

  $nginxProcess = Start-Process -FilePath $nginx -ArgumentList @(
    "-p",
    "$testRoot/",
    "-c",
    $nginxConfig
  ) -PassThru -WindowStyle Hidden

  $httpClient = [System.Net.Http.HttpClient]::new()
  $httpClient.Timeout = [TimeSpan]::FromSeconds(10)
  try {
    Wait-Until -TimeoutSeconds 10 -FailureMessage "Nginx did not become ready" -Condition {
      try {
        $response = $httpClient.GetAsync("http://127.0.0.1:$httpPort/health.txt").GetAwaiter().GetResult()
        return $response.IsSuccessStatusCode
      } catch {
        return $false
      }
    }

    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    $requests = @(1..$workerCount | ForEach-Object {
      $httpClient.GetStringAsync("http://127.0.0.1:$httpPort/index.php")
    })
    [System.Threading.Tasks.Task]::WaitAll([System.Threading.Tasks.Task[]]$requests)
    $stopwatch.Stop()
    foreach ($request in $requests) {
      if ($request.Result.Trim() -ne "ok") {
        throw "PHP FastCGI returned an unexpected response: $($request.Result)"
      }
    }
    if ($stopwatch.Elapsed.TotalSeconds -ge $maximumParallelElapsedSeconds) {
      throw "Four FastCGI requests took $($stopwatch.Elapsed.TotalSeconds) seconds and did not run concurrently"
    }
  } finally {
    $httpClient.Dispose()
  }

  Stop-Process -Id $phpProcess.Id -Force
  Wait-Until -TimeoutSeconds 10 -FailureMessage "PHP FastCGI workers remained after the parent stopped" -Condition {
    @($phpProcessIds | Where-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue }).Count -eq 0
  }
  $phpProcess = $null

  Write-Host "Verified $workerCount Windows FastCGI workers, parallel requests, and worker cleanup"
} finally {
  if ($nginxProcess) {
    & $nginx -p "$testRoot/" -c $nginxConfig -s quit 2>$null
    try {
      Wait-Process -Id $nginxProcess.Id -Timeout 5 -ErrorAction Stop
    } catch {
      Stop-Process -Id $nginxProcess.Id -Force -ErrorAction SilentlyContinue
    }
  }
  if ($phpProcess -and (Get-Process -Id $phpProcess.Id -ErrorAction SilentlyContinue)) {
    Stop-Process -Id $phpProcess.Id -Force -ErrorAction SilentlyContinue
  }
  foreach ($processId in $phpProcessIds) {
    Stop-Process -Id $processId -Force -ErrorAction SilentlyContinue
  }
  if ($null -eq $previousChildren) {
    Remove-Item Env:PHP_FCGI_CHILDREN -ErrorAction SilentlyContinue
  } else {
    $env:PHP_FCGI_CHILDREN = $previousChildren
  }
  if ($null -eq $previousMaxRequests) {
    Remove-Item Env:PHP_FCGI_MAX_REQUESTS -ErrorAction SilentlyContinue
  } else {
    $env:PHP_FCGI_MAX_REQUESTS = $previousMaxRequests
  }
  if (Test-Path $testRoot) {
    Remove-Item -Recurse -Force $testRoot
  }
}
