#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "Usage: $0 <package-manifest> <artifact-dir> <dev|community>" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST_PATH="$1"
ARTIFACT_DIR="$2"
PACKAGE_VARIANT="$3"

if ! command -v jq >/dev/null 2>&1; then
  echo "Missing required command: jq" >&2
  exit 1
fi
if [[ ! -f "$MANIFEST_PATH" ]]; then
  echo "Runtime package manifest does not exist: $MANIFEST_PATH" >&2
  exit 1
fi
if [[ "$(jq -r '.platform' "$MANIFEST_PATH")" != "macos" \
  || "$(jq -r '.architecture' "$MANIFEST_PATH")" != "arm64" ]]
then
  echo "Runtime package manifest must target macOS ARM64" >&2
  exit 1
fi
case "$PACKAGE_VARIANT" in
  dev|community) ;;
  *)
    echo "Unsupported Runtime Package variant: $PACKAGE_VARIANT" >&2
    exit 1
    ;;
esac

mkdir -p "$ARTIFACT_DIR"
while IFS= read -r package; do
  name="$(jq -r '.name' <<< "$package")"
  version="$(jq -r '.version' <<< "$package")"
  source_sha256="$(jq -r '.source.archiveSha256' <<< "$package")"
  fingerprint="$(jq -r '.source.verification.fingerprint // empty' <<< "$package")"
  minimum_macos_version="$(jq -r '.minimumOsVersion' <<< "$package")"
  build_profile="$(jq -r '.buildProfile // "default"' <<< "$package")"

  if [[ -z "$name" || -z "$version" || ! "$source_sha256" =~ ^[0-9a-f]{64}$ \
    || ! "$fingerprint" =~ ^[0-9A-F]{40}$ ]]
  then
    echo "Invalid macOS Runtime package definition: $name $version" >&2
    exit 1
  fi

  case "$name" in
    dnsmasq)
      [[ "$PACKAGE_VARIANT" == "dev" ]] || {
        echo "dnsmasq is only supported as a bundled Runtime" >&2
        exit 1
      }
      FABDEV_ARTIFACT_DIR="$ARTIFACT_DIR" \
      MACOSX_DEPLOYMENT_TARGET="$minimum_macos_version" \
      DNSMASQ_VERSION="$version" \
      DNSMASQ_SHA256="$source_sha256" \
      DNSMASQ_RELEASE_FINGERPRINT="$fingerprint" \
        "$SCRIPT_DIR/build-dnsmasq-runtime.sh"
      ;;
    nginx)
      [[ "$PACKAGE_VARIANT" == "dev" ]] || {
        echo "Nginx is only supported as a bundled Runtime" >&2
        exit 1
      }
      FABDEV_ARTIFACT_DIR="$ARTIFACT_DIR" \
      MACOSX_DEPLOYMENT_TARGET="$minimum_macos_version" \
      NGINX_VERSION="$version" \
      NGINX_SHA256="$source_sha256" \
      NGINX_RELEASE_FINGERPRINT="$fingerprint" \
        "$SCRIPT_DIR/build-nginx-runtime.sh"
      ;;
    php)
      FABDEV_ARTIFACT_DIR="$ARTIFACT_DIR" \
      FABDEV_RUNTIME_PACKAGE_VARIANT="$PACKAGE_VARIANT" \
      FABDEV_PHP_BUILD_PROFILE="$build_profile" \
      MACOSX_DEPLOYMENT_TARGET="$minimum_macos_version" \
      PHP_VERSION="$version" \
      PHP_SHA256="$source_sha256" \
      PHP_RELEASE_FINGERPRINT="$fingerprint" \
        "$SCRIPT_DIR/build-php-runtime.sh"
      ;;
    mariadb)
      FABDEV_ARTIFACT_DIR="$ARTIFACT_DIR" \
      FABDEV_RUNTIME_PACKAGE_VARIANT="$PACKAGE_VARIANT" \
      MACOSX_DEPLOYMENT_TARGET="$minimum_macos_version" \
      MARIADB_VERSION="$version" \
      MARIADB_SHA256="$source_sha256" \
      MARIADB_RELEASE_FINGERPRINT="$fingerprint" \
        "$SCRIPT_DIR/build-mariadb-runtime.sh"
      ;;
    node)
      FABDEV_ARTIFACT_DIR="$ARTIFACT_DIR" \
      FABDEV_RUNTIME_PACKAGE_VARIANT="$PACKAGE_VARIANT" \
      FABDEV_MINIMUM_MACOS_VERSION="$minimum_macos_version" \
      NODE_VERSION="$version" \
      NODE_SHA256="$source_sha256" \
      NODE_RELEASE_FINGERPRINT="$fingerprint" \
        "$SCRIPT_DIR/build-node-runtime.sh"
      ;;
    *)
      echo "Unsupported macOS Runtime package: $name" >&2
      exit 1
      ;;
  esac
done < <(jq -c '.packages[]' "$MANIFEST_PATH")

echo "Built every macOS ARM64 Runtime declared by $MANIFEST_PATH"
