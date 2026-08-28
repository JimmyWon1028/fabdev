#!/usr/bin/env bash

set -euo pipefail

if [[ "$(id -u)" -ne 0 ]]; then
  echo "Run this uninstaller with sudo." >&2
  exit 1
fi

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

if [[ -e "$PLIST_PATH" ]]; then
  managed="$(read_managed_marker "$PLIST_PATH")"
  if [[ "$managed" != "local-test" ]]; then
    echo "Refusing to remove unmanaged LaunchDaemon: $PLIST_PATH" >&2
    exit 1
  fi
fi

/bin/launchctl bootout "$SERVICE_TARGET" 2>/dev/null || true
if [[ -x "$HELPER_PATH" ]]; then
  "$HELPER_PATH" --remove-local-test-resolver
fi
/bin/rm -f "$HELPER_PATH"
/bin/rm -f "$PLIST_PATH"

echo "fabDev local test helper removed."
