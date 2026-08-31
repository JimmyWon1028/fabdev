#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "Usage: $0 <runtime-root>" >&2
  exit 1
fi

RUNTIME_ROOT="$1"
for required_path in \
  "$RUNTIME_ROOT/bin/mariadbd" \
  "$RUNTIME_ROOT/bin/mariadb" \
  "$RUNTIME_ROOT/bin/mariadb-admin" \
  "$RUNTIME_ROOT/scripts/mariadb-install-db"
do
  if [[ ! -x "$required_path" ]]; then
    echo "MariaDB Runtime health check is missing executable: $required_path" >&2
    exit 1
  fi
done

HEALTH_ROOT="$(mktemp -d /private/tmp/fabdev-mariadb-health.XXXXXX)"
SERVER_PID=""

cleanup_health() {
  if [[ -n "$SERVER_PID" ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  rm -rf -- "$HEALTH_ROOT"
}
trap cleanup_health EXIT

(
  cd "$RUNTIME_ROOT"
  /bin/sh scripts/mariadb-install-db \
    --no-defaults \
    --datadir="$HEALTH_ROOT/data" \
    --auth-root-authentication-method=normal \
    --skip-name-resolve \
    --skip-test-db
)

"$RUNTIME_ROOT/bin/mariadbd" \
  --no-defaults \
  --basedir="$RUNTIME_ROOT" \
  --datadir="$HEALTH_ROOT/data" \
  --socket="$HEALTH_ROOT/mariadb.sock" \
  --pid-file="$HEALTH_ROOT/mariadb.pid" \
  --log-error="$HEALTH_ROOT/mariadb.log" \
  --skip-networking \
  --user="$(id -un)" &
SERVER_PID=$!

for _ in {1..100}; do
  if [[ -S "$HEALTH_ROOT/mariadb.sock" ]]; then
    break
  fi
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    cat "$HEALTH_ROOT/mariadb.log" >&2
    exit 1
  fi
  sleep 0.1
done
if [[ ! -S "$HEALTH_ROOT/mariadb.sock" ]]; then
  cat "$HEALTH_ROOT/mariadb.log" >&2
  echo "MariaDB Runtime did not create its Unix Socket" >&2
  exit 1
fi

query_result="$(
  "$RUNTIME_ROOT/bin/mariadb" \
    --no-defaults \
    --protocol=socket \
    --socket="$HEALTH_ROOT/mariadb.sock" \
    --user=root \
    --batch \
    --skip-column-names \
    --execute="SELECT VERSION(), @@version_comment, 1 + 1"
)"
if [[ "$query_result" != *$'\t'fabDev$'\t'2 ]]; then
  echo "Unexpected MariaDB Runtime query result: $query_result" >&2
  exit 1
fi

"$RUNTIME_ROOT/bin/mariadb-admin" \
  --no-defaults \
  --protocol=socket \
  --socket="$HEALTH_ROOT/mariadb.sock" \
  --user=root \
  shutdown
wait "$SERVER_PID"
SERVER_PID=""

if [[ -e "$HEALTH_ROOT/mariadb.sock" || -e "$HEALTH_ROOT/mariadb.pid" ]]; then
  echo "MariaDB Runtime left a Socket or PID file after shutdown" >&2
  exit 1
fi

echo "MariaDB Runtime initialize, start, query, and shutdown health check passed"
