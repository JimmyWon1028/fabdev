#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "Usage: $0 <profile> <install-prefix>" >&2
  exit 1
fi

PROFILE="$1"
INSTALL_PREFIX="$2"
MACOS_TARGET="${MACOSX_DEPLOYMENT_TARGET:-13.0}"
OPENSSL_VERSION="3.6.3"
OPENSSL_SHA256="243a86649cf6f23eeb6a2ff2456e09e5d77dd9018a54d3d96b0c6bdd6ba6c7f1"
PCRE2_VERSION="10.47"
PCRE2_SHA256="47fe8c99461250d42f89e6e8fdaeba9da057855d06eb7fc08d9ca03fd08d7bc7"
CURL_VERSION="8.21.0"
CURL_SHA256="ad6f2f94934b38e31e48272833c99b891d045b4565fe942a53fbd27bd3910e16"
ICU_VERSION="78.3"
ICU_SHA256="3a2e7a47604ba702f345878308e6fefeca612ee895cf4a5f222e7955fabfe0c0"
IMAGEMAGICK_VERSION="7.1.2-30"
IMAGEMAGICK_SHA256="3ef82a66a4b28af069ac4f826ed958c17ad9baac6393368db7ec6d5920f6be7d"
LIBZIP_VERSION="1.11.4"
LIBZIP_SHA256="8a247f57d1e3e6f6d11413b12a6f28a9d388de110adc0ec608d893180ed7097b"
ONIGURUMA_VERSION="6.9.10"
ONIGURUMA_SHA256="2a5cfc5ae259e4e97f86b68dfffc152cdaffe94e2060b770cb827238d769fc05"
LIBXML2_VERSION="2.15.3"
LIBXML2_SHA256="78262a6e7ac170d6528ebfe2efccdf220191a5af6a6cd61ea4a9a9a5042c7a07"
LIBXSLT_VERSION="1.1.45"
LIBXSLT_SHA256="9acfe68419c4d06a45c550321b3212762d92f41465062ca4ea19e632ee5d216e"
LIBSODIUM_VERSION="1.0.22"
LIBSODIUM_SHA256="adbdd8f16149e81ac6078a03aca6fc03b592b89ef7b5ed83841c086191be3349"
LIBICONV_VERSION="1.19"
LIBICONV_SHA256="88dd96a8c0464eca144fc791ae60cd31cd8ee78321e67397e25fc095c4a19aa6"
FREETYPE_VERSION="2.14.3"
FREETYPE_SHA256="36bc4f1cc413335368ee656c42afca65c5a3987e8768cc28cf11ba775e785a5f"
JPEG_TURBO_VERSION="3.2.0"
JPEG_TURBO_SHA256="6f30092cef9fb839779646608f4ee14ae3cbac989c47fa05e841b0841f09878e"
LIBPNG_VERSION="1.6.58"
LIBPNG_SHA256="28eb403f51f0f7405249132cecfe82ea5c0ef97f1b32c5a65828814ae0d34775"
GETTEXT_VERSION="1.0"
GETTEXT_SHA256="85d99b79c981a404874c02e0342176cf75c7698e2b51fe41031cf6526d974f1a"
TIDY_VERSION="5.8.0"
TIDY_SHA256="59c86d5b2e452f63c5cdb29c866a12a4c55b1741d7025cf2f3ce0cde99b0660e"

case "$PROFILE" in
  mariadb|php) ;;
  *)
    echo "Unsupported macOS Runtime dependency profile: $PROFILE" >&2
    exit 1
    ;;
esac

if [[ "$INSTALL_PREFIX" != /* || "$INSTALL_PREFIX" == "/" ]]; then
  echo "Install prefix must be a non-root absolute path: $INSTALL_PREFIX" >&2
  exit 1
fi

required_commands=(cmake curl make perl pkg-config shasum tar xcrun)
if [[ "$PROFILE" == "php" ]]; then
  required_commands+=(7zz)
fi
for command_name in "${required_commands[@]}"; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Missing required command: $command_name" >&2
    exit 1
  fi
done

BUILD_JOBS="${FABDEV_BUILD_JOBS:-}"
if [[ -z "$BUILD_JOBS" ]]; then
  BUILD_JOBS="$(sysctl -n hw.logicalcpu 2>/dev/null || true)"
fi
if [[ ! "$BUILD_JOBS" =~ ^[1-9][0-9]*$ ]]; then
  BUILD_JOBS="$(getconf _NPROCESSORS_ONLN 2>/dev/null || true)"
fi
if [[ ! "$BUILD_JOBS" =~ ^[1-9][0-9]*$ ]]; then
  BUILD_JOBS=4
fi

BUILD_ROOT="${FABDEV_DEPENDENCY_BUILD_ROOT:-$(dirname "$INSTALL_PREFIX")/.dependency-build-$PROFILE}"
DOWNLOAD_DIR="$BUILD_ROOT/downloads"
SOURCE_ROOT="$BUILD_ROOT/source"
MACOS_SDK="${SDKROOT:-$(xcrun --sdk macosx --show-sdk-path)}"

download_verified_archive() {
  local url="$1"
  local destination="$2"
  local sha256="$3"

  if [[ ! -f "$destination" ]] || ! echo "$sha256  $destination" | shasum -a 256 --check --status; then
    curl --fail --location --retry 3 "$url" --output "$destination"
  fi
  echo "$sha256  $destination" | shasum -a 256 --check
}

configure_make_install() {
  local source_dir="$1"
  shift

  (
    cd "$source_dir"
    ./configure --prefix="$INSTALL_PREFIX" "$@"
    make -j "$BUILD_JOBS"
    make install
  )
}

mkdir -p "$DOWNLOAD_DIR"
OPENSSL_ARCHIVE="$DOWNLOAD_DIR/openssl-$OPENSSL_VERSION.tar.gz"
download_verified_archive "https://github.com/openssl/openssl/releases/download/openssl-$OPENSSL_VERSION/openssl-$OPENSSL_VERSION.tar.gz" "$OPENSSL_ARCHIVE" "$OPENSSL_SHA256"

if [[ "$PROFILE" == "mariadb" ]]; then
  PCRE2_ARCHIVE="$DOWNLOAD_DIR/pcre2-$PCRE2_VERSION.tar.bz2"
  download_verified_archive "https://github.com/PCRE2Project/pcre2/releases/download/pcre2-$PCRE2_VERSION/pcre2-$PCRE2_VERSION.tar.bz2" "$PCRE2_ARCHIVE" "$PCRE2_SHA256"
else
  CURL_ARCHIVE="$DOWNLOAD_DIR/curl-$CURL_VERSION.tar.bz2"
  ICU_ARCHIVE="$DOWNLOAD_DIR/icu4c-$ICU_VERSION-sources.tgz"
  IMAGEMAGICK_ARCHIVE="$DOWNLOAD_DIR/ImageMagick-$IMAGEMAGICK_VERSION.7z"
  LIBZIP_ARCHIVE="$DOWNLOAD_DIR/libzip-$LIBZIP_VERSION.tar.xz"
  ONIGURUMA_ARCHIVE="$DOWNLOAD_DIR/onig-$ONIGURUMA_VERSION.tar.gz"
  LIBXML2_ARCHIVE="$DOWNLOAD_DIR/libxml2-$LIBXML2_VERSION.tar.xz"
  LIBXSLT_ARCHIVE="$DOWNLOAD_DIR/libxslt-$LIBXSLT_VERSION.tar.xz"
  LIBSODIUM_ARCHIVE="$DOWNLOAD_DIR/libsodium-$LIBSODIUM_VERSION.tar.gz"
  LIBICONV_ARCHIVE="$DOWNLOAD_DIR/libiconv-$LIBICONV_VERSION.tar.gz"
  FREETYPE_ARCHIVE="$DOWNLOAD_DIR/freetype-$FREETYPE_VERSION.tar.xz"
  JPEG_TURBO_ARCHIVE="$DOWNLOAD_DIR/libjpeg-turbo-$JPEG_TURBO_VERSION.tar.gz"
  LIBPNG_ARCHIVE="$DOWNLOAD_DIR/libpng-$LIBPNG_VERSION.tar.xz"
  GETTEXT_ARCHIVE="$DOWNLOAD_DIR/gettext-$GETTEXT_VERSION.tar.gz"
  TIDY_ARCHIVE="$DOWNLOAD_DIR/tidy-html5-$TIDY_VERSION.tar.gz"

  download_verified_archive "https://curl.se/download/curl-$CURL_VERSION.tar.bz2" "$CURL_ARCHIVE" "$CURL_SHA256"
  download_verified_archive "https://github.com/unicode-org/icu/releases/download/release-$ICU_VERSION/icu4c-$ICU_VERSION-sources.tgz" "$ICU_ARCHIVE" "$ICU_SHA256"
  download_verified_archive "https://github.com/ImageMagick/ImageMagick/releases/download/$IMAGEMAGICK_VERSION/ImageMagick-$IMAGEMAGICK_VERSION.7z" "$IMAGEMAGICK_ARCHIVE" "$IMAGEMAGICK_SHA256"
  download_verified_archive "https://libzip.org/download/libzip-$LIBZIP_VERSION.tar.xz" "$LIBZIP_ARCHIVE" "$LIBZIP_SHA256"
  download_verified_archive "https://github.com/kkos/oniguruma/releases/download/v$ONIGURUMA_VERSION/onig-$ONIGURUMA_VERSION.tar.gz" "$ONIGURUMA_ARCHIVE" "$ONIGURUMA_SHA256"
  download_verified_archive "https://download.gnome.org/sources/libxml2/2.15/libxml2-$LIBXML2_VERSION.tar.xz" "$LIBXML2_ARCHIVE" "$LIBXML2_SHA256"
  download_verified_archive "https://download.gnome.org/sources/libxslt/1.1/libxslt-$LIBXSLT_VERSION.tar.xz" "$LIBXSLT_ARCHIVE" "$LIBXSLT_SHA256"
  download_verified_archive "https://download.libsodium.org/libsodium/releases/libsodium-$LIBSODIUM_VERSION.tar.gz" "$LIBSODIUM_ARCHIVE" "$LIBSODIUM_SHA256"
  download_verified_archive "https://ftp.gnu.org/gnu/libiconv/libiconv-$LIBICONV_VERSION.tar.gz" "$LIBICONV_ARCHIVE" "$LIBICONV_SHA256"
  download_verified_archive "https://downloads.sourceforge.net/project/freetype/freetype2/$FREETYPE_VERSION/freetype-$FREETYPE_VERSION.tar.xz" "$FREETYPE_ARCHIVE" "$FREETYPE_SHA256"
  download_verified_archive "https://github.com/libjpeg-turbo/libjpeg-turbo/releases/download/$JPEG_TURBO_VERSION/libjpeg-turbo-$JPEG_TURBO_VERSION.tar.gz" "$JPEG_TURBO_ARCHIVE" "$JPEG_TURBO_SHA256"
  download_verified_archive "https://downloads.sourceforge.net/project/libpng/libpng16/$LIBPNG_VERSION/libpng-$LIBPNG_VERSION.tar.xz" "$LIBPNG_ARCHIVE" "$LIBPNG_SHA256"
  download_verified_archive "https://ftp.gnu.org/gnu/gettext/gettext-$GETTEXT_VERSION.tar.gz" "$GETTEXT_ARCHIVE" "$GETTEXT_SHA256"
  download_verified_archive "https://github.com/htacg/tidy-html5/archive/refs/tags/$TIDY_VERSION.tar.gz" "$TIDY_ARCHIVE" "$TIDY_SHA256"
fi

rm -rf "$SOURCE_ROOT" "$INSTALL_PREFIX"
mkdir -p "$SOURCE_ROOT" "$INSTALL_PREFIX"

export MACOSX_DEPLOYMENT_TARGET="$MACOS_TARGET"
export SDKROOT="$MACOS_SDK"
export CPPFLAGS="-I$INSTALL_PREFIX/include"
export LDFLAGS="-L$INSTALL_PREFIX/lib -Wl,-headerpad_max_install_names"
export PKG_CONFIG_PATH="$INSTALL_PREFIX/lib/pkgconfig:$INSTALL_PREFIX/share/pkgconfig"
export PKG_CONFIG_LIBDIR="$PKG_CONFIG_PATH:$MACOS_SDK/usr/lib/pkgconfig"
export lt_cv_sys_max_cmd_len="${FABDEV_LIBTOOL_MAX_COMMAND_LENGTH:-786432}"

if [[ "$PROFILE" == "php" ]]; then
  zlib_version="$(sed -n 's/^#define ZLIB_VERSION "\([^"]*\)"/\1/p' "$MACOS_SDK/usr/include/zlib.h")"
  if [[ -z "$zlib_version" ]]; then
    echo "Unable to determine the macOS SDK zlib version" >&2
    exit 1
  fi
  mkdir -p "$INSTALL_PREFIX/lib/pkgconfig"
  printf '%s\n' \
    "prefix=$MACOS_SDK/usr" \
    'exec_prefix=${prefix}' \
    'libdir=${exec_prefix}/lib' \
    'includedir=${prefix}/include' \
    '' \
    'Name: zlib' \
    'Description: macOS SDK zlib' \
    "Version: $zlib_version" \
    'Libs: -L${libdir} -lz' \
    'Cflags: -I${includedir}' \
    > "$INSTALL_PREFIX/lib/pkgconfig/zlib.pc"
fi

openssl_source="$SOURCE_ROOT/openssl-$OPENSSL_VERSION"
tar -xzf "$OPENSSL_ARCHIVE" -C "$SOURCE_ROOT"
(
  cd "$openssl_source"
  openssl_linkage="no-shared"
  if [[ "$PROFILE" == "php" ]]; then
    openssl_linkage="shared"
  fi
  ./Configure darwin64-arm64-cc "$openssl_linkage" no-tests --prefix="$INSTALL_PREFIX" --openssldir=/etc/ssl "-mmacosx-version-min=$MACOS_TARGET"
  make -j "$BUILD_JOBS"
  make install_sw
)

if [[ "$PROFILE" == "mariadb" ]]; then
  pcre2_source="$SOURCE_ROOT/pcre2-$PCRE2_VERSION"
  tar -xjf "$PCRE2_ARCHIVE" -C "$SOURCE_ROOT"
  cmake -S "$pcre2_source" -B "$pcre2_source/_build" -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$INSTALL_PREFIX" -DCMAKE_OSX_SYSROOT="$MACOS_SDK" -DCMAKE_OSX_DEPLOYMENT_TARGET="$MACOS_TARGET" -DBUILD_SHARED_LIBS=OFF -DPCRE2_BUILD_PCRE2_16=OFF -DPCRE2_BUILD_PCRE2_32=OFF -DPCRE2_BUILD_PCRE2GREP=OFF -DPCRE2_BUILD_TESTS=OFF -DPCRE2_SUPPORT_JIT=ON
  cmake --build "$pcre2_source/_build" --parallel "$BUILD_JOBS"
  cmake --install "$pcre2_source/_build"

  if [[ ! -f "$INSTALL_PREFIX/lib/libcrypto.a" || ! -f "$INSTALL_PREFIX/lib/libssl.a" || ! -f "$INSTALL_PREFIX/lib/libpcre2-8.a" ]]
  then
    echo "macOS Runtime dependency profile is incomplete: $INSTALL_PREFIX" >&2
    exit 1
  fi
  echo "Created $PROFILE macOS Runtime dependencies in $INSTALL_PREFIX"
  exit 0
fi

libpng_source="$SOURCE_ROOT/libpng-$LIBPNG_VERSION"
tar -xJf "$LIBPNG_ARCHIVE" -C "$SOURCE_ROOT"
configure_make_install "$libpng_source" --disable-static --enable-shared

jpeg_turbo_source="$SOURCE_ROOT/libjpeg-turbo-$JPEG_TURBO_VERSION"
tar -xzf "$JPEG_TURBO_ARCHIVE" -C "$SOURCE_ROOT"
cmake -S "$jpeg_turbo_source" -B "$jpeg_turbo_source/_build" -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$INSTALL_PREFIX" -DCMAKE_OSX_SYSROOT="$MACOS_SDK" -DCMAKE_OSX_DEPLOYMENT_TARGET="$MACOS_TARGET" -DENABLE_SHARED=TRUE -DENABLE_STATIC=FALSE -DWITH_TOOLS=FALSE -DWITH_TESTS=FALSE -DWITH_TURBOJPEG=FALSE
cmake --build "$jpeg_turbo_source/_build" --parallel "$BUILD_JOBS"
cmake --install "$jpeg_turbo_source/_build"

freetype_source="$SOURCE_ROOT/freetype-$FREETYPE_VERSION"
tar -xJf "$FREETYPE_ARCHIVE" -C "$SOURCE_ROOT"
configure_make_install "$freetype_source" --disable-static --enable-shared --without-brotli --without-bzip2 --without-harfbuzz

oniguruma_source="$SOURCE_ROOT/onig-$ONIGURUMA_VERSION"
tar -xzf "$ONIGURUMA_ARCHIVE" -C "$SOURCE_ROOT"
configure_make_install "$oniguruma_source" --disable-static --enable-shared

libsodium_source="$SOURCE_ROOT/libsodium-$LIBSODIUM_VERSION"
tar -xzf "$LIBSODIUM_ARCHIVE" -C "$SOURCE_ROOT"
configure_make_install "$libsodium_source" --disable-static --enable-shared

libiconv_source="$SOURCE_ROOT/libiconv-$LIBICONV_VERSION"
tar -xzf "$LIBICONV_ARCHIVE" -C "$SOURCE_ROOT"
configure_make_install "$libiconv_source" --disable-static --enable-shared --enable-extra-encodings

icu_source="$SOURCE_ROOT/icu/source"
tar -xzf "$ICU_ARCHIVE" -C "$SOURCE_ROOT"
(
  cd "$icu_source"
  ./runConfigureICU MacOSX --prefix="$INSTALL_PREFIX" --disable-samples --disable-tests --disable-static --enable-shared --with-library-bits=64
  make -j "$BUILD_JOBS"
  make install
)

libxml2_source="$SOURCE_ROOT/libxml2-$LIBXML2_VERSION"
tar -xJf "$LIBXML2_ARCHIVE" -C "$SOURCE_ROOT"
configure_make_install "$libxml2_source" --disable-static --enable-shared --without-lzma --without-python --with-zlib

libxslt_source="$SOURCE_ROOT/libxslt-$LIBXSLT_VERSION"
tar -xJf "$LIBXSLT_ARCHIVE" -C "$SOURCE_ROOT"
configure_make_install "$libxslt_source" --disable-static --enable-shared --without-crypto --without-python

gettext_source="$SOURCE_ROOT/gettext-$GETTEXT_VERSION/gettext-runtime"
tar -xzf "$GETTEXT_ARCHIVE" -C "$SOURCE_ROOT"
configure_make_install "$gettext_source" --disable-static --enable-shared --disable-csharp --disable-java --without-emacs

libzip_source="$SOURCE_ROOT/libzip-$LIBZIP_VERSION"
tar -xJf "$LIBZIP_ARCHIVE" -C "$SOURCE_ROOT"
cmake -S "$libzip_source" -B "$libzip_source/_build" -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$INSTALL_PREFIX" -DCMAKE_OSX_SYSROOT="$MACOS_SDK" -DCMAKE_OSX_DEPLOYMENT_TARGET="$MACOS_TARGET" -DBUILD_SHARED_LIBS=ON -DBUILD_EXAMPLES=OFF -DBUILD_REGRESS=OFF -DBUILD_TOOLS=OFF -DENABLE_BZIP2=OFF -DENABLE_COMMONCRYPTO=ON -DENABLE_GNUTLS=OFF -DENABLE_LZMA=OFF -DENABLE_MBEDTLS=OFF -DENABLE_OPENSSL=OFF -DENABLE_ZSTD=OFF
cmake --build "$libzip_source/_build" --parallel "$BUILD_JOBS"
cmake --install "$libzip_source/_build"

tidy_source="$SOURCE_ROOT/tidy-html5-$TIDY_VERSION"
tar -xzf "$TIDY_ARCHIVE" -C "$SOURCE_ROOT"
CMAKE_POLICY_VERSION_MINIMUM=3.5 cmake -S "$tidy_source" -B "$tidy_source/_build" -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$INSTALL_PREFIX" -DCMAKE_OSX_SYSROOT="$MACOS_SDK" -DCMAKE_OSX_DEPLOYMENT_TARGET="$MACOS_TARGET" -DBUILD_SHARED_LIB=ON -DSUPPORT_CONSOLE_APP=OFF
cmake --build "$tidy_source/_build" --parallel "$BUILD_JOBS"
cmake --install "$tidy_source/_build"

curl_source="$SOURCE_ROOT/curl-$CURL_VERSION"
tar -xjf "$CURL_ARCHIVE" -C "$SOURCE_ROOT"
configure_make_install "$curl_source" --disable-static --enable-shared --disable-docs --disable-ldap --disable-ldaps --disable-manual --without-brotli --without-gssapi --without-libidn2 --without-libpsl --without-libssh2 --without-nghttp2 --without-nghttp3 --without-zstd --with-ca-bundle=/etc/ssl/cert.pem --with-openssl="$INSTALL_PREFIX"

imagemagick_source="$SOURCE_ROOT/ImageMagick-$IMAGEMAGICK_VERSION"
7zz x -y -o"$SOURCE_ROOT" "$IMAGEMAGICK_ARCHIVE"
configure_make_install "$imagemagick_source" --disable-opencl --disable-openmp --disable-static --enable-osx-universal-binary=no --enable-shared --with-freetype=yes --with-heic=no --with-jpeg=yes --with-modules=no --with-png=yes --with-raw=no --with-tiff=no --with-webp=no --without-bzlib --without-djvu --without-fftw --without-fontconfig --without-gslib --without-jxl --without-lcms --without-lqr --without-lzma --without-openexr --without-pango --without-perl --without-raqm --without-rsvg --without-wmf --without-x --without-xml --without-zstd

while IFS= read -r library; do
  library_id="$(otool -D "$library" | sed -n '2p')"
  if [[ -n "$library_id" && "$library_id" != "$INSTALL_PREFIX/lib/"* ]]; then
    install_name_tool -id "$INSTALL_PREFIX/lib/$(basename "$library")" "$library"
  fi
done < <(find "$INSTALL_PREFIX/lib" -type f -name '*.dylib' -print)

while IFS= read -r mach_file; do
  while IFS= read -r dependency; do
    dependency_source=""
    case "$dependency" in
      @rpath/*)
        dependency_source="$INSTALL_PREFIX/lib/${dependency#@rpath/}"
        ;;
      @loader_path/*)
        dependency_source="$(dirname "$mach_file")/${dependency#@loader_path/}"
        ;;
      /*)
        if [[ "$dependency" != "$INSTALL_PREFIX/"* ]]; then
          dependency_source="$INSTALL_PREFIX/lib/$(basename "$dependency")"
        fi
        ;;
      *)
        dependency_source="$INSTALL_PREFIX/lib/$dependency"
        ;;
    esac
    if [[ -n "$dependency_source" && -e "$dependency_source" ]]; then
      install_name_tool -change \
        "$dependency" \
        "$INSTALL_PREFIX/lib/$(basename "$dependency_source")" \
        "$mach_file"
    fi
  done < <(otool -L "$mach_file" | tail -n +2 | awk '{print $1}')
done < <(
  find "$INSTALL_PREFIX/bin" "$INSTALL_PREFIX/sbin" "$INSTALL_PREFIX/lib" \
    -type f \( -perm -111 -o -name '*.dylib' \) -print \
    | while IFS= read -r candidate; do
      if file -b "$candidate" | grep -q 'Mach-O'; then
        echo "$candidate"
      fi
    done
)

required_php_dependencies=(
  lib/libMagickCore-7.Q16HDRI.dylib
  lib/libMagickWand-7.Q16HDRI.dylib
  lib/libcurl.dylib
  lib/libfreetype.dylib
  lib/libicui18n.dylib
  lib/libicuuc.dylib
  lib/libiconv.dylib
  lib/libintl.dylib
  lib/libjpeg.dylib
  lib/libonig.dylib
  lib/libpng.dylib
  lib/libsodium.dylib
  lib/libssl.dylib
  lib/libtidy.dylib
  lib/libxml2.dylib
  lib/libxslt.dylib
  lib/libzip.dylib
)
for relative_path in "${required_php_dependencies[@]}"; do
  if [[ ! -e "$INSTALL_PREFIX/$relative_path" ]]; then
    echo "PHP macOS Runtime dependency profile is incomplete: $relative_path" >&2
    exit 1
  fi
done

echo "Created $PROFILE macOS Runtime dependencies in $INSTALL_PREFIX"
