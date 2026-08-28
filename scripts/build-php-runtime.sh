#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PHP_VERSION="${PHP_VERSION:-8.2.33}"
PHP_SHA256="${PHP_SHA256:-}"
PHP_RELEASE_FINGERPRINT="${PHP_RELEASE_FINGERPRINT:-}"
PHP_CFLAGS="${PHP_CFLAGS:-}"
PHP_CXXFLAGS="${PHP_CXXFLAGS:-}"
PHP_PATCHES=()
PHP_MAKE_ARGS=()
PHP_REGENERATE_CONFIGURE=0
IMAGICK_VERSION="3.8.1"
IMAGICK_SHA256="3a3587c0a524c17d0dad9673a160b90cd776e836838474e173b549ed864352ee"
IMAP_VERSION="1.0.3"
IMAP_SHA256="0c2c0b1f94f299004be996b85a424e3d11ff65ac0a3c980db3213289a4a3faaf"
CCLIENT_COMMIT="cab109466534e206a3652ef1c68fe88101b68bda"
CCLIENT_SHA256="fbb800741d13f63582fb5b0242ab8e69d0be3d174b85f310d1299df830ed263b"

case "$PHP_VERSION" in
  7.4.33)
    PHP_SHA256="${PHP_SHA256:-924846abf93bc613815c55dd3f5809377813ac62a9ec4eb3778675b82a27b927}"
    PHP_RELEASE_FINGERPRINT="${PHP_RELEASE_FINGERPRINT:-5A52880781F755608BF815FC910DEB46F53EA312}"
    PHP_CFLAGS="${PHP_CFLAGS:--std=gnu11}"
    PHP_CXXFLAGS="${PHP_CXXFLAGS:--std=c++17}"
    PHP_PATCHES+=("$PROJECT_DIR/resources/patches/php-7.4.33-modern-clang.patch")
    PHP_MAKE_ARGS+=("PHP_PHARCMD_SETTINGS=-n -d 'open_basedir=' -d 'output_buffering=0' -d 'memory_limit=-1' -d phar.readonly=0 -d pcre.jit=0")
    PHP_REGENERATE_CONFIGURE=1
    ;;
  8.2.33)
    PHP_SHA256="${PHP_SHA256:-fbdeace9b38220436a4c8fd79b900df92878151db145e641750743a283b514c1}"
    PHP_RELEASE_FINGERPRINT="${PHP_RELEASE_FINGERPRINT:-E60913E4DF209907D8E30D96659A97C9CF2A795A}"
    ;;
  8.4.24)
    PHP_SHA256="${PHP_SHA256:-e127be09a8506f4327c5cfa78a614b00d210714484ec215ce0011b4a03c00731}"
    PHP_RELEASE_FINGERPRINT="${PHP_RELEASE_FINGERPRINT:-9D7F99A0CB8F05C8A6958D6256A97AF7600A39A6}"
    ;;
  *)
    if [[ -z "$PHP_SHA256" || -z "$PHP_RELEASE_FINGERPRINT" ]]; then
      echo "Unsupported PHP version: $PHP_VERSION" >&2
      echo "Set both PHP_SHA256 and PHP_RELEASE_FINGERPRINT to build an unlisted version." >&2
      exit 1
    fi
    ;;
esac
BUILD_ROOT="${FABDEV_BUILD_ROOT:-$PROJECT_DIR/.build/php-$PHP_VERSION}"
DOWNLOAD_DIR="$BUILD_ROOT/downloads"
SOURCE_DIR="$BUILD_ROOT/source/php-$PHP_VERSION"
RUNTIME_ROOT="${FABDEV_RUNTIME_PREFIX:-$BUILD_ROOT/runtime/php/$PHP_VERSION}"
ARTIFACT_DIR="${FABDEV_ARTIFACT_DIR:-$PROJECT_DIR/artifacts}"
MACOS_TARGET="${MACOSX_DEPLOYMENT_TARGET:-$(sw_vers -productVersion | cut -d. -f1).0}"
ARCHIVE_NAME="php-$PHP_VERSION-macos-arm64-dev.tar.gz"
SOURCE_ARCHIVE="$DOWNLOAD_DIR/php-$PHP_VERSION.tar.xz"
SOURCE_SIGNATURE="$SOURCE_ARCHIVE.asc"
KEYRING="$DOWNLOAD_DIR/php-keyring.gpg"
GNUPG_HOME="$DOWNLOAD_DIR/gnupg"
IMAGICK_ARCHIVE="$DOWNLOAD_DIR/imagick-$IMAGICK_VERSION.tgz"
IMAP_ARCHIVE="$DOWNLOAD_DIR/imap-$IMAP_VERSION.tgz"
CCLIENT_ARCHIVE="$DOWNLOAD_DIR/uw-imap-$CCLIENT_COMMIT.tar.gz"
CCLIENT_SOURCE_DIR="$BUILD_ROOT/source/uw-imap-$CCLIENT_COMMIT"
CCLIENT_PREFIX="$BUILD_ROOT/dependencies/c-client"
EXTENSION_BUILD_ROOT="$BUILD_ROOT/extensions"

download_verified_archive() {
  local url="$1"
  local destination="$2"
  local sha256="$3"

  if [[ ! -f "$destination" ]] || ! echo "$sha256  $destination" | shasum -a 256 --check --status; then
    curl --fail --location --retry 3 "$url" --output "$destination"
  fi
  echo "$sha256  $destination" | shasum -a 256 --check
}

cleanup_gnupg() {
  gpgconf --homedir "$GNUPG_HOME" --kill all >/dev/null 2>&1 || true
}
trap cleanup_gnupg EXIT

required_commands=(brew curl gpg gpgconf make patch pkg-config shasum tar xcrun)
required_formulae=(autoconf bison curl freetype gettext icu4c@78 imagemagick jpeg-turbo libiconv libpng libxml2 libsodium libxslt libzip oniguruma openssl@3 pkgconf re2c readline sqlite tidy-html5 xz)

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

download_verified_archive \
  "https://pecl.php.net/get/imagick-$IMAGICK_VERSION.tgz" \
  "$IMAGICK_ARCHIVE" \
  "$IMAGICK_SHA256"
download_verified_archive \
  "https://pecl.php.net/get/imap-$IMAP_VERSION.tgz" \
  "$IMAP_ARCHIVE" \
  "$IMAP_SHA256"
download_verified_archive \
  "https://github.com/uw-imap/imap/archive/$CCLIENT_COMMIT.tar.gz" \
  "$CCLIENT_ARCHIVE" \
  "$CCLIENT_SHA256"

if [[ ! -f "$SOURCE_ARCHIVE" ]] || ! echo "$PHP_SHA256  $SOURCE_ARCHIVE" | shasum -a 256 --check --status; then
  curl --fail --location --retry 3 \
    "https://www.php.net/distributions/php-$PHP_VERSION.tar.xz" \
    --output "$SOURCE_ARCHIVE"
fi
if [[ ! -f "$SOURCE_SIGNATURE" ]]; then
  curl --fail --location --retry 3 \
    "https://www.php.net/distributions/php-$PHP_VERSION.tar.xz.asc" \
    --output "$SOURCE_SIGNATURE"
fi
if [[ ! -f "$KEYRING" ]]; then
  curl --fail --location --retry 3 \
    "https://www.php.net/distributions/php-keyring.gpg" \
    --output "$KEYRING"
fi

echo "$PHP_SHA256  $SOURCE_ARCHIVE" | shasum -a 256 --check
rm -rf "$GNUPG_HOME"
mkdir -m 0700 "$GNUPG_HOME"
gpg --batch --homedir "$GNUPG_HOME" --import "$KEYRING"
gpg_status="$(gpg --batch --homedir "$GNUPG_HOME" --status-fd 1 --verify "$SOURCE_SIGNATURE" "$SOURCE_ARCHIVE" 2>&1)"
echo "$gpg_status"
if ! grep -q "VALIDSIG $PHP_RELEASE_FINGERPRINT" <<< "$gpg_status"; then
  echo "PHP release signature did not match the pinned release manager fingerprint" >&2
  exit 1
fi

rm -rf "$SOURCE_DIR" "$RUNTIME_ROOT"
mkdir -p "$(dirname "$SOURCE_DIR")" "$RUNTIME_ROOT"
tar -xJf "$SOURCE_ARCHIVE" -C "$(dirname "$SOURCE_DIR")"
for patch_file in "${PHP_PATCHES[@]:-}"; do
  if [[ -z "$patch_file" ]]; then
    continue
  fi
  patch --directory="$SOURCE_DIR" --strip=1 --input="$patch_file"
done
if [[ "$PHP_REGENERATE_CONFIGURE" -eq 1 ]]; then
  (cd "$SOURCE_DIR" && ./buildconf --force)
fi

brew_prefixes=(openssl@3 curl icu4c@78 imagemagick libzip oniguruma libiconv libxml2 libxslt sqlite libsodium freetype jpeg-turbo libpng gettext readline tidy-html5)
pkg_config_paths=()
include_paths=()
library_paths=()
for formula in "${brew_prefixes[@]}"; do
  formula_prefix="$(brew --prefix "$formula")"
  pkg_config_paths+=("$formula_prefix/lib/pkgconfig")
  include_paths+=("-I$formula_prefix/include")
  library_paths+=("-L$formula_prefix/lib")
done

export PATH="$(brew --prefix bison)/bin:$(brew --prefix re2c)/bin:$(brew --prefix curl)/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin"
export PKG_CONFIG_PATH="$(IFS=:; echo "${pkg_config_paths[*]}")"
export CPPFLAGS="${include_paths[*]}"
export LDFLAGS="${library_paths[*]}"
export MACOSX_DEPLOYMENT_TARGET="$MACOS_TARGET"
if [[ -n "$PHP_CFLAGS" ]]; then
  export CFLAGS="$PHP_CFLAGS"
fi
if [[ -n "$PHP_CXXFLAGS" ]]; then
  export CXXFLAGS="$PHP_CXXFLAGS"
fi

cd "$SOURCE_DIR"
./configure \
  --prefix="$RUNTIME_ROOT" \
  --with-config-file-path="$RUNTIME_ROOT/etc" \
  --with-config-file-scan-dir="$RUNTIME_ROOT/etc/conf.d" \
  --enable-bcmath \
  --enable-calendar \
  --disable-cgi \
  --disable-phpdbg \
  --enable-exif \
  --enable-fpm \
  --enable-gd \
  --enable-intl \
  --enable-mbstring \
  --enable-mysqlnd \
  --enable-opcache \
  --enable-pcntl \
  --enable-soap \
  --enable-sockets \
  --with-curl \
  --with-freetype \
  --with-gettext="$(brew --prefix gettext)" \
  --with-iconv="$(brew --prefix libiconv)" \
  --with-jpeg \
  --with-mysqli=mysqlnd \
  --with-openssl \
  --with-pdo-mysql=mysqlnd \
  --with-pdo-sqlite \
  --with-readline="$(brew --prefix readline)" \
  --with-sodium \
  --with-tidy="$(brew --prefix tidy-html5)" \
  --with-xsl \
  --with-zip \
  --with-zlib

if [[ -n "${PHP_MAKE_ARGS[*]:-}" ]]; then
  make -j "$(sysctl -n hw.logicalcpu)" "${PHP_MAKE_ARGS[@]}"
else
  make -j "$(sysctl -n hw.logicalcpu)"
fi
make install

rm -rf "$CCLIENT_SOURCE_DIR" "$CCLIENT_PREFIX"
mkdir -p "$CCLIENT_SOURCE_DIR" "$CCLIENT_PREFIX/include/c-client" "$CCLIENT_PREFIX/lib"
tar -xzf "$CCLIENT_ARCHIVE" -C "$CCLIENT_SOURCE_DIR" --strip-components=1
(
  cd "$CCLIENT_SOURCE_DIR"
  make an
  make -C c-client osx \
    CC=clang \
    EXTRACFLAGS="-fPIC -Wno-deprecated-declarations -Wno-error=incompatible-function-pointer-types -DMAC_OSX_KLUDGE=1 -include poll.h -include time.h -include utime.h" \
    EXTRALDFLAGS="-L$(brew --prefix openssl@3)/lib" \
    SSLINCLUDE="$(brew --prefix openssl@3)/include" \
    SSLLIB="$(brew --prefix openssl@3)/lib" \
    SSLCERTS="/etc/ssl/certs" \
    SSLKEYS="/etc/ssl/private" \
    SSLTYPE=nopwd \
    PASSWDTYPE=std
)
cp -L "$CCLIENT_SOURCE_DIR/c-client/"*.h "$CCLIENT_PREFIX/include/c-client/"
cp "$CCLIENT_SOURCE_DIR/c-client/c-client.a" "$CCLIENT_PREFIX/lib/libc-client.a"

build_shared_extension() {
  local extension_name="$1"
  local extension_source="$2"
  shift 2

  cd "$extension_source"
  "$RUNTIME_ROOT/bin/phpize"
  ./configure --with-php-config="$RUNTIME_ROOT/bin/php-config" "$@"
  make -j "$(sysctl -n hw.logicalcpu)"
  make install
}

rm -rf "$EXTENSION_BUILD_ROOT"
mkdir -p "$EXTENSION_BUILD_ROOT"

imagick_source="$EXTENSION_BUILD_ROOT/imagick"
mkdir -p "$imagick_source"
tar -xzf "$IMAGICK_ARCHIVE" -C "$imagick_source" --strip-components=1
patch --directory="$imagick_source" --strip=1 \
  --input="$PROJECT_DIR/resources/patches/imagick-3.8.1-fabdev-runtime-paths.patch"
build_shared_extension \
  imagick \
  "$imagick_source" \
  --with-imagick="$(brew --prefix imagemagick)"

imap_source="$EXTENSION_BUILD_ROOT/imap"
if [[ "$PHP_VERSION" == 8.4.* ]]; then
  mkdir -p "$imap_source"
  tar -xzf "$IMAP_ARCHIVE" -C "$imap_source" --strip-components=1
else
  cp -R "$SOURCE_DIR/ext/imap" "$imap_source"
fi
build_shared_extension \
  imap \
  "$imap_source" \
  --with-imap="$CCLIENT_PREFIX" \
  --with-imap-ssl \
  --with-kerberos=no

imagemagick_prefix="$(brew --prefix imagemagick)"
mkdir -p \
  "$RUNTIME_ROOT/etc/ImageMagick-7" \
  "$RUNTIME_ROOT/lib/ImageMagick/modules-Q16HDRI/coders" \
  "$RUNTIME_ROOT/lib/ImageMagick/modules-Q16HDRI/filters" \
  "$RUNTIME_ROOT/share/ImageMagick-7"
cp "$imagemagick_prefix/etc/ImageMagick-7/"*.xml "$RUNTIME_ROOT/etc/ImageMagick-7/"
cp "$imagemagick_prefix/lib/ImageMagick/modules-Q16HDRI/coders/"*.so \
  "$RUNTIME_ROOT/lib/ImageMagick/modules-Q16HDRI/coders/"
cp "$imagemagick_prefix/lib/ImageMagick/modules-Q16HDRI/coders/"*.la \
  "$RUNTIME_ROOT/lib/ImageMagick/modules-Q16HDRI/coders/"
cp "$imagemagick_prefix/lib/ImageMagick/modules-Q16HDRI/filters/"*.so \
  "$RUNTIME_ROOT/lib/ImageMagick/modules-Q16HDRI/filters/"
cp "$imagemagick_prefix/lib/ImageMagick/modules-Q16HDRI/filters/"*.la \
  "$RUNTIME_ROOT/lib/ImageMagick/modules-Q16HDRI/filters/"
cp "$imagemagick_prefix/share/ImageMagick-7/"*.xml "$RUNTIME_ROOT/share/ImageMagick-7/"
for module_metadata in \
  "$RUNTIME_ROOT/lib/ImageMagick/modules-Q16HDRI/coders/"*.la \
  "$RUNTIME_ROOT/lib/ImageMagick/modules-Q16HDRI/filters/"*.la; do
  sed -i '' \
    -e "s|^dependency_libs=.*|dependency_libs=''|" \
    -e "s|^libdir=.*|libdir='.'|" \
    "$module_metadata"
done

opcache_path="$(find "$RUNTIME_ROOT/lib/php/extensions" -type f -name opcache.so -print -quit)"
if [[ -z "$opcache_path" ]]; then
  echo "Installed PHP Runtime does not contain opcache.so" >&2
  exit 1
fi
php_extension_api="$(basename "$(dirname "$opcache_path")")"

mkdir -p "$RUNTIME_ROOT/etc/conf.d" "$RUNTIME_ROOT/var/logs" "$RUNTIME_ROOT/var/php-fpm.d" "$RUNTIME_ROOT/var/run" "$RUNTIME_ROOT/var/session"
sed -e "s|@RUNTIME_ROOT@|$RUNTIME_ROOT|g" -e "s|@SERVICE_ROOT@|$RUNTIME_ROOT/var|g" -e "s|@PHP_EXTENSION_API@|$php_extension_api|g" "$PROJECT_DIR/resources/php/php.ini" > "$RUNTIME_ROOT/etc/php.ini"
sed -e "s|@RUNTIME_ROOT@|$RUNTIME_ROOT|g" -e "s|@SERVICE_ROOT@|$RUNTIME_ROOT/var|g" "$PROJECT_DIR/resources/php/php-fpm.conf" > "$RUNTIME_ROOT/etc/php-fpm.conf"
sed -e "s|@RUNTIME_ROOT@|$RUNTIME_ROOT|g" -e "s|@SERVICE_ROOT@|$RUNTIME_ROOT/var|g" "$PROJECT_DIR/resources/php/www.conf" > "$RUNTIME_ROOT/var/php-fpm.d/www.conf"
cp "$PROJECT_DIR/resources/php/php.ini" "$RUNTIME_ROOT/etc/php.ini.template"
cp "$PROJECT_DIR/resources/php/php-fpm.conf" "$RUNTIME_ROOT/etc/php-fpm.conf.template"
cp "$PROJECT_DIR/resources/php/www.conf" "$RUNTIME_ROOT/etc/www.conf.template"

"$SCRIPT_DIR/package-php-runtime.sh" "$RUNTIME_ROOT" "$PHP_VERSION" "$ARTIFACT_DIR"
