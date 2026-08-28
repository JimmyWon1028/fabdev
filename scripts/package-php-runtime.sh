#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "Usage: $0 <runtime-root> <php-version> <artifact-dir>" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
RUNTIME_ROOT="$1"
PHP_VERSION="$2"
ARTIFACT_DIR="$3"
ARCHIVE_NAME="php-$PHP_VERSION-macos-arm64-dev.tar.gz"

if [[ ! -x "$RUNTIME_ROOT/bin/php" || ! -x "$RUNTIME_ROOT/sbin/php-fpm" ]]; then
  echo "Runtime does not contain PHP CLI and FPM binaries: $RUNTIME_ROOT" >&2
  exit 1
fi

mkdir -p "$ARTIFACT_DIR"
"$SCRIPT_DIR/bundle-macos-dylibs.sh" "$RUNTIME_ROOT"

"$RUNTIME_ROOT/bin/php" --ini
"$RUNTIME_ROOT/bin/php" -v
php_modules="$("$RUNTIME_ROOT/bin/php" -m)"
echo "$php_modules"
for required_module in imagick imap tidy; do
  if ! grep -Fxq "$required_module" <<< "$php_modules"; then
    echo "Runtime does not load required PHP module: $required_module" >&2
    exit 1
  fi
done
"$RUNTIME_ROOT/bin/php" -r '
  $image = new Imagick();
  $image->newImage(2, 2, "white");
  $image->setImageFormat("png");
  if ($image->getImagesBlob() === "") {
    fwrite(STDERR, "Imagick PNG encoding failed\n");
    exit(1);
  }
  if (!function_exists("imap_open")) {
    fwrite(STDERR, "IMAP function check failed\n");
    exit(1);
  }
  if (tidy_parse_string("<p>ok</p>") === false) {
    fwrite(STDERR, "Tidy parsing failed\n");
    exit(1);
  }
  echo "Imagick PNG, IMAP, and Tidy functional checks passed\n";
'
"$RUNTIME_ROOT/sbin/php-fpm" --fpm-config "$RUNTIME_ROOT/etc/php-fpm.conf" --test

COPYFILE_DISABLE=1 tar -czf "$ARTIFACT_DIR/$ARCHIVE_NAME" -C "$(dirname "$RUNTIME_ROOT")" "$(basename "$RUNTIME_ROOT")"
artifact_sha256="$(shasum -a 256 "$ARTIFACT_DIR/$ARCHIVE_NAME" | awk '{print $1}')"
artifact_size="$(stat -f '%z' "$ARTIFACT_DIR/$ARCHIVE_NAME")"

sed \
  -e "s|@NAME@|php|g" \
  -e "s|@VERSION@|$PHP_VERSION|g" \
  -e "s|@ARCHIVE@|$ARCHIVE_NAME|g" \
  -e "s|@SIZE@|$artifact_size|g" \
  -e "s|@SHA256@|$artifact_sha256|g" \
  -e "s|@SIGNATURE@|development-ad-hoc|g" \
  "$PROJECT_DIR/resources/runtime/release.template.json" \
  > "$ARTIFACT_DIR/php-$PHP_VERSION-macos-arm64-dev.json"

echo "Created $ARTIFACT_DIR/$ARCHIVE_NAME"
echo "SHA-256: $artifact_sha256"
