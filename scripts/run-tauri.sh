#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_PATH="$(rustup which cargo)"
RUST_BIN_DIR="${CARGO_PATH%/cargo}"

export PATH="$RUST_BIN_DIR:$PATH"
if [[ "$(uname -s)" == "Darwin" && "${1:-}" == "dev" ]]; then
  #2026.10.25 by codex, use the active Apple SDK and matching compilers for native dev builds.
  export SDKROOT="${SDKROOT:-$(xcrun --sdk macosx --show-sdk-path)}"
  export CC="${CC:-$(xcrun --sdk macosx --find clang)}"
  export CXX="${CXX:-$(xcrun --sdk macosx --find clang++)}"
  export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-13.0}"
fi
if [[ "${1:-}" == "build" ]]; then
  export CARGO_PROFILE_RELEASE_STRIP="${CARGO_PROFILE_RELEASE_STRIP:-none}"
fi
if [[ "$(uname -s)" == "Darwin" && ( "${1:-}" == "dev" || "${1:-}" == "build" ) ]]; then
  node "$REPO_ROOT/scripts/prepare-bundled-runtime-assets.mjs"
fi
if [[ "${1:-}" == "dev" ]]; then
  HOST_TRIPLE="$(rustup run stable rustc -vV | awk '/^host:/ { print $2 }')"
  AGENT_TARGET_DIR="$REPO_ROOT/.build/desktop-agent-dev"
  CARGO_TARGET_DIR="$AGENT_TARGET_DIR" "$CARGO_PATH" build -p fabdev-agent
  mkdir -p "$REPO_ROOT/apps/desktop/src-tauri/binaries"
  install -m 755 \
    "$AGENT_TARGET_DIR/debug/fabdev-agent" \
    "$REPO_ROOT/apps/desktop/src-tauri/binaries/fabdev-agent-$HOST_TRIPLE"
  if [[ "$(uname -s)" == "Darwin" ]]; then
    DEV_DATA_DIR="${FABDEV_DATA_DIR:-$HOME/Library/Application Support/FabDev}"
    "$REPO_ROOT/scripts/prepare-macos-dev-agent-app.sh" \
      "$AGENT_TARGET_DIR/debug/fabdev-agent" \
      "$REPO_ROOT/crates/agent/Info.plist" \
      "$AGENT_TARGET_DIR/fabDev Agent Dev.app" \
      "$DEV_DATA_DIR"
  fi
fi
exec pnpm --filter @fabdev/desktop tauri "$@"
