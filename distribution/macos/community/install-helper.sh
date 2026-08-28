#!/usr/bin/env bash

set -euo pipefail

if [[ "$(id -u)" -ne 0 ]]; then
  echo "請使用安裝程式授權管理員權限。" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PACKAGE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SOURCE_HELPER="$PACKAGE_ROOT/fabDev.app/Contents/Resources/fabdev-system-helper"
SOURCE_PLIST="$SCRIPT_DIR/com.fabdev.system-helper.plist"
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
  echo "找不到 fabDev System Helper：$SOURCE_HELPER" >&2
  exit 1
fi

/usr/bin/codesign --verify --strict "$SOURCE_HELPER"
/usr/bin/plutil -lint "$SOURCE_PLIST" >/dev/null

if [[ -e "$PLIST_PATH" ]]; then
  managed="$(read_managed_marker "$PLIST_PATH")"
  if [[ "$managed" != "community" && "$managed" != "local-test" ]]; then
    echo "拒絕取代非 fabDev Community 管理的 LaunchDaemon：$PLIST_PATH" >&2
    exit 1
  fi
  /bin/launchctl bootout "$SERVICE_TARGET" 2>/dev/null || true
fi

for _ in 1 2 3 4 5; do
  if ! /usr/sbin/lsof -nP -iUDP:53 2>/dev/null | /usr/bin/grep -q . \
    && ! /usr/sbin/lsof -nP -iTCP:80 -sTCP:LISTEN 2>/dev/null | /usr/bin/grep -q . \
    && ! /usr/sbin/lsof -nP -iTCP:443 -sTCP:LISTEN 2>/dev/null | /usr/bin/grep -q .
  then
    break
  fi
  /bin/sleep 0.2
done

if /usr/sbin/lsof -nP -iUDP:53 2>/dev/null | /usr/bin/grep -q .; then
  echo "UDP Port 53 已被其他程式使用，請先停止 Herd、Valet 或其他 DNS 服務。" >&2
  exit 1
fi
if /usr/sbin/lsof -nP -iTCP:80 -sTCP:LISTEN 2>/dev/null | /usr/bin/grep -q .; then
  echo "TCP Port 80 已被其他程式使用，請先停止其他 Web Server。" >&2
  exit 1
fi
if /usr/sbin/lsof -nP -iTCP:443 -sTCP:LISTEN 2>/dev/null | /usr/bin/grep -q .; then
  echo "TCP Port 443 已被其他程式使用，請先停止其他 Web Server。" >&2
  exit 1
fi

/usr/bin/install -d -o root -g wheel -m 755 /Library/PrivilegedHelperTools
/usr/bin/install -o root -g wheel -m 755 "$SOURCE_HELPER" "$HELPER_PATH"
/usr/bin/install -o root -g wheel -m 644 "$SOURCE_PLIST" "$PLIST_PATH"
/bin/launchctl bootstrap system "$PLIST_PATH"
/bin/launchctl enable "$SERVICE_TARGET"
/bin/launchctl kickstart -k "$SERVICE_TARGET"
/bin/launchctl print "$SERVICE_TARGET" >/dev/null

echo "fabDev Community System Helper 已安裝並啟動。"
