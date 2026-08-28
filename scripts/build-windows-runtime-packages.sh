#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PHP_VERSION="${PHP_VERSION:-8.4.24}"
PHP_SHA256="${PHP_SHA256:-86470a30cbbaeafb259e727dfa5cd336f2f3f0a462cd6f8e3eac00fdbded13cb}"
MARIADB_VERSION="${MARIADB_VERSION:-12.3.2}"
MARIADB_SHA256="${MARIADB_SHA256:-67347c129eb9c5923d002ea34fbfa27c60eb95d36dd73b85af2651cdeceecac5}"
MARIADB_RELEASE_FINGERPRINT="${MARIADB_RELEASE_FINGERPRINT:-177F4010FE56CA3336300305F1656F24C74CD1D8}"
NODE_VERSION="${NODE_VERSION:-24.19.0}"
NODE_SHA256="${NODE_SHA256:-57f71ab3652e797d84acddc79c81cc9ff1c6ddb2a1974cdb83f00fee9bff4c73}"
NODE_RELEASE_FINGERPRINT="${NODE_RELEASE_FINGERPRINT:-5BE8A3F6C8A5C01D106C0AD820B1A390B168D356}"
BUILD_ROOT="${FABDEV_BUILD_ROOT:-$PROJECT_DIR/.build/windows-runtime-packages}"
DOWNLOAD_DIR="$BUILD_ROOT/downloads"
EXPANDED_DIR="$BUILD_ROOT/expanded"
RUNTIME_DIR="$BUILD_ROOT/runtime"
GNUPG_HOME="$BUILD_ROOT/gnupg"
ARTIFACT_DIR="${FABDEV_ARTIFACT_DIR:-$PROJECT_DIR/artifacts/windows-x64/runtimes}"

required_commands=(curl gpg gpgconf grep jq shasum tar 7zz)
for command_name in "${required_commands[@]}"; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Missing required command: $command_name" >&2
    exit 1
  fi
done

cleanup_gnupg() {
  gpgconf --homedir "$GNUPG_HOME" --kill all >/dev/null 2>&1 || true
}
trap cleanup_gnupg EXIT

download() {
  local url="$1"
  local destination="$2"
  if [[ ! -f "$destination" ]]; then
    curl --fail --location --retry 3 "$url" --output "$destination"
  fi
}

verify_sha256() {
  local archive="$1"
  local expected="$2"
  echo "$expected  $archive" | shasum -a 256 --check
}

verify_detached_signature() {
  local archive="$1"
  local signature="$2"
  local key_file="$3"
  local fingerprint="$4"
  local label="$5"

  rm -rf "$GNUPG_HOME"
  mkdir -m 0700 "$GNUPG_HOME"
  gpg --batch --homedir "$GNUPG_HOME" --import "$key_file"
  local status
  status="$(gpg --batch --homedir "$GNUPG_HOME" --status-fd 1 \
    --verify "$signature" "$archive" 2>&1)"
  echo "$status"
  if ! grep -q "VALIDSIG $fingerprint" <<< "$status"; then
    echo "$label release signature did not match the pinned fingerprint" >&2
    exit 1
  fi
}

write_release() {
  local name="$1"
  local version="$2"
  local archive="$3"
  local signature="$4"
  local size
  local sha256
  size="$(stat -f '%z' "$archive")"
  sha256="$(shasum -a 256 "$archive" | awk '{print $1}')"
  jq -n \
    --arg name "$name" \
    --arg version "$version" \
    --arg url "$(basename "$archive")" \
    --argjson size "$size" \
    --arg sha256 "$sha256" \
    --arg signature "$signature" \
    '{
      name: $name,
      version: $version,
      platform: "windows",
      architecture: "x64",
      url: $url,
      size: $size,
      sha256: $sha256,
      signature: $signature
    }' > "$ARTIFACT_DIR/$name-$version-windows-x64.json"
}

package_runtime() {
  local name="$1"
  local version="$2"
  local signature="$3"
  local archive="$ARTIFACT_DIR/$name-$version-windows-x64.tar.gz"
  COPYFILE_DISABLE=1 tar -czf "$archive" -C "$RUNTIME_DIR/$name" "$version"
  write_release "$name" "$version" "$archive" "$signature"
  echo "Created $archive"
}

mkdir -p "$DOWNLOAD_DIR" "$EXPANDED_DIR" "$RUNTIME_DIR" "$ARTIFACT_DIR"

php_archive="$DOWNLOAD_DIR/php-$PHP_VERSION-nts-Win32-vs17-x64.zip"
download \
  "https://windows.php.net/downloads/releases/archives/$(basename "$php_archive")" \
  "$php_archive"
verify_sha256 "$php_archive" "$PHP_SHA256"
rm -rf "$EXPANDED_DIR/php-$PHP_VERSION" "$RUNTIME_DIR/php/$PHP_VERSION"
mkdir -p "$EXPANDED_DIR/php-$PHP_VERSION" "$RUNTIME_DIR/php/$PHP_VERSION"
7zz x -y "$php_archive" "-o$EXPANDED_DIR/php-$PHP_VERSION" >/dev/null
cp -R "$EXPANDED_DIR/php-$PHP_VERSION/." "$RUNTIME_DIR/php/$PHP_VERSION/"
[[ -f "$RUNTIME_DIR/php/$PHP_VERSION/php.exe" ]]
[[ -f "$RUNTIME_DIR/php/$PHP_VERSION/php-cgi.exe" ]]
package_runtime "php" "$PHP_VERSION" "official-archive-sha256"

mariadb_archive="$DOWNLOAD_DIR/mariadb-$MARIADB_VERSION-winx64.zip"
mariadb_signature="$mariadb_archive.asc"
mariadb_key="$DOWNLOAD_DIR/mariadb-release.key"
download \
  "https://archive.mariadb.org/mariadb-$MARIADB_VERSION/winx64-packages/$(basename "$mariadb_archive")" \
  "$mariadb_archive"
download \
  "https://archive.mariadb.org/mariadb-$MARIADB_VERSION/winx64-packages/$(basename "$mariadb_signature")" \
  "$mariadb_signature"
download "https://archive.mariadb.org/PublicKey" "$mariadb_key"
verify_sha256 "$mariadb_archive" "$MARIADB_SHA256"
verify_detached_signature \
  "$mariadb_archive" \
  "$mariadb_signature" \
  "$mariadb_key" \
  "$MARIADB_RELEASE_FINGERPRINT" \
  "MariaDB"
rm -rf "$EXPANDED_DIR/mariadb-$MARIADB_VERSION" "$RUNTIME_DIR/mariadb/$MARIADB_VERSION"
mkdir -p "$EXPANDED_DIR/mariadb-$MARIADB_VERSION" "$RUNTIME_DIR/mariadb/$MARIADB_VERSION"
7zz x -y "$mariadb_archive" "-o$EXPANDED_DIR/mariadb-$MARIADB_VERSION" >/dev/null
mariadb_source="$EXPANDED_DIR/mariadb-$MARIADB_VERSION/mariadb-$MARIADB_VERSION-winx64"
cp -R "$mariadb_source/." "$RUNTIME_DIR/mariadb/$MARIADB_VERSION/"
[[ -f "$RUNTIME_DIR/mariadb/$MARIADB_VERSION/bin/mariadbd.exe" ]]
[[ -f "$RUNTIME_DIR/mariadb/$MARIADB_VERSION/bin/mariadb.exe" ]]
[[ -f "$RUNTIME_DIR/mariadb/$MARIADB_VERSION/bin/mariadb-install-db.exe" ]]
package_runtime \
  "mariadb" \
  "$MARIADB_VERSION" \
  "pgp:$MARIADB_RELEASE_FINGERPRINT"

node_archive="$DOWNLOAD_DIR/node-v$NODE_VERSION-win-x64.zip"
node_checksums_signature="$DOWNLOAD_DIR/SHASUMS256.txt.asc"
node_checksums="$DOWNLOAD_DIR/SHASUMS256.txt"
node_key="$DOWNLOAD_DIR/node-release-key.asc"
download "https://nodejs.org/dist/v$NODE_VERSION/$(basename "$node_archive")" "$node_archive"
download "https://nodejs.org/dist/v$NODE_VERSION/SHASUMS256.txt.asc" "$node_checksums_signature"
download \
  "https://raw.githubusercontent.com/nodejs/release-keys/main/keys/$NODE_RELEASE_FINGERPRINT.asc" \
  "$node_key"
verify_sha256 "$node_archive" "$NODE_SHA256"
rm -rf "$GNUPG_HOME"
mkdir -m 0700 "$GNUPG_HOME"
gpg --batch --homedir "$GNUPG_HOME" --import "$node_key"
node_status="$(gpg --batch --homedir "$GNUPG_HOME" --status-fd 1 \
  --output "$node_checksums" --decrypt "$node_checksums_signature" 2>&1)"
echo "$node_status"
if ! grep -q "VALIDSIG $NODE_RELEASE_FINGERPRINT" <<< "$node_status"; then
  echo "Node.js release signature did not match the pinned fingerprint" >&2
  exit 1
fi
if ! grep -q "^$NODE_SHA256  $(basename "$node_archive")$" "$node_checksums"; then
  echo "Node.js signed checksums do not contain the pinned archive hash" >&2
  exit 1
fi
rm -rf "$EXPANDED_DIR/node-$NODE_VERSION" "$RUNTIME_DIR/node/$NODE_VERSION"
mkdir -p "$EXPANDED_DIR/node-$NODE_VERSION" "$RUNTIME_DIR/node/$NODE_VERSION"
7zz x -y "$node_archive" "-o$EXPANDED_DIR/node-$NODE_VERSION" >/dev/null
node_source="$EXPANDED_DIR/node-$NODE_VERSION/node-v$NODE_VERSION-win-x64"
cp -R "$node_source/." "$RUNTIME_DIR/node/$NODE_VERSION/"
[[ -f "$RUNTIME_DIR/node/$NODE_VERSION/node.exe" ]]
[[ -f "$RUNTIME_DIR/node/$NODE_VERSION/npm.cmd" ]]
package_runtime "node" "$NODE_VERSION" "pgp:$NODE_RELEASE_FINGERPRINT"

echo "Windows x64 Runtime Packages are ready in $ARTIFACT_DIR"
