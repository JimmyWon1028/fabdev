#!/usr/bin/env bash

set -euo pipefail

if [[ "$(id -u)" -ne 0 ]]; then
  echo "Run this installer with sudo." >&2
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_HELPER="$REPO_ROOT/helpers/macos/.build/release/fabdev-system-helper"
SOURCE_PLIST="$REPO_ROOT/helpers/macos/com.fabdev.system-helper.local-test.plist"
HELPER_PATH="/Library/PrivilegedHelperTools/com.fabdev.system-helper"
PLIST_PATH="/Library/LaunchDaemons/com.fabdev.system-helper.plist"
SERVICE_TARGET="system/com.fabdev.system-helper"
MANAGED_KEY="fabDevManaged"
LEGACY_MANAGED_KEY="Fab""DevManaged"

read_managed_marker() {
  local path="$1"
  local managed
  managed="$(/usr/libexec/PlistBuddy -c "Print :$MANAGED_KEY" "$path" 2>/dev/null || true)"
  if [[ -z "$managed" ]]; then
    managed="$(/usr/libexec/PlistBuddy -c "Print :$LEGACY_MANAGED_KEY" "$path" 2>/dev/null || true)"
  fi
  echo "$managed"
}

if [[ ! -x "$SOURCE_HELPER" ]]; then
  echo "Build the helper first with: pnpm run build:helper:macos" >&2
  exit 1
fi

/usr/bin/codesign --verify --strict "$SOURCE_HELPER"
/usr/bin/plutil -lint "$SOURCE_PLIST" >/dev/null

if [[ -e "$PLIST_PATH" ]]; then
  managed="$(read_managed_marker "$PLIST_PATH")"
  if [[ "$managed" != "local-test" ]]; then
    echo "Refusing to replace unmanaged LaunchDaemon: $PLIST_PATH" >&2
    exit 1
  fi
  /bin/launchctl bootout "$SERVICE_TARGET" 2>/dev/null || true
fi

if /usr/sbin/lsof -nP -iUDP:53 2>/dev/null | /usr/bin/grep -q .; then
  echo "UDP port 53 is already in use." >&2
  exit 1
fi
if /usr/sbin/lsof -nP -iTCP:80 -sTCP:LISTEN 2>/dev/null | /usr/bin/grep -q .; then
  echo "TCP port 80 is already in use." >&2
  exit 1
fi
if /usr/sbin/lsof -nP -iTCP:443 -sTCP:LISTEN 2>/dev/null | /usr/bin/grep -q .; then
  echo "TCP port 443 is already in use." >&2
  exit 1
fi

/usr/bin/install -d -o root -g wheel -m 755 /Library/PrivilegedHelperTools
/usr/bin/install -o root -g wheel -m 755 "$SOURCE_HELPER" "$HELPER_PATH"
/usr/bin/install -o root -g wheel -m 644 "$SOURCE_PLIST" "$PLIST_PATH"
/bin/launchctl bootstrap system "$PLIST_PATH"
/bin/launchctl enable "$SERVICE_TARGET"
/bin/launchctl kickstart -k "$SERVICE_TARGET"

echo "fabDev local test helper installed and started."
