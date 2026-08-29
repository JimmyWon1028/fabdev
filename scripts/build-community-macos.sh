#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_DIR="$REPO_ROOT/distribution/macos/community"
ARTIFACT_DIR="$REPO_ROOT/artifacts"
COMMUNITY_RUNTIME_DIR="$ARTIFACT_DIR/community-runtimes"
VERSION="$(/usr/bin/plutil -extract version raw -o - "$REPO_ROOT/apps/desktop/src-tauri/tauri.conf.json")"
APP_PATH="$REPO_ROOT/target/release/bundle/macos/fabDev.app"
CLI_PATH="$REPO_ROOT/target/release/fabdev"
DMG_PATH="$ARTIFACT_DIR/fabDev-Community-$VERSION-macos-arm64.dmg"
STAGING_ROOT="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/fabdev-community.XXXXXX")"
PACKAGE_ROOT="$STAGING_ROOT/fabDev Community $VERSION"

cleanup() {
  case "$STAGING_ROOT" in
    "${TMPDIR:-/tmp}"/fabdev-community.*) /bin/rm -rf "$STAGING_ROOT" ;;
    *) echo "拒絕清除非預期的暫存路徑：$STAGING_ROOT" >&2 ;;
  esac
}
trap cleanup EXIT

if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "arm64" ]]; then
  echo "Community DMG 必須在 Apple Silicon Mac 建置。" >&2
  exit 1
fi

runtime_stems=(
  dnsmasq-2.93-macos-arm64-community
  nginx-1.30.4-macos-arm64-community
  php-7.4.33-macos-arm64-community
  php-8.2.33-macos-arm64-community
)

FABDEV_BUNDLED_RUNTIMES_ONLY=1 \
  "$REPO_ROOT/scripts/prepare-community-runtimes.sh"

for stem in "${runtime_stems[@]}"; do
  archive="$COMMUNITY_RUNTIME_DIR/$stem.tar.gz"
  descriptor="$COMMUNITY_RUNTIME_DIR/$stem.json"
  if [[ ! -f "$archive" || ! -f "$descriptor" ]]; then
    echo "缺少 Runtime Package：$stem" >&2
    exit 1
  fi
  expected="$(/usr/bin/plutil -extract sha256 raw -o - "$descriptor")"
  actual="$(/usr/bin/shasum -a 256 "$archive" | /usr/bin/awk '{print $1}')"
  if [[ "$actual" != "$expected" ]]; then
    echo "Runtime SHA-256 不符：$archive" >&2
    exit 1
  fi
done

FABDEV_BUNDLED_RUNTIME_SOURCE="$COMMUNITY_RUNTIME_DIR" \
  "$REPO_ROOT/scripts/run-tauri.sh" build --bundles app
CARGO_PROFILE_RELEASE_STRIP=none \
  "$REPO_ROOT/scripts/run-cargo.sh" build --release -p fabdev-cli
/usr/bin/codesign --force --sign - --identifier com.fabdev.cli "$CLI_PATH"
/usr/bin/codesign --force --deep --sign - "$APP_PATH"
/usr/bin/codesign --verify --deep --strict --verbose=2 "$APP_PATH"

/bin/mkdir -p "$PACKAGE_ROOT/Support/bin"
/usr/bin/ditto "$APP_PATH" "$PACKAGE_ROOT/fabDev.app"
/bin/cp "$CLI_PATH" "$PACKAGE_ROOT/Support/bin/fabdev"
/bin/cp "$SOURCE_DIR/install-helper.sh" "$PACKAGE_ROOT/Support/install-helper.sh"
/bin/cp "$SOURCE_DIR/uninstall-helper.sh" "$PACKAGE_ROOT/Support/uninstall-helper.sh"
/bin/cp "$SOURCE_DIR/com.fabdev.system-helper.plist" \
  "$PACKAGE_ROOT/Support/com.fabdev.system-helper.plist"
/usr/bin/ditto "$SOURCE_DIR/demo" "$PACKAGE_ROOT/Support/demo"
/bin/cp "$SOURCE_DIR/Install-fabDev.command" "$PACKAGE_ROOT/Install-fabDev.command"
/bin/cp "$SOURCE_DIR/Uninstall-fabDev.command" "$PACKAGE_ROOT/Uninstall-fabDev.command"
/bin/cp "$SOURCE_DIR/INSTALL.zh-TW.md" "$PACKAGE_ROOT/安裝說明.md"

/bin/chmod 755 \
  "$PACKAGE_ROOT/Install-fabDev.command" \
  "$PACKAGE_ROOT/Uninstall-fabDev.command" \
  "$PACKAGE_ROOT/Support/install-helper.sh" \
  "$PACKAGE_ROOT/Support/uninstall-helper.sh" \
  "$PACKAGE_ROOT/Support/bin/fabdev"

(
  cd "$PACKAGE_ROOT"
  /usr/bin/find . -type f ! -name SHA256SUMS | LC_ALL=C /usr/bin/sort | while IFS= read -r path; do
    /usr/bin/shasum -a 256 "$path"
  done > SHA256SUMS
)

/bin/mkdir -p "$ARTIFACT_DIR"
/usr/bin/hdiutil create \
  -volname "fabDev Community $VERSION" \
  -srcfolder "$PACKAGE_ROOT" \
  -format UDZO \
  -ov \
  "$DMG_PATH"
/usr/bin/shasum -a 256 "$DMG_PATH" > "$DMG_PATH.sha256"

echo "Created $DMG_PATH"
echo "Created $DMG_PATH.sha256"
