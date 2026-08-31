#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "Usage: $0 <php-runtime-root> <nginx-runtime-root>" >&2
  exit 1
fi

PHP_RUNTIME_ROOT="$1"
NGINX_RUNTIME_ROOT="$2"
PHP_BIN="$PHP_RUNTIME_ROOT/bin/php"
PHP_FPM_BIN="$PHP_RUNTIME_ROOT/sbin/php-fpm"
NGINX_BIN="$NGINX_RUNTIME_ROOT/sbin/nginx"
SITE_DOMAIN="fabdev-php-runtime.test"

if [[ "$PHP_RUNTIME_ROOT" != /* || "$PHP_RUNTIME_ROOT" == "/" \
  || ! -x "$PHP_BIN" || ! -x "$PHP_FPM_BIN" ]]
then
  echo "Invalid PHP Runtime root: $PHP_RUNTIME_ROOT" >&2
  exit 1
fi
if [[ "$NGINX_RUNTIME_ROOT" != /* || "$NGINX_RUNTIME_ROOT" == "/" \
  || ! -x "$NGINX_BIN" ]]
then
  echo "Invalid Nginx Runtime root: $NGINX_RUNTIME_ROOT" >&2
  exit 1
fi
if ! command -v curl >/dev/null 2>&1; then
  echo "Missing required command: curl" >&2
  exit 1
fi

TEST_ROOT="$(mktemp -d /private/tmp/fabdev-php-site.XXXXXX)"
FPM_PID_FILE="$TEST_ROOT/php-fpm.pid"
FPM_SOCKET="$TEST_ROOT/php-fpm.sock"
NGINX_PID_FILE="$TEST_ROOT/nginx.pid"
NGINX_PID=""
HTTP_PORT=""

stop_process() {
  local pid="$1"
  local attempt

  if [[ ! "$pid" =~ ^[1-9][0-9]*$ ]] \
    || ! kill -0 "$pid" >/dev/null 2>&1
  then
    return
  fi
  kill "$pid" >/dev/null 2>&1 || true
  for attempt in {1..50}; do
    if ! kill -0 "$pid" >/dev/null 2>&1; then
      return
    fi
    sleep 0.1
  done
  kill -KILL "$pid" >/dev/null 2>&1 || true
}

cleanup() {
  local fpm_pid=""

  if [[ -f "$NGINX_PID_FILE" ]]; then
    NGINX_PID="$(sed -n '1p' "$NGINX_PID_FILE")"
  fi
  if [[ -f "$FPM_PID_FILE" ]]; then
    fpm_pid="$(sed -n '1p' "$FPM_PID_FILE")"
  fi
  stop_process "$NGINX_PID"
  stop_process "$fpm_pid"
  if [[ "$TEST_ROOT" == /private/tmp/fabdev-php-site.* ]]; then
    rm -rf -- "$TEST_ROOT"
  fi
}
trap cleanup EXIT

port_is_available() {
  local port="$1"

  "$PHP_BIN" -n -r '
    $server = @stream_socket_server("tcp://127.0.0.1:" . $argv[1], $errorCode, $errorMessage);
    if ($server === false) {
      exit(1);
    }
    fclose($server);
  ' "$port"
}

if [[ -n "${FABDEV_PHP_SITE_HTTP_PORT:-}" ]]; then
  if [[ ! "$FABDEV_PHP_SITE_HTTP_PORT" =~ ^[1-9][0-9]*$ ]] \
    || ((FABDEV_PHP_SITE_HTTP_PORT > 65535)) \
    || ! port_is_available "$FABDEV_PHP_SITE_HTTP_PORT"
  then
    echo "Unavailable PHP Site health check port: $FABDEV_PHP_SITE_HTTP_PORT" >&2
    exit 1
  fi
  HTTP_PORT="$FABDEV_PHP_SITE_HTTP_PORT"
else
  for candidate_port in {18084..18104}; do
    if port_is_available "$candidate_port"; then
      HTTP_PORT="$candidate_port"
      break
    fi
  done
  if [[ -z "$HTTP_PORT" ]]; then
    echo "No loopback port is available for the PHP Site health check" >&2
    exit 1
  fi
fi

mkdir -p "$TEST_ROOT/site"
printf '%s\n' \
  '[global]' \
  "pid = $FPM_PID_FILE" \
  "error_log = $TEST_ROOT/php-fpm.log" \
  'daemonize = yes' \
  '' \
  '[www]' \
  "listen = $FPM_SOCKET" \
  'pm = static' \
  'pm.max_children = 1' \
  'clear_env = no' \
  > "$TEST_ROOT/php-fpm.conf"

printf '%s\n' \
  '<?php' \
  'header("Content-Type: application/json");' \
  'header("X-FabDev-Site: nginx-fpm");' \
  'echo json_encode([' \
  '  "host" => $_SERVER["HTTP_HOST"] ?? null,' \
  '  "sapi" => PHP_SAPI,' \
  '  "sum" => 1 + 1,' \
  '  "version" => PHP_VERSION,' \
  ']);' \
  > "$TEST_ROOT/site/index.php"

printf '%s\n' \
  'worker_processes 1;' \
  "pid $NGINX_PID_FILE;" \
  "error_log $TEST_ROOT/nginx-error.log;" \
  'events { worker_connections 32; }' \
  'http {' \
  "  access_log $TEST_ROOT/nginx-access.log;" \
  '  server {' \
  "    listen 127.0.0.1:$HTTP_PORT;" \
  "    server_name $SITE_DOMAIN;" \
  "    root $TEST_ROOT/site;" \
  '    index index.php;' \
  '    location / {' \
  '      try_files $uri $uri/ /index.php?$query_string;' \
  '    }' \
  '    location ~ \.php$ {' \
  "      include $NGINX_RUNTIME_ROOT/conf/fastcgi_params;" \
  '      fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;' \
  "      fastcgi_pass unix:$FPM_SOCKET;" \
  '    }' \
  '  }' \
  '}' \
  > "$TEST_ROOT/nginx.conf"

"$PHP_FPM_BIN" \
  --php-ini "$PHP_RUNTIME_ROOT/etc/php.ini" \
  --fpm-config "$TEST_ROOT/php-fpm.conf" \
  --test
"$NGINX_BIN" \
  -p "$NGINX_RUNTIME_ROOT/" \
  -c "$TEST_ROOT/nginx.conf" \
  -t

"$PHP_FPM_BIN" \
  --php-ini "$PHP_RUNTIME_ROOT/etc/php.ini" \
  --fpm-config "$TEST_ROOT/php-fpm.conf"
for attempt in {1..100}; do
  if [[ -S "$FPM_SOCKET" ]]; then
    break
  fi
  sleep 0.1
done
if [[ ! -S "$FPM_SOCKET" ]]; then
  echo "PHP-FPM did not create its Site health check socket" >&2
  exit 1
fi

"$NGINX_BIN" \
  -p "$NGINX_RUNTIME_ROOT/" \
  -c "$TEST_ROOT/nginx.conf" \
  -g 'daemon off;' \
  > "$TEST_ROOT/nginx-process.log" 2>&1 &
NGINX_PID=$!

for attempt in {1..100}; do
  if curl \
    --fail \
    --silent \
    --show-error \
    --header "Host: $SITE_DOMAIN" \
    --dump-header "$TEST_ROOT/headers.txt" \
    --output "$TEST_ROOT/body.json" \
    "http://127.0.0.1:$HTTP_PORT/"
  then
    break
  fi
  sleep 0.1
done
if [[ ! -s "$TEST_ROOT/body.json" ]] \
  || ! grep -qi '^X-FabDev-Site: nginx-fpm' "$TEST_ROOT/headers.txt"
then
  echo "Nginx did not return the expected PHP Site response" >&2
  exit 1
fi

"$PHP_BIN" -n -r '
  $payload = json_decode(file_get_contents($argv[1]), true);
  if (!is_array($payload)
    || ($payload["host"] ?? null) !== $argv[2]
    || ($payload["sapi"] ?? null) !== "fpm-fcgi"
    || ($payload["sum"] ?? null) !== 2
  ) {
    fwrite(STDERR, "Unexpected Nginx PHP Site response\n");
    exit(1);
  }
  echo "Nginx Site HTTP passed with PHP {$payload["version"]}\n";
' "$TEST_ROOT/body.json" "$SITE_DOMAIN"

cleanup
trap - EXIT
if curl \
  --fail \
  --silent \
  --max-time 1 \
  "http://127.0.0.1:$HTTP_PORT/" \
  >/dev/null 2>&1
then
  echo "Nginx Site health check port remained active after cleanup" >&2
  exit 1
fi

echo "PHP Runtime Site HTTP health check passed"
