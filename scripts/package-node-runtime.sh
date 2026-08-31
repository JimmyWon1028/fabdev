#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "Usage: $0 <runtime-root> <node-version> <artifact-dir>" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
RUNTIME_ROOT="$1"
NODE_VERSION="$2"
ARTIFACT_DIR="$3"
PACKAGE_VARIANT="${FABDEV_RUNTIME_PACKAGE_VARIANT:-dev}"
case "$PACKAGE_VARIANT" in
  dev|community) ;;
  *)
    echo "Unsupported Runtime Package variant: $PACKAGE_VARIANT" >&2
    exit 1
    ;;
esac
ARCHIVE_NAME="node-$NODE_VERSION-macos-arm64-$PACKAGE_VARIANT.tar.gz"

if [[ ! -x "$RUNTIME_ROOT/bin/node" || ! -x "$RUNTIME_ROOT/bin/npm" ]]; then
  echo "Runtime does not contain the required Node.js binaries: $RUNTIME_ROOT" >&2
  exit 1
fi

if [[ "$($RUNTIME_ROOT/bin/node --version)" != "v$NODE_VERSION" ]]; then
  echo "Node.js Runtime version does not match $NODE_VERSION" >&2
  exit 1
fi

if [[ "$PACKAGE_VARIANT" == "community" ]]; then
  "$SCRIPT_DIR/validate-macos-runtime-minimum.sh" \
    "$RUNTIME_ROOT" \
    "${FABDEV_MINIMUM_MACOS_VERSION:?FABDEV_MINIMUM_MACOS_VERSION is required for Community packages}"
fi

mkdir -p "$ARTIFACT_DIR"
COPYFILE_DISABLE=1 tar -czf "$ARTIFACT_DIR/$ARCHIVE_NAME" \
  -C "$(dirname "$RUNTIME_ROOT")" "$(basename "$RUNTIME_ROOT")"
archive_root="$(basename "$RUNTIME_ROOT")"
while IFS= read -r archive_entry; do
  if [[ "$archive_entry" != "$archive_root/" && "$archive_entry" != "$archive_root/"* ]]; then
    echo "Runtime Archive contains an entry outside $archive_root/: $archive_entry" >&2
    exit 1
  fi
done < <(tar -tzf "$ARTIFACT_DIR/$ARCHIVE_NAME")
artifact_sha256="$(shasum -a 256 "$ARTIFACT_DIR/$ARCHIVE_NAME" | awk '{print $1}')"
artifact_size="$(stat -f '%z' "$ARTIFACT_DIR/$ARCHIVE_NAME")"

if [[ "$PACKAGE_VARIANT" == "dev" ]]; then
  sed \
    -e "s|@NAME@|node|g" \
    -e "s|@VERSION@|$NODE_VERSION|g" \
    -e "s|@ARCHIVE@|$ARCHIVE_NAME|g" \
    -e "s|@SIZE@|$artifact_size|g" \
    -e "s|@SHA256@|$artifact_sha256|g" \
    -e "s|@SIGNATURE@|development-ad-hoc|g" \
    "$PROJECT_DIR/resources/runtime/release.template.json" \
    > "$ARTIFACT_DIR/node-$NODE_VERSION-macos-arm64-dev.json"
else
  printf '%s  %s\n' "$artifact_sha256" "$ARCHIVE_NAME" \
    > "$ARTIFACT_DIR/$ARCHIVE_NAME.sha256"
fi

echo "Created $ARTIFACT_DIR/$ARCHIVE_NAME"
echo "SHA-256: $artifact_sha256"
