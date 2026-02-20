#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)}"
ARCH="$(uname -m)"
BASE="dist/orchestraterm-$VERSION-macos-$ARCH"

for ext in dmg sha256; do
  if [[ ! -f "$BASE.$ext" ]]; then
    echo "missing: $BASE.$ext" >&2
    exit 1
  fi
done

(
  cd dist
  shasum -a 256 -c "$(basename "$BASE").sha256"
)

hdiutil imageinfo "$BASE.dmg" | rg -n "Format:|Class Name:|Total Bytes:" || true
