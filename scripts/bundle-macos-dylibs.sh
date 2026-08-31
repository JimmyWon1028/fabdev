#!/usr/bin/env bash

set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <runtime-root> [allowed-dependency-prefix ...]" >&2
  exit 1
fi

RUNTIME_ROOT="$1"
shift
if [[ $# -eq 0 ]]; then
  ALLOWED_DEPENDENCY_PREFIXES=(/opt/homebrew)
else
  ALLOWED_DEPENDENCY_PREFIXES=("$@")
fi
LIB_DIR="$RUNTIME_ROOT/lib"
mkdir -p "$LIB_DIR"

for allowed_prefix in "${ALLOWED_DEPENDENCY_PREFIXES[@]}"; do
  if [[ "$allowed_prefix" != /* || "$allowed_prefix" == "/" ]]; then
    echo "Allowed dependency prefix must be a non-root absolute path: $allowed_prefix" >&2
    exit 1
  fi
done

is_allowed_dependency() {
  local dependency="$1"
  local allowed_prefix

  for allowed_prefix in "${ALLOWED_DEPENDENCY_PREFIXES[@]}"; do
    if [[ "$dependency" == "$allowed_prefix/"* ]]; then
      return 0
    fi
  done
  return 1
}

validate_dependency() {
  local mach_file="$1"
  local dependency="$2"

  case "$dependency" in
    @*|/usr/lib/*|/System/Library/*|"$RUNTIME_ROOT/"*)
      return 0
      ;;
  esac
  if is_allowed_dependency "$dependency"; then
    return 0
  fi
  echo "Mach-O dependency is outside the Runtime and allowed prefixes: $mach_file -> $dependency" >&2
  return 1
}

collect_mach_files() {
  local search_paths=()
  local candidate_path
  for candidate_path in "$RUNTIME_ROOT/bin" "$RUNTIME_ROOT/sbin" "$RUNTIME_ROOT/lib"; do
    if [[ -d "$candidate_path" ]]; then
      search_paths+=("$candidate_path")
    fi
  done

  while IFS= read -r candidate; do
    if file -b "$candidate" | grep -q 'Mach-O'; then
      echo "$candidate"
    fi
  done < <(
    find "${search_paths[@]}" \
      -type f \( -perm -111 -o -name '*.so' -o -name '*.dylib' \) -print
  )
}

copy_dependency() {
  local dependency="$1"
  local destination="$LIB_DIR/$(basename "$dependency")"
  local nested_dependency
  local nested_name
  local nested_source
  local source_rpath
  if [[ ! -f "$destination" ]]; then
    cp -L "$dependency" "$destination"
    chmod u+w "$destination"

    while IFS= read -r nested_dependency; do
      if is_allowed_dependency "$nested_dependency"; then
        copy_dependency "$nested_dependency"
      else
        case "$nested_dependency" in
        @loader_path/*)
          nested_source="$(dirname "$dependency")/${nested_dependency#@loader_path/}"
          if [[ -f "$nested_source" ]]; then
            copy_dependency "$nested_source"
          fi
          ;;
        @rpath/*)
          nested_name="${nested_dependency#@rpath/}"
          nested_source="$(dirname "$dependency")/$nested_name"
          if [[ -f "$nested_source" ]]; then
            copy_dependency "$nested_source"
          else
            while IFS= read -r source_rpath; do
              case "$source_rpath" in
                @loader_path/*)
                  source_rpath="$(dirname "$dependency")/${source_rpath#@loader_path/}"
                  ;;
              esac
              if [[ -f "$source_rpath/$nested_name" ]]; then
                copy_dependency "$source_rpath/$nested_name"
                break
              fi
            done < <(
              otool -l "$dependency" \
                | awk '$1 == "cmd" && $2 == "LC_RPATH" { getline; getline; print $2 }'
            )
          fi
          ;;
        esac
      fi
    done < <(otool -L "$dependency" | tail -n +2 | awk '{print $1}')
  fi
}

runtime_lib_rpath() {
  local mach_file="$1"
  local current_dir
  local relative_path=""

  current_dir="$(dirname "$mach_file")"
  while [[ "$current_dir" != "$LIB_DIR" ]]; do
    if [[ "$current_dir" == "/" ]]; then
      return 1
    fi
    relative_path="../$relative_path"
    current_dir="$(dirname "$current_dir")"
  done

  if [[ -z "$relative_path" ]]; then
    echo '@loader_path'
  else
    echo "@loader_path/${relative_path%/}"
  fi
}

while true; do
  copied=0
  while IFS= read -r mach_file; do
    while IFS= read -r dependency; do
      validate_dependency "$mach_file" "$dependency"
      if is_allowed_dependency "$dependency"; then
        destination="$LIB_DIR/$(basename "$dependency")"
        if [[ ! -f "$destination" ]]; then
          copy_dependency "$dependency"
          copied=1
        fi
      fi
    done < <(otool -L "$mach_file" | tail -n +2 | awk '{print $1}')
  done < <(collect_mach_files)
  if [[ "$copied" -eq 0 ]]; then
    break
  fi
done

while IFS= read -r mach_file; do
  if [[ "$mach_file" == *.dylib ]]; then
    install_name_tool -id "@rpath/$(basename "$mach_file")" "$mach_file"
  fi

  while IFS= read -r dependency; do
    if is_allowed_dependency "$dependency"; then
      install_name_tool -change "$dependency" "@rpath/$(basename "$dependency")" "$mach_file"
    fi
  done < <(otool -L "$mach_file" | tail -n +2 | awk '{print $1}')

  while IFS= read -r search_path; do
    case "$search_path" in
      /*)
        install_name_tool -delete_rpath "$search_path" "$mach_file"
        ;;
    esac
  done < <(
    otool -l "$mach_file" \
      | awk '$1 == "cmd" && $2 == "LC_RPATH" { getline; getline; print $2 }'
  )

  if { [[ "$mach_file" == "$RUNTIME_ROOT/bin/"* ]] || [[ "$mach_file" == "$RUNTIME_ROOT/sbin/"* ]]; } \
    && otool -L "$mach_file" | tail -n +2 | awk '{print $1}' | grep -q '^@rpath/'; then
    if ! otool -l "$mach_file" | grep -q '@executable_path/../lib'; then
      install_name_tool -add_rpath '@executable_path/../lib' "$mach_file"
    fi
  fi

  if [[ "$mach_file" == "$LIB_DIR/"* ]] \
    && otool -L "$mach_file" | tail -n +2 | awk '{print $1}' | grep -q '^@rpath/'; then
    loader_rpath="$(runtime_lib_rpath "$mach_file")"
    if ! otool -l "$mach_file" | grep -Fq "$loader_rpath"; then
      install_name_tool -add_rpath "$loader_rpath" "$mach_file"
    fi
  fi
done < <(collect_mach_files)

while IFS= read -r mach_file; do
  codesign --force --sign - "$mach_file"
done < <(collect_mach_files | sort -r)
