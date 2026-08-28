#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CACHE_DIR="${TMPDIR:-/tmp}/fabdev-swift-module-cache"

mkdir -p "$CACHE_DIR"

env \
  CLANG_MODULE_CACHE_PATH="$CACHE_DIR" \
  SWIFTPM_MODULECACHE_OVERRIDE="$CACHE_DIR" \
  xcrun swift test \
    --disable-sandbox \
    --package-path "$REPO_ROOT/helpers/macos"
