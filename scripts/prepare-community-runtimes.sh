#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ARTIFACT_DIR="${FABDEV_ARTIFACT_DIR:-$PROJECT_DIR/artifacts}"
OUTPUT_DIR="${FABDEV_COMMUNITY_RUNTIME_DIR:-$ARTIFACT_DIR/community-runtimes}"
MANIFEST_PATH="${1:-$PROJECT_DIR/resources/runtime-packages/macos-arm64-bundled.json}"

if ! command -v jq >/dev/null 2>&1; then
  echo "Missing required command: jq" >&2
  exit 1
fi

if [[ ! -f "$MANIFEST_PATH" ]]; then
  echo "Runtime package manifest does not exist: $MANIFEST_PATH" >&2
  exit 1
fi

/bin/mkdir -p "$OUTPUT_DIR"
/usr/bin/find "$OUTPUT_DIR" -maxdepth 1 -type f -delete

while IFS= read -r runtime; do
  name="$(jq -r '.name' <<< "$runtime")"
  version="$(jq -r '.version' <<< "$runtime")"
  source_stem="$name-$version-macos-arm64-dev"
  community_stem="$name-$version-macos-arm64-community"
  source_archive="$ARTIFACT_DIR/$source_stem.tar.gz"
  source_descriptor="$ARTIFACT_DIR/$source_stem.json"
  community_archive="$OUTPUT_DIR/$community_stem.tar.gz"
  community_descriptor="$OUTPUT_DIR/$community_stem.json"

  if [[ ! -f "$source_archive" || ! -f "$source_descriptor" ]]; then
    echo "Missing development Runtime Package: $source_stem" >&2
    exit 1
  fi

  expected="$(/usr/bin/plutil -extract sha256 raw -o - "$source_descriptor")"
  actual="$(/usr/bin/shasum -a 256 "$source_archive" | /usr/bin/awk '{print $1}')"
  if [[ "$actual" != "$expected" ]]; then
    echo "Runtime SHA-256 mismatch: $source_archive" >&2
    exit 1
  fi

  /bin/cp "$source_archive" "$community_archive"
  size="$(/usr/bin/stat -f '%z' "$community_archive")"
  sed \
    -e "s|@NAME@|$name|g" \
    -e "s|@VERSION@|$version|g" \
    -e "s|@ARCHIVE@|$community_stem.tar.gz|g" \
    -e "s|@SIZE@|$size|g" \
    -e "s|@SHA256@|$actual|g" \
    -e "s|@SIGNATURE@|community-ad-hoc|g" \
    "$PROJECT_DIR/resources/runtime/release.template.json" \
    > "$community_descriptor"
done < <(jq -c '.packages[]' "$MANIFEST_PATH")

generated_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
jq --slurp \
  --arg generated_at "$generated_at" \
  '{schemaVersion: 1, channel: "community", generatedAt: $generated_at, runtimes: .}' \
  "$OUTPUT_DIR"/*-macos-arm64-community.json \
  > "$OUTPUT_DIR/catalog.json"

echo "Created Community Runtime packages in $OUTPUT_DIR"
