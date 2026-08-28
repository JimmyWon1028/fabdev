#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
NGINX_VERSION="${NGINX_VERSION:-1.30.4}"
NGINX_SHA256="${NGINX_SHA256:-4261dc90e9e47c1c4041276e9aaa3d48ebe2e664f728e14fa95ae6c67d57a08b}"
NGINX_RELEASE_FINGERPRINT="${NGINX_RELEASE_FINGERPRINT:-43387825DDB1BB97EC36BA5D007C8D7C15D87369}"
BUILD_ROOT="${FABDEV_BUILD_ROOT:-$PROJECT_DIR/.build/nginx-$NGINX_VERSION}"
DOWNLOAD_DIR="$BUILD_ROOT/downloads"
SOURCE_DIR="$BUILD_ROOT/source/nginx-$NGINX_VERSION"
RUNTIME_ROOT="${FABDEV_RUNTIME_PREFIX:-$BUILD_ROOT/runtime/nginx/$NGINX_VERSION}"
ARTIFACT_DIR="${FABDEV_ARTIFACT_DIR:-$PROJECT_DIR/artifacts}"
MACOS_TARGET="${MACOSX_DEPLOYMENT_TARGET:-$(sw_vers -productVersion | cut -d. -f1).0}"
SOURCE_ARCHIVE="$DOWNLOAD_DIR/nginx-$NGINX_VERSION.tar.gz"
SOURCE_SIGNATURE="$SOURCE_ARCHIVE.asc"
RELEASE_KEY="$DOWNLOAD_DIR/nginx-release.key"
GNUPG_HOME="$DOWNLOAD_DIR/gnupg"

cleanup_gnupg() {
  gpgconf --homedir "$GNUPG_HOME" --kill all >/dev/null 2>&1 || true
}
trap cleanup_gnupg EXIT

required_commands=(brew curl gpg gpgconf make shasum tar xcrun)
required_formulae=(openssl@3 pcre2 zlib)

for command_name in "${required_commands[@]}"; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Missing required command: $command_name" >&2
    exit 1
  fi
done

for formula in "${required_formulae[@]}"; do
  if ! brew --prefix "$formula" >/dev/null 2>&1; then
    echo "Missing Homebrew build dependency: $formula" >&2
    exit 1
  fi
done

mkdir -p "$DOWNLOAD_DIR" "$ARTIFACT_DIR"

if [[ ! -f "$SOURCE_ARCHIVE" ]] || ! echo "$NGINX_SHA256  $SOURCE_ARCHIVE" | shasum -a 256 --check --status; then
  curl --fail --location --retry 3 \
    "https://nginx.org/download/nginx-$NGINX_VERSION.tar.gz" \
    --output "$SOURCE_ARCHIVE"
fi
if [[ ! -f "$SOURCE_SIGNATURE" ]]; then
  curl --fail --location --retry 3 \
    "https://nginx.org/download/nginx-$NGINX_VERSION.tar.gz.asc" \
    --output "$SOURCE_SIGNATURE"
fi
if [[ ! -f "$RELEASE_KEY" ]]; then
  curl --fail --location --retry 3 \
    "https://nginx.org/keys/arut.key" \
    --output "$RELEASE_KEY"
fi

echo "$NGINX_SHA256  $SOURCE_ARCHIVE" | shasum -a 256 --check
rm -rf "$GNUPG_HOME"
mkdir -m 0700 "$GNUPG_HOME"
gpg --batch --homedir "$GNUPG_HOME" --import "$RELEASE_KEY"
gpg_status="$(gpg --batch --homedir "$GNUPG_HOME" --status-fd 1 --verify "$SOURCE_SIGNATURE" "$SOURCE_ARCHIVE" 2>&1)"
echo "$gpg_status"
if ! grep -q "VALIDSIG $NGINX_RELEASE_FINGERPRINT" <<< "$gpg_status"; then
  echo "Nginx release signature did not match the pinned fingerprint" >&2
  exit 1
fi

rm -rf "$SOURCE_DIR" "$RUNTIME_ROOT"
mkdir -p "$(dirname "$SOURCE_DIR")" "$RUNTIME_ROOT"
tar -xzf "$SOURCE_ARCHIVE" -C "$(dirname "$SOURCE_DIR")"

openssl_prefix="$(brew --prefix openssl@3)"
pcre_prefix="$(brew --prefix pcre2)"
zlib_prefix="$(brew --prefix zlib)"
export MACOSX_DEPLOYMENT_TARGET="$MACOS_TARGET"

cd "$SOURCE_DIR"
./configure \
  --prefix="$RUNTIME_ROOT" \
  --sbin-path="$RUNTIME_ROOT/sbin/nginx" \
  --conf-path="$RUNTIME_ROOT/conf/nginx.conf" \
  --pid-path="$RUNTIME_ROOT/var/run/nginx.pid" \
  --lock-path="$RUNTIME_ROOT/var/run/nginx.lock" \
  --error-log-path="$RUNTIME_ROOT/var/log/error.log" \
  --http-log-path="$RUNTIME_ROOT/var/log/access.log" \
  --with-http_ssl_module \
  --with-http_v2_module \
  --with-http_stub_status_module \
  --with-cc-opt="-I$openssl_prefix/include -I$pcre_prefix/include -I$zlib_prefix/include" \
  --with-ld-opt="-L$openssl_prefix/lib -L$pcre_prefix/lib -L$zlib_prefix/lib"

make -j "$(sysctl -n hw.logicalcpu)"
make install

mkdir -p "$RUNTIME_ROOT/lib" "$RUNTIME_ROOT/var/log" "$RUNTIME_ROOT/var/run"
cp "$SOURCE_DIR/LICENSE" "$RUNTIME_ROOT/LICENSE"
cp "$PROJECT_DIR/resources/nginx/nginx.conf" "$RUNTIME_ROOT/conf/fabdev-nginx.conf.template"

"$SCRIPT_DIR/package-nginx-runtime.sh" "$RUNTIME_ROOT" "$NGINX_VERSION" "$ARTIFACT_DIR"
