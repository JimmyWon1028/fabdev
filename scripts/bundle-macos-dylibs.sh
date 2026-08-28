#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "Usage: $0 <runtime-root>" >&2
  exit 1
fi

RUNTIME_ROOT="$1"
LIB_DIR="$RUNTIME_ROOT/lib"
mkdir -p "$LIB_DIR"

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
      case "$nested_dependency" in
        /opt/homebrew/*)
          copy_dependency "$nested_dependency"
          ;;
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
      case "$dependency" in
        /opt/homebrew/*)
          destination="$LIB_DIR/$(basename "$dependency")"
          if [[ ! -f "$destination" ]]; then
            copy_dependency "$dependency"
            copied=1
          fi
          ;;
      esac
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
    case "$dependency" in
      /opt/homebrew/*)
        install_name_tool -change "$dependency" "@rpath/$(basename "$dependency")" "$mach_file"
        ;;
    esac
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
