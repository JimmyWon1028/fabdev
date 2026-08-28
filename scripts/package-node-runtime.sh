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
ARCHIVE_NAME="node-$NODE_VERSION-macos-arm64-dev.tar.gz"

if [[ ! -x "$RUNTIME_ROOT/bin/node" || ! -x "$RUNTIME_ROOT/bin/npm" ]]; then
  echo "Runtime does not contain the required Node.js binaries: $RUNTIME_ROOT" >&2
  exit 1
fi

if [[ "$($RUNTIME_ROOT/bin/node --version)" != "v$NODE_VERSION" ]]; then
  echo "Node.js Runtime version does not match $NODE_VERSION" >&2
  exit 1
fi

mkdir -p "$ARTIFACT_DIR"
COPYFILE_DISABLE=1 tar -czf "$ARTIFACT_DIR/$ARCHIVE_NAME" \
  -C "$(dirname "$RUNTIME_ROOT")" "$(basename "$RUNTIME_ROOT")"
artifact_sha256="$(shasum -a 256 "$ARTIFACT_DIR/$ARCHIVE_NAME" | awk '{print $1}')"
artifact_size="$(stat -f '%z' "$ARTIFACT_DIR/$ARCHIVE_NAME")"

sed \
  -e "s|@NAME@|node|g" \
  -e "s|@VERSION@|$NODE_VERSION|g" \
  -e "s|@ARCHIVE@|$ARCHIVE_NAME|g" \
  -e "s|@SIZE@|$artifact_size|g" \
  -e "s|@SHA256@|$artifact_sha256|g" \
  -e "s|@SIGNATURE@|development-ad-hoc|g" \
  "$PROJECT_DIR/resources/runtime/release.template.json" \
  > "$ARTIFACT_DIR/node-$NODE_VERSION-macos-arm64-dev.json"

echo "Created $ARTIFACT_DIR/$ARCHIVE_NAME"
echo "SHA-256: $artifact_sha256"
