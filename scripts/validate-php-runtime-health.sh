#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "Usage: $0 <runtime-root>" >&2
  exit 1
fi

RUNTIME_ROOT="$1"
PHP_BIN="$RUNTIME_ROOT/bin/php"
PHP_FPM_BIN="$RUNTIME_ROOT/sbin/php-fpm"

if [[ "$RUNTIME_ROOT" != /* || "$RUNTIME_ROOT" == "/" \
  || ! -x "$PHP_BIN" || ! -x "$PHP_FPM_BIN" ]]
then
  echo "Invalid PHP Runtime root: $RUNTIME_ROOT" >&2
  exit 1
fi

TEST_ROOT="$(mktemp -d /private/tmp/fabdev-php-health.XXXXXX)"
FPM_PID_FILE="$TEST_ROOT/php-fpm.pid"
FPM_SOCKET="$TEST_ROOT/php-fpm.sock"

cleanup() {
  local fpm_pid=""
  local attempt

  if [[ -f "$FPM_PID_FILE" ]]; then
    fpm_pid="$(sed -n '1p' "$FPM_PID_FILE")"
  fi
  if [[ "$fpm_pid" =~ ^[1-9][0-9]*$ ]] \
    && kill -0 "$fpm_pid" >/dev/null 2>&1
  then
    kill "$fpm_pid" >/dev/null 2>&1 || true
    for attempt in {1..50}; do
      if ! kill -0 "$fpm_pid" >/dev/null 2>&1; then
        break
      fi
      sleep 0.1
    done
    if kill -0 "$fpm_pid" >/dev/null 2>&1; then
      kill -KILL "$fpm_pid" >/dev/null 2>&1 || true
    fi
  fi
  if [[ "$TEST_ROOT" == /private/tmp/fabdev-php-health.* ]]; then
    rm -rf -- "$TEST_ROOT"
  fi
}
trap cleanup EXIT

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
  'header("X-FabDev-Runtime: php-fpm");' \
  'echo json_encode(["sapi" => PHP_SAPI, "version" => PHP_VERSION, "sum" => 1 + 1]);' \
  > "$TEST_ROOT/index.php"

"$PHP_FPM_BIN" \
  --php-ini "$RUNTIME_ROOT/etc/php.ini" \
  --fpm-config "$TEST_ROOT/php-fpm.conf" \
  --test
"$PHP_FPM_BIN" \
  --php-ini "$RUNTIME_ROOT/etc/php.ini" \
  --fpm-config "$TEST_ROOT/php-fpm.conf"

for attempt in {1..100}; do
  if [[ -S "$FPM_SOCKET" ]]; then
    break
  fi
  sleep 0.1
done
if [[ ! -S "$FPM_SOCKET" ]]; then
  echo "PHP-FPM did not create its test socket" >&2
  exit 1
fi

"$PHP_BIN" -n -r '
  function encodeLength(int $length): string {
    return $length < 128 ? chr($length) : pack("N", $length | 0x80000000);
  }

  function record(int $type, string $content): string {
    $paddingLength = (8 - strlen($content) % 8) % 8;
    return pack("CCnnCC", 1, $type, 1, strlen($content), $paddingLength, 0)
      . $content
      . str_repeat("\0", $paddingLength);
  }

  $socketPath = $argv[1];
  $scriptPath = $argv[2];
  $socket = stream_socket_client("unix://" . $socketPath, $errorCode, $errorMessage, 5);
  if ($socket === false) {
    fwrite(STDERR, "Unable to connect to PHP-FPM: {$errorMessage}\n");
    exit(1);
  }

  $params = [
    "DOCUMENT_ROOT" => dirname($scriptPath),
    "GATEWAY_INTERFACE" => "CGI/1.1",
    "REQUEST_METHOD" => "GET",
    "REQUEST_URI" => "/index.php",
    "SCRIPT_FILENAME" => $scriptPath,
    "SCRIPT_NAME" => "/index.php",
    "SERVER_PROTOCOL" => "HTTP/1.1",
    "SERVER_SOFTWARE" => "fabDev Runtime health check",
  ];
  $paramContent = "";
  foreach ($params as $name => $value) {
    $paramContent .= encodeLength(strlen($name))
      . encodeLength(strlen($value))
      . $name
      . $value;
  }

  fwrite($socket, record(1, pack("nC", 1, 0) . str_repeat("\0", 5)));
  fwrite($socket, record(4, $paramContent));
  fwrite($socket, record(4, ""));
  fwrite($socket, record(5, ""));

  $stdout = "";
  while (!feof($socket)) {
    $header = fread($socket, 8);
    if ($header === "" || strlen($header) !== 8) {
      break;
    }
    $record = unpack("Cversion/Ctype/nrequestId/ncontentLength/CpaddingLength/Creserved", $header);
    $content = $record["contentLength"] > 0
      ? stream_get_contents($socket, $record["contentLength"])
      : "";
    if ($record["paddingLength"] > 0) {
      stream_get_contents($socket, $record["paddingLength"]);
    }
    if ($record["type"] === 6) {
      $stdout .= $content;
    }
    if ($record["type"] === 3) {
      break;
    }
  }
  fclose($socket);

  [$headers, $body] = array_pad(explode("\r\n\r\n", $stdout, 2), 2, "");
  $payload = json_decode($body, true);
  if (!str_contains($headers, "X-FabDev-Runtime: php-fpm")
    || !is_array($payload)
    || ($payload["sapi"] ?? null) !== "fpm-fcgi"
    || ($payload["sum"] ?? null) !== 2
  ) {
    fwrite(STDERR, "Unexpected PHP-FPM response:\n{$stdout}\n");
    exit(1);
  }
  echo "PHP-FPM FastCGI request passed with PHP {$payload["version"]}\n";
' "$FPM_SOCKET" "$TEST_ROOT/index.php"

echo "PHP Runtime health check passed"
