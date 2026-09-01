#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PHP_PACKAGE="${1:-}"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This validation must run on macOS." >&2
  exit 1
fi

if [[ -z "$PHP_PACKAGE" || "$PHP_PACKAGE" != /* || ! -f "$PHP_PACKAGE" ]]; then
  echo "Usage: $0 /absolute/path/php-X.Y.Z-macos-arm64-community.tar.gz" >&2
  exit 1
fi

if [[ ! "$(basename "$PHP_PACKAGE")" =~ ^php-[0-9]+\.[0-9]+\.[0-9]+-macos-arm64-community\.tar\.gz$ ]]; then
  echo "The package filename must match php-X.Y.Z-macos-arm64-community.tar.gz." >&2
  exit 1
fi

cd "$REPO_ROOT"

FABDEV_MACOS_PHP_RUNTIME_PACKAGE="$PHP_PACKAGE" \
  "$SCRIPT_DIR/run-cargo.sh" test -p fabdev-updater \
  runtime_updates::tests::streams_the_real_macos_php_package_over_loopback \
  -- --ignored --exact --nocapture

FABDEV_MACOS_PHP_RUNTIME_PACKAGE="$PHP_PACKAGE" \
  "$SCRIPT_DIR/run-cargo.sh" test -p fabdev-agent \
  tests::installs_real_macos_php_through_the_online_agent_protocol \
  -- --ignored --exact --nocapture

echo "macOS PHP loopback download and Agent Protocol install validation passed."
