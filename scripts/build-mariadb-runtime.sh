#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
MARIADB_VERSION="${MARIADB_VERSION:-12.3.2}"
MARIADB_SHA256="${MARIADB_SHA256:-82798714baf2f3456ed2f311fc803dc120f2bf3b82358e773847d628cdb4b670}"
MARIADB_RELEASE_FINGERPRINT="${MARIADB_RELEASE_FINGERPRINT:-177F4010FE56CA3336300305F1656F24C74CD1D8}"
BUILD_ROOT="${FABDEV_BUILD_ROOT:-$PROJECT_DIR/.build/mariadb-$MARIADB_VERSION}"
DOWNLOAD_DIR="$BUILD_ROOT/downloads"
SOURCE_DIR="$BUILD_ROOT/source/mariadb-$MARIADB_VERSION"
RUNTIME_ROOT="${FABDEV_RUNTIME_PREFIX:-$BUILD_ROOT/runtime/mariadb/$MARIADB_VERSION}"
ARTIFACT_DIR="${FABDEV_ARTIFACT_DIR:-$PROJECT_DIR/artifacts}"
MACOS_TARGET="${MACOSX_DEPLOYMENT_TARGET:-$(sw_vers -productVersion | cut -d. -f1).0}"
MACOS_SDK="${SDKROOT:-$(xcrun --sdk macosx --show-sdk-path)}"
SOURCE_ARCHIVE="$DOWNLOAD_DIR/mariadb-$MARIADB_VERSION.tar.gz"
SOURCE_SIGNATURE="$SOURCE_ARCHIVE.asc"
RELEASE_KEY="$DOWNLOAD_DIR/mariadb-release.key"
GNUPG_HOME="$DOWNLOAD_DIR/gnupg"

cleanup_gnupg() {
  gpgconf --homedir "$GNUPG_HOME" --kill all >/dev/null 2>&1 || true
}
trap cleanup_gnupg EXIT

required_commands=(brew cmake curl gpg gpgconf make shasum tar xcrun)
required_formulae=(bison groonga lz4 lzo openssl@3 pcre2 pkgconf xz zstd)

for command_name in "${required_commands[@]}"; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Missing required command: $command_name" >&2
    exit 1
  fi
done

for formula in "${required_formulae[@]}"; do
  formula_prefix="$(brew --prefix "$formula" 2>/dev/null || true)"
  if [[ -z "$formula_prefix" || ! -d "$formula_prefix" ]]; then
    echo "Missing Homebrew build dependency: $formula" >&2
    exit 1
  fi
done

mkdir -p "$DOWNLOAD_DIR" "$ARTIFACT_DIR"

if [[ ! -f "$SOURCE_ARCHIVE" ]] || ! echo "$MARIADB_SHA256  $SOURCE_ARCHIVE" | shasum -a 256 --check --status; then
  curl --fail --location --retry 3 \
    "https://archive.mariadb.org/mariadb-$MARIADB_VERSION/source/mariadb-$MARIADB_VERSION.tar.gz" \
    --output "$SOURCE_ARCHIVE"
fi
if [[ ! -f "$SOURCE_SIGNATURE" ]]; then
  curl --fail --location --retry 3 \
    "https://archive.mariadb.org/mariadb-$MARIADB_VERSION/source/mariadb-$MARIADB_VERSION.tar.gz.asc" \
    --output "$SOURCE_SIGNATURE"
fi
if [[ ! -f "$RELEASE_KEY" ]]; then
  curl --fail --location --retry 3 \
    "https://archive.mariadb.org/PublicKey" \
    --output "$RELEASE_KEY"
fi

echo "$MARIADB_SHA256  $SOURCE_ARCHIVE" | shasum -a 256 --check
rm -rf "$GNUPG_HOME"
mkdir -m 0700 "$GNUPG_HOME"
gpg --batch --homedir "$GNUPG_HOME" --import "$RELEASE_KEY"
gpg_status="$(gpg --batch --homedir "$GNUPG_HOME" --status-fd 1 --verify "$SOURCE_SIGNATURE" "$SOURCE_ARCHIVE" 2>&1)"
echo "$gpg_status"
if ! grep -q "VALIDSIG $MARIADB_RELEASE_FINGERPRINT" <<< "$gpg_status"; then
  echo "MariaDB release signature did not match the pinned fingerprint" >&2
  exit 1
fi

rm -rf "$SOURCE_DIR" "$RUNTIME_ROOT"
mkdir -p "$(dirname "$SOURCE_DIR")" "$RUNTIME_ROOT"
tar -xzf "$SOURCE_ARCHIVE" -C "$(dirname "$SOURCE_DIR")"

rm -rf "$SOURCE_DIR/storage/mroonga/vendor/groonga"
rm -rf "$SOURCE_DIR/extra/wolfssl"
rm -rf "$SOURCE_DIR/zlib"

export MACOSX_DEPLOYMENT_TARGET="$MACOS_TARGET"
export PATH="$(brew --prefix bison)/bin:$PATH"
export PKG_CONFIG_PATH="$(brew --prefix openssl@3)/lib/pkgconfig:$(brew --prefix pcre2)/lib/pkgconfig"
CMAKE_DEPENDENCY_PREFIXES="$(brew --prefix groonga);$(brew --prefix lz4);$(brew --prefix lzo);$(brew --prefix openssl@3);$(brew --prefix pcre2);$(brew --prefix xz);$(brew --prefix zstd)"

cmake -S "$SOURCE_DIR" -B "$SOURCE_DIR/_build" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_C_FLAGS=-w \
  -DCMAKE_CXX_FLAGS=-w \
  -DCMAKE_INSTALL_PREFIX="$RUNTIME_ROOT" \
  -DCMAKE_PREFIX_PATH="$CMAKE_DEPENDENCY_PREFIXES" \
  -DCMAKE_OSX_SYSROOT="$MACOS_SDK" \
  -DCMAKE_OSX_DEPLOYMENT_TARGET="$MACOS_TARGET" \
  -DZLIB_INCLUDE_DIR="$MACOS_SDK/usr/include" \
  -DZLIB_LIBRARY="$MACOS_SDK/usr/lib/libz.tbd" \
  -DMYSQL_DATADIR="$RUNTIME_ROOT/data" \
  -DINSTALL_INCLUDEDIR=include/mysql \
  -DINSTALL_MANDIR=share/man \
  -DINSTALL_DOCDIR=share/doc/mariadb \
  -DINSTALL_MYSQLSHAREDIR=share/mysql \
  -DINSTALL_SYSCONFDIR="$RUNTIME_ROOT/etc" \
  -DWITH_LIBFMT=bundled \
  -DWITH_PCRE=system \
  -DWITH_SSL=system \
  -DWITH_ZLIB=system \
  -DWITH_UNIT_TESTS=OFF \
  -DPLUGIN_ROCKSDB=NO \
  -DCONNECT_WITH_JDBC=OFF \
  -DDEFAULT_CHARSET=utf8mb4 \
  -DDEFAULT_COLLATION=utf8mb4_general_ci \
  -DCOMPILATION_COMMENT=fabDev

cmake --build "$SOURCE_DIR/_build" --parallel "$(sysctl -n hw.logicalcpu)"
cmake --install "$SOURCE_DIR/_build"

rm -rf "$RUNTIME_ROOT/mariadb-test" "$RUNTIME_ROOT/sql-bench" "$RUNTIME_ROOT/data"
mkdir -p "$RUNTIME_ROOT/lib"
cp "$SOURCE_DIR/COPYING" "$RUNTIME_ROOT/COPYING"

"$SCRIPT_DIR/package-mariadb-runtime.sh" "$RUNTIME_ROOT" "$MARIADB_VERSION" "$ARTIFACT_DIR"
