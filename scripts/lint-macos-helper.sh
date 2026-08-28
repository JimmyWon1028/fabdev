#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

xcrun swift format lint \
  --recursive \
  --strict \
  "$REPO_ROOT/helpers/macos/Sources" \
  "$REPO_ROOT/helpers/macos/Tests" \
  "$REPO_ROOT/helpers/macos/Package.swift"
