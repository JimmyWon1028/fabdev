#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 4 ]]; then
  echo "usage: $0 <agent-binary> <info-plist> <app-path> <data-dir>" >&2
  exit 64
fi

AGENT_BINARY="$1"
INFO_PLIST="$2"
APP_PATH="$3"
DATA_DIR="$4"
APP_MACOS_DIR="$APP_PATH/Contents/MacOS"
APP_AGENT="$APP_MACOS_DIR/fabdev-agent"
SOCKET_PATH="$DATA_DIR/state/agent.sock"
LOG_PATH="$DATA_DIR/logs/agent-process.log"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "macOS dev Agent App preparation is only supported on macOS" >&2
  exit 69
fi

mkdir -p "$APP_MACOS_DIR" "$DATA_DIR/state" "$DATA_DIR/logs"
install -m 755 "$AGENT_BINARY" "$APP_AGENT"
install -m 644 "$INFO_PLIST" "$APP_PATH/Contents/Info.plist"
codesign --force --deep --sign - "$APP_PATH"

active_agent_pid=""
if [[ -S "$SOCKET_PATH" ]]; then
  active_agent_pid="$(lsof -t -- "$SOCKET_PATH" 2>/dev/null | head -n 1 || true)"
fi
if [[ -n "$active_agent_pid" ]]; then
  exit 0
fi

if [[ -e "$SOCKET_PATH" ]]; then
  rm -f -- "$SOCKET_PATH"
fi

open -n --stdout "$LOG_PATH" --stderr "$LOG_PATH" "$APP_PATH" \
  --args --data-dir "$DATA_DIR"

for _ in {1..50}; do
  if [[ -S "$SOCKET_PATH" ]] && lsof -t -- "$SOCKET_PATH" >/dev/null 2>&1; then
    exit 0
  fi
  sleep 0.1
done

echo "fabDev macOS dev Agent App did not become ready" >&2
exit 1
