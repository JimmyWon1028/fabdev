#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
NODE_VERSION="${NODE_VERSION:-24.19.0}"
NODE_SHA256="${NODE_SHA256:-8294b7aa9b03997481c06babf1e8b270c859358f27da57a11509afe537ac381d}"
NODE_RELEASE_FINGERPRINT="${NODE_RELEASE_FINGERPRINT:-5BE8A3F6C8A5C01D106C0AD820B1A390B168D356}"
BUILD_ROOT="${FABDEV_BUILD_ROOT:-$PROJECT_DIR/.build/node-$NODE_VERSION}"
DOWNLOAD_DIR="$BUILD_ROOT/downloads"
RUNTIME_ROOT="${FABDEV_RUNTIME_PREFIX:-$BUILD_ROOT/runtime/node/$NODE_VERSION}"
ARTIFACT_DIR="${FABDEV_ARTIFACT_DIR:-$PROJECT_DIR/artifacts}"
ARCHIVE_NAME="node-v$NODE_VERSION-darwin-arm64.tar.gz"
SOURCE_ARCHIVE="$DOWNLOAD_DIR/$ARCHIVE_NAME"
CHECKSUMS_SIGNATURE="$DOWNLOAD_DIR/SHASUMS256.txt.asc"
CHECKSUMS_FILE="$DOWNLOAD_DIR/SHASUMS256.txt"
RELEASE_KEY="$DOWNLOAD_DIR/node-release-key.asc"
GNUPG_HOME="$DOWNLOAD_DIR/gnupg"

cleanup_gnupg() {
  gpgconf --homedir "$GNUPG_HOME" --kill all >/dev/null 2>&1 || true
}
trap cleanup_gnupg EXIT

required_commands=(curl gpg gpgconf grep shasum tar)
for command_name in "${required_commands[@]}"; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Missing required command: $command_name" >&2
    exit 1
  fi
done

if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "arm64" ]]; then
  echo "Node.js Runtime packaging currently supports macOS ARM64 only" >&2
  exit 1
fi

mkdir -p "$DOWNLOAD_DIR" "$ARTIFACT_DIR"

if [[ ! -f "$SOURCE_ARCHIVE" ]] || ! echo "$NODE_SHA256  $SOURCE_ARCHIVE" | shasum -a 256 --check --status; then
  curl --fail --location --retry 3 \
    "https://nodejs.org/dist/v$NODE_VERSION/$ARCHIVE_NAME" \
    --output "$SOURCE_ARCHIVE"
fi
if [[ ! -f "$CHECKSUMS_SIGNATURE" ]]; then
  curl --fail --location --retry 3 \
    "https://nodejs.org/dist/v$NODE_VERSION/SHASUMS256.txt.asc" \
    --output "$CHECKSUMS_SIGNATURE"
fi
if [[ ! -f "$RELEASE_KEY" ]]; then
  curl --fail --location --retry 3 \
    "https://raw.githubusercontent.com/nodejs/release-keys/main/keys/$NODE_RELEASE_FINGERPRINT.asc" \
    --output "$RELEASE_KEY"
fi

echo "$NODE_SHA256  $SOURCE_ARCHIVE" | shasum -a 256 --check
rm -rf "$GNUPG_HOME"
mkdir -m 0700 "$GNUPG_HOME"
gpg --batch --homedir "$GNUPG_HOME" --import "$RELEASE_KEY"
gpg_status="$(gpg --batch --homedir "$GNUPG_HOME" --status-fd 1 \
  --output "$CHECKSUMS_FILE" --decrypt "$CHECKSUMS_SIGNATURE" 2>&1)"
echo "$gpg_status"
if ! grep -q "VALIDSIG $NODE_RELEASE_FINGERPRINT" <<< "$gpg_status"; then
  echo "Node.js release signature did not match the pinned fingerprint" >&2
  exit 1
fi
if ! grep -q "^$NODE_SHA256  $ARCHIVE_NAME$" "$CHECKSUMS_FILE"; then
  echo "Node.js signed checksums do not contain the pinned archive hash" >&2
  exit 1
fi

rm -rf "$RUNTIME_ROOT"
mkdir -p "$RUNTIME_ROOT"
tar -xzf "$SOURCE_ARCHIVE" -C "$RUNTIME_ROOT" --strip-components=1

if [[ "$($RUNTIME_ROOT/bin/node --version)" != "v$NODE_VERSION" ]]; then
  echo "Extracted Node.js Runtime reported an unexpected version" >&2
  exit 1
fi
"$RUNTIME_ROOT/bin/npm" --version

"$SCRIPT_DIR/package-node-runtime.sh" "$RUNTIME_ROOT" "$NODE_VERSION" "$ARTIFACT_DIR"
