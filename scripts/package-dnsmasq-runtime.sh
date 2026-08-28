#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "Usage: $0 <runtime-root> <dnsmasq-version> <artifact-dir>" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUNTIME_ROOT="$1"
DNSMASQ_VERSION="$2"
ARTIFACT_DIR="$3"
ARCHIVE_NAME="dnsmasq-$DNSMASQ_VERSION-macos-arm64-dev.tar.gz"

if [[ ! -x "$RUNTIME_ROOT/sbin/dnsmasq" ]]; then
  echo "Runtime does not contain a dnsmasq binary: $RUNTIME_ROOT" >&2
  exit 1
fi

mkdir -p "$ARTIFACT_DIR"
"$SCRIPT_DIR/bundle-macos-dylibs.sh" "$RUNTIME_ROOT"

"$RUNTIME_ROOT/sbin/dnsmasq" --version
"$RUNTIME_ROOT/sbin/dnsmasq" --test --conf-file="$RUNTIME_ROOT/fabdev.conf.template"

tar -czf "$ARTIFACT_DIR/$ARCHIVE_NAME" -C "$(dirname "$RUNTIME_ROOT")" "$(basename "$RUNTIME_ROOT")"
artifact_sha256="$(shasum -a 256 "$ARTIFACT_DIR/$ARCHIVE_NAME" | awk '{print $1}')"
artifact_size="$(stat -f '%z' "$ARTIFACT_DIR/$ARCHIVE_NAME")"

sed \
  -e "s|@NAME@|dnsmasq|g" \
  -e "s|@VERSION@|$DNSMASQ_VERSION|g" \
  -e "s|@ARCHIVE@|$ARCHIVE_NAME|g" \
  -e "s|@SIZE@|$artifact_size|g" \
  -e "s|@SHA256@|$artifact_sha256|g" \
  -e "s|@SIGNATURE@|development-ad-hoc|g" \
  "$SCRIPT_DIR/../resources/runtime/release.template.json" \
  > "$ARTIFACT_DIR/dnsmasq-$DNSMASQ_VERSION-macos-arm64-dev.json"

echo "Created $ARTIFACT_DIR/$ARCHIVE_NAME"
echo "SHA-256: $artifact_sha256"
