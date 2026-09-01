#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 7 ]]; then
  echo "Usage: $0 <release-version> <catalog-sequence> <generated-at> <expires-at> <minimum-app-version> <package-manifest> <output-dir>" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RELEASE_VERSION="$1"
CATALOG_SEQUENCE="$2"
GENERATED_AT="$3"
EXPIRES_AT="$4"
MINIMUM_APP_VERSION="$5"
PACKAGE_MANIFEST="$6"
OUTPUT_DIR="$7"

if [[ ! -f "$PACKAGE_MANIFEST" ]]; then
  echo "Runtime package manifest does not exist: $PACKAGE_MANIFEST" >&2
  exit 1
fi
PACKAGE_MANIFEST="$(cd "$(dirname "$PACKAGE_MANIFEST")" && pwd -P)/$(basename "$PACKAGE_MANIFEST")"

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

"$SCRIPT_DIR/build-macos-runtime-packages.sh" \
  "$PACKAGE_MANIFEST" \
  "$STAGING_DIR" \
  community

"$SCRIPT_DIR/run-cargo.sh" run --locked -p fabdev-runtime --bin fabdev-runtime-catalog -- \
  generate-macos \
  "$RELEASE_VERSION" \
  "$CATALOG_SEQUENCE" \
  "$GENERATED_AT" \
  "$EXPIRES_AT" \
  "$MINIMUM_APP_VERSION" \
  "$PACKAGE_MANIFEST" \
  "$STAGING_DIR" \
  "$STAGING_DIR/fabdev-runtime-v1.json"

"$SCRIPT_DIR/run-cargo.sh" run --locked -p fabdev-runtime --bin fabdev-runtime-catalog -- \
  validate \
  "$STAGING_DIR/fabdev-runtime-v1.json" \
  "$MINIMUM_APP_VERSION"

(
  cd "$STAGING_DIR"
  shasum -a 256 --check ./*.tar.gz.sha256
  package_count="$(jq '.packages | length' "$PACKAGE_MANIFEST")"
  expected_file_count="$((package_count * 2 + 1))"
  test "$(find . -maxdepth 1 -type f | wc -l | tr -d ' ')" = "$expected_file_count"
)

mv "$STAGING_DIR" "$OUTPUT_DIR"
trap - EXIT
echo "Created macOS ARM64 online Runtime packages in $OUTPUT_DIR"
