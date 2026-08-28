#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_TRIPLE="${TAURI_ENV_TARGET_TRIPLE:-}"
HOST_TRIPLE="$(rustup run stable rustc -vV | awk '/^host:/ { print $2 }')"
TARGET_TRIPLE="${TARGET_TRIPLE:-$HOST_TRIPLE}"
if [[ -z "$TARGET_TRIPLE" ]]; then
  echo "Unable to determine Rust target triple" >&2
  exit 1
fi

if [[ "$TARGET_TRIPLE" == "$HOST_TRIPLE" ]]; then
  CARGO_PROFILE_RELEASE_STRIP=none \
    "$REPO_ROOT/scripts/run-cargo.sh" build -p fabdev-agent --release
  BUILD_OUTPUT_DIR="$REPO_ROOT/target/release"
else
  CARGO_PROFILE_RELEASE_STRIP=none \
    "$REPO_ROOT/scripts/run-cargo.sh" build -p fabdev-agent --release --target "$TARGET_TRIPLE"
  BUILD_OUTPUT_DIR="$REPO_ROOT/target/$TARGET_TRIPLE/release"
fi

SOURCE_AGENT="$BUILD_OUTPUT_DIR/fabdev-agent"
BINARY_DIR="$REPO_ROOT/apps/desktop/src-tauri/binaries"
DESTINATION_AGENT="$BINARY_DIR/fabdev-agent-$TARGET_TRIPLE"
if [[ ! -x "$SOURCE_AGENT" ]]; then
  echo "fabDev Agent build output is missing: $SOURCE_AGENT" >&2
  exit 1
fi

mkdir -p "$BINARY_DIR"
install -m 755 "$SOURCE_AGENT" "$DESTINATION_AGENT"
echo "Prepared Desktop Agent sidecar: $DESTINATION_AGENT"
