#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "Usage: $0 <runtime-root> <maximum-minimum-os-version>" >&2
  exit 1
fi

RUNTIME_ROOT="$1"
MAXIMUM_MINIMUM_OS_VERSION="$2"

if [[ ! -d "$RUNTIME_ROOT" ]]; then
  echo "Runtime root does not exist: $RUNTIME_ROOT" >&2
  exit 1
fi
if [[ ! "$MAXIMUM_MINIMUM_OS_VERSION" =~ ^[0-9]+\.[0-9]+(\.[0-9]+)?$ ]]; then
  echo "Invalid maximum minimum macOS version: $MAXIMUM_MINIMUM_OS_VERSION" >&2
  exit 1
fi

version_is_at_most() {
  local actual="$1"
  local maximum="$2"
  awk -v actual="$actual" -v maximum="$maximum" 'BEGIN {
    split(actual, actual_parts, ".")
    split(maximum, maximum_parts, ".")
    for (component_index = 1; component_index <= 3; component_index++) {
      actual_part = actual_parts[component_index] + 0
      maximum_part = maximum_parts[component_index] + 0
      if (actual_part < maximum_part) exit 0
      if (actual_part > maximum_part) exit 1
    }
    exit 0
  }'
}

mach_o_count=0
incompatible_count=0
while IFS= read -r binary; do
  mach_o_count=$((mach_o_count + 1))
  minimum_versions="$(/usr/bin/vtool -show-build "$binary" 2>/dev/null | awk '/minos/{print $2}')"
  if [[ -z "$minimum_versions" ]]; then
    echo "Mach-O does not declare a minimum macOS version: $binary" >&2
    incompatible_count=$((incompatible_count + 1))
    continue
  fi
  while IFS= read -r minimum_version; do
    if ! version_is_at_most "$minimum_version" "$MAXIMUM_MINIMUM_OS_VERSION"; then
      echo "Mach-O requires macOS $minimum_version, above $MAXIMUM_MINIMUM_OS_VERSION: $binary" >&2
      incompatible_count=$((incompatible_count + 1))
    fi
  done <<< "$minimum_versions"
done < <(
  find "$RUNTIME_ROOT" -type f -exec file {} + \
    | awk -F': ' '$2 ~ /Mach-O/ { print $1 }'
)

if [[ $mach_o_count -eq 0 ]]; then
  echo "Runtime does not contain a Mach-O binary: $RUNTIME_ROOT" >&2
  exit 1
fi
if [[ $incompatible_count -ne 0 ]]; then
  echo "Runtime contains $incompatible_count incompatible Mach-O minimum version declaration(s)" >&2
  exit 1
fi

echo "Validated $mach_o_count Mach-O files for macOS $MAXIMUM_MINIMUM_OS_VERSION or earlier"
