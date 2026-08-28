#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKAGE_DIR="$REPO_ROOT/helpers/macos"
HELPER_PATH="$PACKAGE_DIR/.build/release/fabdev-system-helper"
CACHE_DIR="${TMPDIR:-/tmp}/fabdev-swift-module-cache"
CODESIGN_IDENTITY="${FABDEV_CODESIGN_IDENTITY:--}"

mkdir -p "$CACHE_DIR"

env \
  CLANG_MODULE_CACHE_PATH="$CACHE_DIR" \
  SWIFTPM_MODULECACHE_OVERRIDE="$CACHE_DIR" \
  MACOSX_DEPLOYMENT_TARGET=13.0 \
  xcrun swift build \
    --disable-sandbox \
    --configuration release \
    --package-path "$PACKAGE_DIR"

if [[ "${FABDEV_RELEASE_BUILD:-0}" == "1" && "$CODESIGN_IDENTITY" == "-" ]]; then
  echo "FABDEV_CODESIGN_IDENTITY is required for release builds" >&2
  exit 1
fi

if [[ "$CODESIGN_IDENTITY" == "-" ]]; then
  codesign --force --sign - --identifier com.fabdev.system-helper "$HELPER_PATH"
else
  codesign \
    --force \
    --options runtime \
    --timestamp \
    --sign "$CODESIGN_IDENTITY" \
    --identifier com.fabdev.system-helper \
    "$HELPER_PATH"
fi

codesign --verify --strict --verbose=2 "$HELPER_PATH"
plutil -lint "$REPO_ROOT/apps/desktop/src-tauri/macos/com.fabdev.system-helper.plist"
