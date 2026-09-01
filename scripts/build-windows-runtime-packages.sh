#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PACKAGE_MANIFEST="${FABDEV_WINDOWS_PACKAGE_MANIFEST:-$PROJECT_DIR/resources/runtime-packages/windows-x64.json}"
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

if [[ ! -f "$PACKAGE_MANIFEST" || -L "$PACKAGE_MANIFEST" ]]; then
  echo "Windows Runtime package manifest must be a regular file: $PACKAGE_MANIFEST" >&2
  exit 1
fi

jq -e '
  .schemaVersion == 1
  and .platform == "windows"
  and .architecture == "x64"
  and (.minimumOsVersion | type == "string")
  and (.packages | type == "array" and length > 0)
  and ([.packages[] | "\(.name)@\(.version)"] | length == (unique | length))
  and all(.packages[];
    (.name | test("^(php|mariadb|node)$"))
    and (.version | test("^[0-9]+\\.[0-9]+\\.[0-9]+$"))
    and (.source.archiveUrl | startswith("https://"))
    and (.source.archiveSha256 | test("^[0-9a-f]{64}$"))
    and (.healthCheckProfile == (.name + "-runtime-v1"))
    and if .name == "php" then
      .source.verification.method == "official-sha256"
    elif .name == "mariadb" then
      .source.verification.method == "pgp"
      and (.source.verification.fingerprint | test("^[0-9A-F]{40}$"))
      and (.source.signatureUrl | startswith("https://"))
      and (.source.keyUrl | startswith("https://"))
    else
      .source.verification.method == "pgp"
      and (.source.verification.fingerprint | test("^[0-9A-F]{40}$"))
      and (.source.signedChecksumsUrl | startswith("https://"))
      and (.source.keyUrl | startswith("https://"))
    end
  )
' "$PACKAGE_MANIFEST" >/dev/null

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
  local archive="$ARTIFACT_DIR/$name-$version-windows-x64-community.tar.gz"
  COPYFILE_DISABLE=1 tar -czf "$archive" -C "$RUNTIME_DIR/$name" "$version"
  write_release "$name" "$version" "$archive" "$signature"
  echo "Created $archive"
}

build_php_runtime() {
  local version="$1"
  local archive_url="$2"
  local archive_sha256="$3"
  local archive="$DOWNLOAD_DIR/$(basename "$archive_url")"
  download "$archive_url" "$archive"
  verify_sha256 "$archive" "$archive_sha256"
  rm -rf "$EXPANDED_DIR/php-$version" "$RUNTIME_DIR/php/$version"
  mkdir -p "$EXPANDED_DIR/php-$version" "$RUNTIME_DIR/php/$version"
  unzip -q "$archive" -d "$EXPANDED_DIR/php-$version"
  cp -R "$EXPANDED_DIR/php-$version/." "$RUNTIME_DIR/php/$version/"
  [[ -f "$RUNTIME_DIR/php/$version/php.exe" ]]
  [[ -f "$RUNTIME_DIR/php/$version/php-cgi.exe" ]]
  [[ -f "$RUNTIME_DIR/php/$version/ext/php_mysqli.dll" ]]
  [[ -f "$RUNTIME_DIR/php/$version/ext/php_pdo_mysql.dll" ]]
  package_runtime "php" "$version" "official-archive-sha256"
}

build_mariadb_runtime() {
  local version="$1"
  local archive_url="$2"
  local archive_sha256="$3"
  local signature_url="$4"
  local key_url="$5"
  local fingerprint="$6"
  local archive="$DOWNLOAD_DIR/$(basename "$archive_url")"
  local signature="$DOWNLOAD_DIR/$(basename "$signature_url")"
  local key="$DOWNLOAD_DIR/mariadb-release-$fingerprint.key"
  local source="$EXPANDED_DIR/mariadb-$version/mariadb-$version-winx64"

  download "$archive_url" "$archive"
  download "$signature_url" "$signature"
  download "$key_url" "$key"
  verify_sha256 "$archive" "$archive_sha256"
  verify_detached_signature "$archive" "$signature" "$key" "$fingerprint" "MariaDB"
  rm -rf "$EXPANDED_DIR/mariadb-$version" "$RUNTIME_DIR/mariadb/$version"
  mkdir -p "$EXPANDED_DIR/mariadb-$version" "$RUNTIME_DIR/mariadb/$version"
  unzip -q "$archive" -d "$EXPANDED_DIR/mariadb-$version"
  cp -R "$source/." "$RUNTIME_DIR/mariadb/$version/"
  [[ -f "$RUNTIME_DIR/mariadb/$version/bin/mariadbd.exe" ]]
  [[ -f "$RUNTIME_DIR/mariadb/$version/bin/mariadb.exe" ]]
  [[ -f "$RUNTIME_DIR/mariadb/$version/bin/mariadb-install-db.exe" ]]
  package_runtime "mariadb" "$version" "pgp:$fingerprint"
}

build_node_runtime() {
  local version="$1"
  local archive_url="$2"
  local archive_sha256="$3"
  local signed_checksums_url="$4"
  local key_url="$5"
  local fingerprint="$6"
  local archive="$DOWNLOAD_DIR/$(basename "$archive_url")"
  local checksums_signature="$DOWNLOAD_DIR/SHASUMS256-$version.txt.asc"
  local checksums="$DOWNLOAD_DIR/SHASUMS256-$version.txt"
  local key="$DOWNLOAD_DIR/node-release-key-$fingerprint.asc"
  local status
  local source="$EXPANDED_DIR/node-$version/node-v$version-win-x64"

  download "$archive_url" "$archive"
  download "$signed_checksums_url" "$checksums_signature"
  download "$key_url" "$key"
  verify_sha256 "$archive" "$archive_sha256"
  rm -rf "$GNUPG_HOME"
  mkdir -m 0700 "$GNUPG_HOME"
  gpg --batch --homedir "$GNUPG_HOME" --import "$key"
  status="$(gpg --batch --yes --homedir "$GNUPG_HOME" --status-fd 1 \
    --output "$checksums" --decrypt "$checksums_signature" 2>&1)"
  echo "$status"
  if ! grep -q "VALIDSIG $fingerprint" <<< "$status"; then
    echo "Node.js release signature did not match the pinned fingerprint" >&2
    exit 1
  fi
  if ! grep -q "^$archive_sha256  $(basename "$archive")$" "$checksums"; then
    echo "Node.js signed checksums do not contain the pinned archive hash" >&2
    exit 1
  fi
  rm -rf "$EXPANDED_DIR/node-$version" "$RUNTIME_DIR/node/$version"
  mkdir -p "$EXPANDED_DIR/node-$version" "$RUNTIME_DIR/node/$version"
  unzip -q "$archive" -d "$EXPANDED_DIR/node-$version"
  cp -R "$source/." "$RUNTIME_DIR/node/$version/"
  [[ -f "$RUNTIME_DIR/node/$version/node.exe" ]]
  [[ -f "$RUNTIME_DIR/node/$version/npm.cmd" ]]
  package_runtime "node" "$version" "pgp:$fingerprint"
}

mkdir -p "$DOWNLOAD_DIR" "$EXPANDED_DIR" "$RUNTIME_DIR" "$ARTIFACT_DIR"

while IFS= read -r package; do
  name="$(jq -r '.name' <<< "$package")"
  version="$(jq -r '.version' <<< "$package")"
  archive_url="$(jq -r '.source.archiveUrl' <<< "$package")"
  archive_sha256="$(jq -r '.source.archiveSha256' <<< "$package")"
  verification_method="$(jq -r '.source.verification.method' <<< "$package")"
  fingerprint="$(jq -r '.source.verification.fingerprint // empty' <<< "$package")"

  if [[ ! "$name" =~ ^(php|mariadb|node)$ || ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Invalid Windows Runtime package identity: $name $version" >&2
    exit 1
  fi
  if ! should_build "$name"; then
    continue
  fi

  case "$name" in
    php)
      [[ "$verification_method" == "official-sha256" ]]
      build_php_runtime "$version" "$archive_url" "$archive_sha256"
      ;;
    mariadb)
      [[ "$verification_method" == "pgp" && -n "$fingerprint" ]]
      build_mariadb_runtime \
        "$version" \
        "$archive_url" \
        "$archive_sha256" \
        "$(jq -r '.source.signatureUrl' <<< "$package")" \
        "$(jq -r '.source.keyUrl' <<< "$package")" \
        "$fingerprint"
      ;;
    node)
      [[ "$verification_method" == "pgp" && -n "$fingerprint" ]]
      build_node_runtime \
        "$version" \
        "$archive_url" \
        "$archive_sha256" \
        "$(jq -r '.source.signedChecksumsUrl' <<< "$package")" \
        "$(jq -r '.source.keyUrl' <<< "$package")" \
        "$fingerprint"
      ;;
  esac
done < <(jq -c '.packages[]' "$PACKAGE_MANIFEST")

echo "Windows x64 Runtime Packages are ready in $ARTIFACT_DIR"
