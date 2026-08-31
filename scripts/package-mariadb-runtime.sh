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
PACKAGE_VARIANT="${FABDEV_RUNTIME_PACKAGE_VARIANT:-dev}"
case "$PACKAGE_VARIANT" in
  dev|community) ;;
  *)
    echo "Unsupported Runtime Package variant: $PACKAGE_VARIANT" >&2
    exit 1
    ;;
esac
ARCHIVE_NAME="mariadb-$MARIADB_VERSION-macos-arm64-$PACKAGE_VARIANT.tar.gz"

if [[ ! -x "$RUNTIME_ROOT/bin/mariadbd" || ! -x "$RUNTIME_ROOT/bin/mariadb" \
  || ! -x "$RUNTIME_ROOT/scripts/mariadb-install-db" ]]
then
  echo "Runtime does not contain the required MariaDB binaries: $RUNTIME_ROOT" >&2
  exit 1
fi

mkdir -p "$ARTIFACT_DIR"
if [[ "$PACKAGE_VARIANT" == "community" ]]; then
  if [[ -z "${FABDEV_RUNTIME_DEPENDENCY_PREFIX:-}" ]]; then
    echo "Community Runtime packaging requires FABDEV_RUNTIME_DEPENDENCY_PREFIX" >&2
    exit 1
  fi
  "$SCRIPT_DIR/bundle-macos-dylibs.sh" \
    "$RUNTIME_ROOT" \
    "$FABDEV_RUNTIME_DEPENDENCY_PREFIX"
else
  "$SCRIPT_DIR/bundle-macos-dylibs.sh" "$RUNTIME_ROOT"
fi

"$RUNTIME_ROOT/bin/mariadbd" --no-defaults --version
"$RUNTIME_ROOT/bin/mariadb" --no-defaults --version
"$SCRIPT_DIR/validate-mariadb-runtime-health.sh" "$RUNTIME_ROOT"

if [[ "$PACKAGE_VARIANT" == "community" ]]; then
  "$SCRIPT_DIR/validate-macos-runtime-minimum.sh" \
    "$RUNTIME_ROOT" \
    "${FABDEV_MINIMUM_MACOS_VERSION:-13.0}"
fi

COPYFILE_DISABLE=1 tar -czf "$ARTIFACT_DIR/$ARCHIVE_NAME" -C "$(dirname "$RUNTIME_ROOT")" "$(basename "$RUNTIME_ROOT")"
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
    -e "s|@NAME@|mariadb|g" \
    -e "s|@VERSION@|$MARIADB_VERSION|g" \
    -e "s|@ARCHIVE@|$ARCHIVE_NAME|g" \
    -e "s|@SIZE@|$artifact_size|g" \
    -e "s|@SHA256@|$artifact_sha256|g" \
    -e "s|@SIGNATURE@|development-ad-hoc|g" \
    "$PROJECT_DIR/resources/runtime/release.template.json" \
    > "$ARTIFACT_DIR/mariadb-$MARIADB_VERSION-macos-arm64-dev.json"
else
  printf '%s  %s\n' "$artifact_sha256" "$ARCHIVE_NAME" \
    > "$ARTIFACT_DIR/$ARCHIVE_NAME.sha256"
fi

echo "Created $ARTIFACT_DIR/$ARCHIVE_NAME"
echo "SHA-256: $artifact_sha256"
