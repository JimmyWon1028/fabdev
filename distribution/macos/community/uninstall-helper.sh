#!/usr/bin/env bash

set -euo pipefail

if [[ "$(id -u)" -ne 0 ]]; then
  echo "請使用移除程式授權管理員權限。" >&2
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
  if [[ "$managed" != "community" && "$managed" != "local-test" ]]; then
    echo "拒絕移除非 fabDev Community 管理的 LaunchDaemon：$PLIST_PATH" >&2
    exit 1
  fi
elif [[ -e "$HELPER_PATH" ]]; then
  echo "拒絕移除缺少 fabDev Community LaunchDaemon 標記的 Helper：$HELPER_PATH" >&2
  exit 1
fi

/bin/launchctl bootout "$SERVICE_TARGET" 2>/dev/null || true
if [[ -x "$HELPER_PATH" ]]; then
  "$HELPER_PATH" --remove-local-test-resolver
fi
/bin/rm -f "$HELPER_PATH"
/bin/rm -f "$PLIST_PATH"

echo "fabDev Community System Helper 已移除。"
