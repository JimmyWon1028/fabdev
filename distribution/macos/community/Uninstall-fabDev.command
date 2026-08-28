#!/usr/bin/env bash

set -euo pipefail

if [[ "$(id -u)" -eq 0 ]]; then
  echo "請直接執行此移除程式，不要使用 sudo；需要時會另外要求管理員密碼。" >&2
  exit 1
fi

PACKAGE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SUPPORT_DIR="$PACKAGE_ROOT/Support"
COMMUNITY_CLI="$SUPPORT_DIR/bin/fabdev"
APP_TARGET="/Applications/fabDev.app"
LEGACY_BRAND="Fab""Dev"
LEGACY_APP_TARGET="/Applications/$LEGACY_BRAND.app"
DATA_ROOT="$HOME/Library/Application Support/$LEGACY_BRAND"
DEMO_TARGET="$HOME/$LEGACY_BRAND/Sites/demo"
TRASH_ROOT="$HOME/.Trash"
timestamp="$(/bin/date +%Y%m%d-%H%M%S)"

remove_user_ca_trust() {
  local keychain="$HOME/Library/Keychains/login.keychain-db"
  local common_name="fabDev Local Development CA"
  local expected_identity="CN=$common_name,O=fabDev"
  if [[ ! -f "$keychain" ]]; then
    return
  fi

  while true; do
    local certificate
    certificate="$(
      /usr/bin/security find-certificate -c "$common_name" -p "$keychain" 2>/dev/null || true
    )"
    if [[ -z "$certificate" ]]; then
      return
    fi

    local subject
    local issuer
    subject="$(
      /usr/bin/printf '%s\n' "$certificate" \
        | /usr/bin/openssl x509 -noout -subject -nameopt RFC2253 2>/dev/null \
        | /usr/bin/sed 's/^subject= *//' || true
    )"
    issuer="$(
      /usr/bin/printf '%s\n' "$certificate" \
        | /usr/bin/openssl x509 -noout -issuer -nameopt RFC2253 2>/dev/null \
        | /usr/bin/sed 's/^issuer= *//' || true
    )"
    if [[ "$subject" != "$expected_identity" || "$issuer" != "$expected_identity" ]]; then
      echo "保留無法驗證為 fabDev 自簽 CA 的 Login Keychain 憑證。" >&2
      return
    fi

    local fingerprint
    fingerprint="$(
      /usr/bin/printf '%s\n' "$certificate" \
        | /usr/bin/openssl x509 -outform DER 2>/dev/null \
        | /usr/bin/shasum -a 256 \
        | /usr/bin/awk '{ print $1 }'
    )"
    if [[ ! "$fingerprint" =~ ^[0-9a-fA-F]{64}$ ]]; then
      echo "保留無法計算 Fingerprint 的 fabDev CA。" >&2
      return
    fi
    if ! /usr/bin/security delete-certificate -t -Z "$fingerprint" "$keychain" >/dev/null 2>&1; then
      echo "無法從 Login Keychain 移除 fabDev CA。" >&2
      return
    fi
  done
}

if [[ ! -d "$APP_TARGET" && -d "$LEGACY_APP_TARGET" ]]; then
  APP_TARGET="$LEGACY_APP_TARGET"
fi

echo "此程序會停止 fabDev 服務、移除 System Helper，並將 fabDev.app 移到垃圾桶。"
echo "Sites、Runtime 與設定資料預設保留。"
read -r -p "繼續移除？[y/N] " answer
if [[ "$answer" != "y" && "$answer" != "Y" ]]; then
  echo "已取消。"
  exit 0
fi

if [[ -x "$COMMUNITY_CLI" ]]; then
  "$COMMUNITY_CLI" stop >/dev/null 2>&1 || true
fi
remove_user_ca_trust
/usr/bin/pkill -x fabdev-desktop 2>/dev/null || true
/usr/bin/pkill -x fabdev-agent 2>/dev/null || true

echo "需要管理員權限移除固定 Port Helper 與 App。"
/usr/bin/sudo -v
/usr/bin/sudo "$SUPPORT_DIR/uninstall-helper.sh"

/bin/mkdir -p "$TRASH_ROOT"
if [[ -d "$APP_TARGET" ]]; then
  app_trash="$TRASH_ROOT/fabDev.app-$timestamp"
  /usr/bin/sudo /bin/mv "$APP_TARGET" "$app_trash"
  /usr/bin/sudo /usr/sbin/chown -R "$(id -u):$(id -g)" "$app_trash"
  echo "fabDev.app 已移到垃圾桶：$app_trash"
fi

echo
read -r -p "是否也將 fabDev 設定、Runtime 與 Demo 移到垃圾桶？[y/N] " data_answer
if [[ "$data_answer" == "y" || "$data_answer" == "Y" ]]; then
  if [[ -d "$DATA_ROOT" ]]; then
    /bin/mv "$DATA_ROOT" "$TRASH_ROOT/fabDev-data-$timestamp"
  fi
  if [[ -f "$DEMO_TARGET/.fabdev-community-demo" ]]; then
    /bin/mv "$DEMO_TARGET" "$TRASH_ROOT/fabDev-demo-$timestamp"
  fi
  echo "fabDev 使用者資料已移到垃圾桶，可在清空垃圾桶前復原。"
else
  echo "fabDev 使用者資料已保留。"
fi

/usr/bin/sudo -k
echo "fabDev Community 已移除。"
