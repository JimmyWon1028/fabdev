#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 6 ]]; then
  echo "Usage: $0 <release-version> <catalog-sequence> <generated-at> <expires-at> <minimum-app-version> <output-dir>" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RELEASE_VERSION="$1"
CATALOG_SEQUENCE="$2"
GENERATED_AT="$3"
EXPIRES_AT="$4"
MINIMUM_APP_VERSION="$5"
OUTPUT_DIR="$6"

if [[ -e "$OUTPUT_DIR" ]]; then
  echo "Output directory already exists: $OUTPUT_DIR" >&2
  exit 1
fi

OUTPUT_PARENT="$(dirname "$OUTPUT_DIR")"
mkdir -p "$OUTPUT_PARENT"
OUTPUT_PARENT="$(cd "$OUTPUT_PARENT" && pwd -P)"
OUTPUT_DIR="$OUTPUT_PARENT/$(basename "$OUTPUT_DIR")"
STAGING_DIR="$(mktemp -d "$OUTPUT_PARENT/.fabdev-macos-runtimes.XXXXXX")"
cleanup() {
  if [[ -d "$STAGING_DIR" ]]; then
    rm -rf "$STAGING_DIR"
  fi
}
trap cleanup EXIT

FABDEV_ARTIFACT_DIR="$STAGING_DIR" \
FABDEV_RUNTIME_PACKAGE_VARIANT=community \
MACOSX_DEPLOYMENT_TARGET=13.0 \
PHP_VERSION=8.4.24 \
  "$SCRIPT_DIR/build-php-runtime.sh"

FABDEV_ARTIFACT_DIR="$STAGING_DIR" \
FABDEV_RUNTIME_PACKAGE_VARIANT=community \
MACOSX_DEPLOYMENT_TARGET=13.0 \
MARIADB_VERSION=12.3.2 \
  "$SCRIPT_DIR/build-mariadb-runtime.sh"

for node_version in 20.20.2 24.20.0; do
  FABDEV_ARTIFACT_DIR="$STAGING_DIR" \
  FABDEV_RUNTIME_PACKAGE_VARIANT=community \
  NODE_VERSION="$node_version" \
    "$SCRIPT_DIR/build-node-runtime.sh"
done

"$SCRIPT_DIR/run-cargo.sh" run --locked -p fabdev-runtime --bin fabdev-runtime-catalog -- \
  generate-macos \
  "$RELEASE_VERSION" \
  "$CATALOG_SEQUENCE" \
  "$GENERATED_AT" \
  "$EXPIRES_AT" \
  "$MINIMUM_APP_VERSION" \
  "$STAGING_DIR/php-8.4.24-macos-arm64-community.tar.gz" \
  "$STAGING_DIR/mariadb-12.3.2-macos-arm64-community.tar.gz" \
  "$STAGING_DIR/node-20.20.2-macos-arm64-community.tar.gz" \
  "$STAGING_DIR/node-24.20.0-macos-arm64-community.tar.gz" \
  "$STAGING_DIR/fabdev-runtime-v1.json"

"$SCRIPT_DIR/run-cargo.sh" run --locked -p fabdev-runtime --bin fabdev-runtime-catalog -- \
  validate \
  "$STAGING_DIR/fabdev-runtime-v1.json" \
  "$MINIMUM_APP_VERSION"

(
  cd "$STAGING_DIR"
  shasum -a 256 --check ./*.tar.gz.sha256
  test "$(find . -maxdepth 1 -type f | wc -l | tr -d ' ')" = "9"
)

mv "$STAGING_DIR" "$OUTPUT_DIR"
trap - EXIT
echo "Created macOS ARM64 online Runtime packages in $OUTPUT_DIR"
