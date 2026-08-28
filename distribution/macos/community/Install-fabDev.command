#!/usr/bin/env bash

set -euo pipefail

if [[ "$(id -u)" -eq 0 ]]; then
  echo "請直接執行此安裝程式，不要使用 sudo；需要時會另外要求管理員密碼。" >&2
  exit 1
fi

PACKAGE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_SOURCE="$PACKAGE_ROOT/fabDev.app"
APP_TARGET="/Applications/fabDev.app"
LEGACY_BRAND="Fab""Dev"
LEGACY_APP_TARGET="/Applications/$LEGACY_BRAND.app"
SUPPORT_DIR="$PACKAGE_ROOT/Support"
COMMUNITY_CLI="$SUPPORT_DIR/bin/fabdev"
DATA_ROOT="$HOME/Library/Application Support/$LEGACY_BRAND"
DEMO_SOURCE="$SUPPORT_DIR/demo"
DEMO_TARGET="$HOME/$LEGACY_BRAND/Sites/demo"
TEMP_APP="/Applications/.fabDev.app.installing"
PREVIOUS_APP="/Applications/.fabDev.app.previous"
CURRENT_APP_TARGET="$APP_TARGET"

if [[ ! -d "$CURRENT_APP_TARGET" && -d "$LEGACY_APP_TARGET" ]]; then
  CURRENT_APP_TARGET="$LEGACY_APP_TARGET"
fi

if [[ "$(uname -m)" != "arm64" ]]; then
  echo "此版本只支援 Apple Silicon Mac。" >&2
  exit 1
fi

macos_major="$(/usr/bin/sw_vers -productVersion | /usr/bin/cut -d. -f1)"
if [[ "$macos_major" -lt 13 ]]; then
  echo "fabDev Community 需要 macOS 13 或更新版本。" >&2
  exit 1
fi

if [[ ! -d "$APP_SOURCE" || ! -x "$COMMUNITY_CLI" ]]; then
  echo "安裝包不完整，請重新下載 fabDev Community DMG。" >&2
  exit 1
fi

echo "正在驗證安裝包完整性…"
(
  cd "$PACKAGE_ROOT"
  /usr/bin/shasum -a 256 -c SHA256SUMS
)
/usr/bin/codesign --verify --deep --strict "$APP_SOURCE"

if /usr/bin/pgrep -x fabdev-desktop >/dev/null 2>&1; then
  echo "請先從 menu bar 選擇 Quit fabDev，再重新執行安裝程式。" >&2
  exit 1
fi

echo
echo "將安裝："
echo "  • fabDev.app 到 /Applications"
echo "  • App 內建 PHP 7.4.33、PHP 8.2.33"
echo "  • App 內建 Nginx 1.30.4 與 dnsmasq 2.93"
echo "  • PHP 8.4 與 MariaDB 可在 App 內另外安裝"
echo "  • 固定代理 53→53535、80→8080、443→8443 的 Community Helper"
echo "  • 全新安裝時建立 demo.test"
echo
read -r -p "繼續安裝？[y/N] " answer
if [[ "$answer" != "y" && "$answer" != "Y" ]]; then
  echo "已取消。"
  exit 0
fi

if [[ -e "$TEMP_APP" || -e "$PREVIOUS_APP" ]]; then
  echo "偵測到未完成的舊安裝，請先移除 $TEMP_APP 或 $PREVIOUS_APP。" >&2
  exit 1
fi
if [[ -d "$APP_TARGET" && -d "$LEGACY_APP_TARGET" ]]; then
  echo "偵測到新舊名稱的 App 同時存在，請先保留其中一份再更新。" >&2
  exit 1
fi
if [[ -d "$CURRENT_APP_TARGET" ]]; then
  existing_id="$(/usr/libexec/PlistBuddy -c "Print :CFBundleIdentifier" "$CURRENT_APP_TARGET/Contents/Info.plist" 2>/dev/null || true)"
  if [[ "$existing_id" != "com.fabdev.desktop" ]]; then
    echo "拒絕取代不是 fabDev 的 App：$CURRENT_APP_TARGET" >&2
    exit 1
  fi
fi

echo "需要管理員權限安裝 App 與固定 Port Helper。"
/usr/bin/sudo -v
/usr/bin/sudo /usr/bin/ditto "$APP_SOURCE" "$TEMP_APP"
/usr/bin/sudo /usr/sbin/chown -R root:wheel "$TEMP_APP"
/usr/bin/sudo /usr/bin/codesign --verify --deep --strict "$TEMP_APP"
if [[ -d "$CURRENT_APP_TARGET" ]]; then
  /usr/bin/sudo /bin/mv "$CURRENT_APP_TARGET" "$PREVIOUS_APP"
fi
if ! /usr/bin/sudo /bin/mv "$TEMP_APP" "$APP_TARGET"; then
  if [[ -d "$PREVIOUS_APP" ]]; then
    /usr/bin/sudo /bin/mv "$PREVIOUS_APP" "$CURRENT_APP_TARGET"
  fi
  exit 1
fi
if [[ -d "$PREVIOUS_APP" ]]; then
  /usr/bin/sudo /bin/rm -rf "$PREVIOUS_APP"
fi

/usr/bin/sudo "$SUPPORT_DIR/install-helper.sh"

if [[ ! -e "$DEMO_TARGET" ]]; then
  /bin/mkdir -p "$(dirname "$DEMO_TARGET")"
  /usr/bin/ditto "$DEMO_SOURCE" "$DEMO_TARGET"
elif [[ ! -f "$DEMO_TARGET/.fabdev-community-demo" ]]; then
  echo "既有路徑不是 fabDev Demo，未覆蓋：$DEMO_TARGET" >&2
else
  /usr/bin/ditto "$DEMO_SOURCE" "$DEMO_TARGET"
fi

if [[ -f "$DEMO_TARGET/.fabdev-community-demo" ]]; then
  "$COMMUNITY_CLI" seed-demo "$DEMO_TARGET" --data-dir "$DATA_ROOT"
fi

/usr/bin/sudo -k
echo
echo "fabDev Community 安裝完成。"
echo "若 macOS 阻擋第一次啟動，請在 Finder 對 fabDev 按右鍵並選擇「打開」。"
/usr/bin/open "$APP_TARGET"
