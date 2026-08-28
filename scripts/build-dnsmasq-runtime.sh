#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DNSMASQ_VERSION="${DNSMASQ_VERSION:-2.93}"
DNSMASQ_SHA256="${DNSMASQ_SHA256:-0c00d4e5c97c8306e5fb932b348b34269c9c29a0e7df0e8e82958b407092bc19}"
DNSMASQ_RELEASE_FINGERPRINT="${DNSMASQ_RELEASE_FINGERPRINT:-D6EACBD6EE46B834248D111215CDDA6AE19135A2}"
BUILD_ROOT="${FABDEV_BUILD_ROOT:-$PROJECT_DIR/.build/dnsmasq-$DNSMASQ_VERSION}"
DOWNLOAD_DIR="$BUILD_ROOT/downloads"
SOURCE_DIR="$BUILD_ROOT/source/dnsmasq-$DNSMASQ_VERSION"
RUNTIME_ROOT="${FABDEV_RUNTIME_PREFIX:-$BUILD_ROOT/runtime/dnsmasq/$DNSMASQ_VERSION}"
ARTIFACT_DIR="${FABDEV_ARTIFACT_DIR:-$PROJECT_DIR/artifacts}"
MACOS_TARGET="${MACOSX_DEPLOYMENT_TARGET:-$(sw_vers -productVersion | cut -d. -f1).0}"
SOURCE_ARCHIVE="$DOWNLOAD_DIR/dnsmasq-$DNSMASQ_VERSION.tar.xz"
SOURCE_SIGNATURE="$SOURCE_ARCHIVE.asc"
GNUPG_HOME="$DOWNLOAD_DIR/gnupg"

cleanup_gnupg() {
  gpgconf --homedir "$GNUPG_HOME" --kill all >/dev/null 2>&1 || true
}
trap cleanup_gnupg EXIT

required_commands=(curl gpg gpgconf make shasum tar xcrun)
for command_name in "${required_commands[@]}"; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Missing required command: $command_name" >&2
    exit 1
  fi
done

mkdir -p "$DOWNLOAD_DIR" "$ARTIFACT_DIR"

if [[ ! -f "$SOURCE_ARCHIVE" ]] || ! echo "$DNSMASQ_SHA256  $SOURCE_ARCHIVE" | shasum -a 256 --check --status; then
  curl --fail --location --retry 3 \
    "https://thekelleys.org.uk/dnsmasq/dnsmasq-$DNSMASQ_VERSION.tar.xz" \
    --output "$SOURCE_ARCHIVE"
fi
if [[ ! -f "$SOURCE_SIGNATURE" ]]; then
  curl --fail --location --retry 3 \
    "https://thekelleys.org.uk/dnsmasq/dnsmasq-$DNSMASQ_VERSION.tar.xz.asc" \
    --output "$SOURCE_SIGNATURE"
fi

echo "$DNSMASQ_SHA256  $SOURCE_ARCHIVE" | shasum -a 256 --check
rm -rf "$GNUPG_HOME"
mkdir -m 0700 "$GNUPG_HOME"
gpg --batch --homedir "$GNUPG_HOME" \
  --keyserver hkps://keyring.debian.org \
  --recv-keys "$DNSMASQ_RELEASE_FINGERPRINT"
gpg_status="$(gpg --batch --homedir "$GNUPG_HOME" --status-fd 1 --verify "$SOURCE_SIGNATURE" "$SOURCE_ARCHIVE" 2>&1)"
echo "$gpg_status"
if ! grep -q "VALIDSIG $DNSMASQ_RELEASE_FINGERPRINT" <<< "$gpg_status"; then
  echo "dnsmasq release signature did not match the pinned fingerprint" >&2
  exit 1
fi

rm -rf "$SOURCE_DIR" "$RUNTIME_ROOT"
mkdir -p "$(dirname "$SOURCE_DIR")" "$RUNTIME_ROOT/sbin" "$RUNTIME_ROOT/share/man/man8" "$RUNTIME_ROOT/lib"
tar -xJf "$SOURCE_ARCHIVE" -C "$(dirname "$SOURCE_DIR")"

export MACOSX_DEPLOYMENT_TARGET="$MACOS_TARGET"
cd "$SOURCE_DIR"
make -j "$(sysctl -n hw.logicalcpu)" \
  COPTS="-DNO_DHCP -DNO_DHCP6 -DNO_TFTP -DNO_SCRIPT -DNO_AUTH -DNO_DUMPFILE" \
  LDFLAGS="-Wl,-headerpad_max_install_names"

install -m 0755 "$SOURCE_DIR/src/dnsmasq" "$RUNTIME_ROOT/sbin/dnsmasq"
install -m 0644 "$SOURCE_DIR/man/dnsmasq.8" "$RUNTIME_ROOT/share/man/man8/dnsmasq.8"
cp "$SOURCE_DIR/COPYING" "$RUNTIME_ROOT/COPYING"
cp "$PROJECT_DIR/resources/dnsmasq/fabdev.conf" "$RUNTIME_ROOT/fabdev.conf.template"

"$SCRIPT_DIR/package-dnsmasq-runtime.sh" "$RUNTIME_ROOT" "$DNSMASQ_VERSION" "$ARTIFACT_DIR"
