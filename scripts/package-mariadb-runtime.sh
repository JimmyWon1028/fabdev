#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "Usage: $0 <runtime-root> <mariadb-version> <artifact-dir>" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
RUNTIME_ROOT="$1"
MARIADB_VERSION="$2"
ARTIFACT_DIR="$3"
ARCHIVE_NAME="mariadb-$MARIADB_VERSION-macos-arm64-dev.tar.gz"

if [[ ! -x "$RUNTIME_ROOT/bin/mariadbd" || ! -x "$RUNTIME_ROOT/bin/mariadb" \
  || ! -x "$RUNTIME_ROOT/scripts/mariadb-install-db" ]]
then
  echo "Runtime does not contain the required MariaDB binaries: $RUNTIME_ROOT" >&2
  exit 1
fi

mkdir -p "$ARTIFACT_DIR"
"$SCRIPT_DIR/bundle-macos-dylibs.sh" "$RUNTIME_ROOT"

"$RUNTIME_ROOT/bin/mariadbd" --no-defaults --version
"$RUNTIME_ROOT/bin/mariadb" --no-defaults --version

COPYFILE_DISABLE=1 tar -czf "$ARTIFACT_DIR/$ARCHIVE_NAME" -C "$(dirname "$RUNTIME_ROOT")" "$(basename "$RUNTIME_ROOT")"
artifact_sha256="$(shasum -a 256 "$ARTIFACT_DIR/$ARCHIVE_NAME" | awk '{print $1}')"
artifact_size="$(stat -f '%z' "$ARTIFACT_DIR/$ARCHIVE_NAME")"

sed \
  -e "s|@NAME@|mariadb|g" \
  -e "s|@VERSION@|$MARIADB_VERSION|g" \
  -e "s|@ARCHIVE@|$ARCHIVE_NAME|g" \
  -e "s|@SIZE@|$artifact_size|g" \
  -e "s|@SHA256@|$artifact_sha256|g" \
  -e "s|@SIGNATURE@|development-ad-hoc|g" \
  "$PROJECT_DIR/resources/runtime/release.template.json" \
  > "$ARTIFACT_DIR/mariadb-$MARIADB_VERSION-macos-arm64-dev.json"

echo "Created $ARTIFACT_DIR/$ARCHIVE_NAME"
echo "SHA-256: $artifact_sha256"
