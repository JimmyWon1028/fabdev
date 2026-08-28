#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ARTIFACT_DIR="${FABDEV_ARTIFACT_DIR:-$PROJECT_DIR/artifacts}"
CATALOG_PATH="${FABDEV_CATALOG_PATH:-$ARTIFACT_DIR/catalog.json}"

if ! command -v jq >/dev/null 2>&1; then
  echo "Missing required command: jq" >&2
  exit 1
fi

shopt -s nullglob
descriptors=("$ARTIFACT_DIR"/*-macos-arm64-dev.json)
if [[ "${#descriptors[@]}" -eq 0 ]]; then
  echo "No Runtime release descriptors found in $ARTIFACT_DIR" >&2
  exit 1
fi

generated_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
jq --slurp \
  --arg generated_at "$generated_at" \
  '{schemaVersion: 1, generatedAt: $generated_at, runtimes: .}' \
  "${descriptors[@]}" \
  > "$CATALOG_PATH"

echo "Created $CATALOG_PATH"
