#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_PATH="$(rustup which cargo)"
RUST_BIN_DIR="${CARGO_PATH%/cargo}"

export PATH="$RUST_BIN_DIR:$PATH"
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
fi
exec pnpm --filter @fabdev/desktop tauri "$@"
