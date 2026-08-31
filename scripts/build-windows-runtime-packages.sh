#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PHP_VERSION="${PHP_VERSION:-8.4.24}"
PHP_SHA256="${PHP_SHA256:-86470a30cbbaeafb259e727dfa5cd336f2f3f0a462cd6f8e3eac00fdbded13cb}"
MARIADB_VERSION="${MARIADB_VERSION:-12.3.2}"
MARIADB_SHA256="${MARIADB_SHA256:-67347c129eb9c5923d002ea34fbfa27c60eb95d36dd73b85af2651cdeceecac5}"
MARIADB_RELEASE_FINGERPRINT="${MARIADB_RELEASE_FINGERPRINT:-177F4010FE56CA3336300305F1656F24C74CD1D8}"
NODE20_VERSION="${NODE20_VERSION:-20.20.2}"
NODE20_SHA256="${NODE20_SHA256:-dc3700fdd57a63eedb8fd7e3c7baaa32e6a740a1b904167ff4204bc68ed8bf77}"
NODE20_RELEASE_FINGERPRINT="${NODE20_RELEASE_FINGERPRINT:-CC68F5A3106FF448322E48ED27F5E38D5B0A215F}"
NODE24_VERSION="${NODE24_VERSION:-24.20.0}"
NODE24_SHA256="${NODE24_SHA256:-6cac9ffbca8f6a47091e4b5c772e0606049c3871cb67d900c0cedde630e545ba}"
NODE24_RELEASE_FINGERPRINT="${NODE24_RELEASE_FINGERPRINT:-5BE8A3F6C8A5C01D106C0AD820B1A390B168D356}"
BUILD_ROOT="${FABDEV_BUILD_ROOT:-$PROJECT_DIR/.build/windows-runtime-packages}"
DOWNLOAD_DIR="$BUILD_ROOT/downloads"
EXPANDED_DIR="$BUILD_ROOT/expanded"
RUNTIME_DIR="$BUILD_ROOT/runtime"
GNUPG_HOME="$BUILD_ROOT/gnupg"
ARTIFACT_DIR="${FABDEV_ARTIFACT_DIR:-$PROJECT_DIR/artifacts/windows-x64/runtimes}"
WINDOWS_RUNTIME_NAMES="${FABDEV_WINDOWS_RUNTIME_NAMES:-php mariadb node}"

required_commands=(curl gpg gpgconf grep jq tar unzip)
for command_name in "${required_commands[@]}"; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Missing required command: $command_name" >&2
    exit 1
  fi
done

for runtime_name in $WINDOWS_RUNTIME_NAMES; do
  if [[ ! "$runtime_name" =~ ^(php|mariadb|node)$ ]]; then
    echo "Unsupported Windows Runtime name: $runtime_name" >&2
    exit 1
  fi
done

should_build() {
  local expected="$1"
  [[ " $WINDOWS_RUNTIME_NAMES " == *" $expected "* ]]
}

sha256_file() {
  local archive="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$archive" | awk '{print $1}'
  else
    shasum -a 256 "$archive" | awk '{print $1}'
  fi
}

file_size() {
  local archive="$1"
  if stat -f '%z' "$archive" >/dev/null 2>&1; then
    stat -f '%z' "$archive"
  else
    stat -c '%s' "$archive"
  fi
}

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
  local actual
  actual="$(sha256_file "$archive")"
  if [[ "$actual" != "$expected" ]]; then
    echo "SHA-256 mismatch for $archive: expected $expected, got $actual" >&2
    exit 1
  fi
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
  size="$(file_size "$archive")"
  sha256="$(sha256_file "$archive")"
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

if should_build "php"; then
  php_archive="$DOWNLOAD_DIR/php-$PHP_VERSION-nts-Win32-vs17-x64.zip"
  download \
    "https://windows.php.net/downloads/releases/archives/$(basename "$php_archive")" \
    "$php_archive"
  verify_sha256 "$php_archive" "$PHP_SHA256"
  rm -rf "$EXPANDED_DIR/php-$PHP_VERSION" "$RUNTIME_DIR/php/$PHP_VERSION"
  mkdir -p "$EXPANDED_DIR/php-$PHP_VERSION" "$RUNTIME_DIR/php/$PHP_VERSION"
  unzip -q "$php_archive" -d "$EXPANDED_DIR/php-$PHP_VERSION"
  cp -R "$EXPANDED_DIR/php-$PHP_VERSION/." "$RUNTIME_DIR/php/$PHP_VERSION/"
  [[ -f "$RUNTIME_DIR/php/$PHP_VERSION/php.exe" ]]
  [[ -f "$RUNTIME_DIR/php/$PHP_VERSION/php-cgi.exe" ]]
  package_runtime "php" "$PHP_VERSION" "official-archive-sha256"
fi

if should_build "mariadb"; then
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
  unzip -q "$mariadb_archive" -d "$EXPANDED_DIR/mariadb-$MARIADB_VERSION"
  mariadb_source="$EXPANDED_DIR/mariadb-$MARIADB_VERSION/mariadb-$MARIADB_VERSION-winx64"
  cp -R "$mariadb_source/." "$RUNTIME_DIR/mariadb/$MARIADB_VERSION/"
  [[ -f "$RUNTIME_DIR/mariadb/$MARIADB_VERSION/bin/mariadbd.exe" ]]
  [[ -f "$RUNTIME_DIR/mariadb/$MARIADB_VERSION/bin/mariadb.exe" ]]
  [[ -f "$RUNTIME_DIR/mariadb/$MARIADB_VERSION/bin/mariadb-install-db.exe" ]]
  package_runtime \
    "mariadb" \
    "$MARIADB_VERSION" \
    "pgp:$MARIADB_RELEASE_FINGERPRINT"
fi

build_node_runtime() {
  local node_version="$1"
  local node_sha256="$2"
  local node_release_fingerprint="$3"
  local node_archive="$DOWNLOAD_DIR/node-v$node_version-win-x64.zip"
  local node_checksums_signature="$DOWNLOAD_DIR/SHASUMS256-$node_version.txt.asc"
  local node_checksums="$DOWNLOAD_DIR/SHASUMS256-$node_version.txt"
  local node_key="$DOWNLOAD_DIR/node-release-key-$node_release_fingerprint.asc"
  local node_status
  local node_source
  download "https://nodejs.org/dist/v$node_version/$(basename "$node_archive")" "$node_archive"
  download "https://nodejs.org/dist/v$node_version/SHASUMS256.txt.asc" "$node_checksums_signature"
  download \
    "https://raw.githubusercontent.com/nodejs/release-keys/main/keys/$node_release_fingerprint.asc" \
    "$node_key"
  verify_sha256 "$node_archive" "$node_sha256"
  rm -rf "$GNUPG_HOME"
  mkdir -m 0700 "$GNUPG_HOME"
  gpg --batch --homedir "$GNUPG_HOME" --import "$node_key"
  node_status="$(gpg --batch --yes --homedir "$GNUPG_HOME" --status-fd 1 \
    --output "$node_checksums" --decrypt "$node_checksums_signature" 2>&1)"
  echo "$node_status"
  if ! grep -q "VALIDSIG $node_release_fingerprint" <<< "$node_status"; then
    echo "Node.js release signature did not match the pinned fingerprint" >&2
    exit 1
  fi
  if ! grep -q "^$node_sha256  $(basename "$node_archive")$" "$node_checksums"; then
    echo "Node.js signed checksums do not contain the pinned archive hash" >&2
    exit 1
  fi
  rm -rf "$EXPANDED_DIR/node-$node_version" "$RUNTIME_DIR/node/$node_version"
  mkdir -p "$EXPANDED_DIR/node-$node_version" "$RUNTIME_DIR/node/$node_version"
  unzip -q "$node_archive" -d "$EXPANDED_DIR/node-$node_version"
  node_source="$EXPANDED_DIR/node-$node_version/node-v$node_version-win-x64"
  cp -R "$node_source/." "$RUNTIME_DIR/node/$node_version/"
  [[ -f "$RUNTIME_DIR/node/$node_version/node.exe" ]]
  [[ -f "$RUNTIME_DIR/node/$node_version/npm.cmd" ]]
  package_runtime "node" "$node_version" "pgp:$node_release_fingerprint"
}

if should_build "node"; then
  build_node_runtime "$NODE20_VERSION" "$NODE20_SHA256" "$NODE20_RELEASE_FINGERPRINT"
  build_node_runtime "$NODE24_VERSION" "$NODE24_SHA256" "$NODE24_RELEASE_FINGERPRINT"
fi

echo "Windows x64 Runtime Packages are ready in $ARTIFACT_DIR"
