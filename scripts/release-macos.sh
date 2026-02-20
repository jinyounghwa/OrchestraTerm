#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: macOS only" >&2
  exit 1
fi

ARCH="$(uname -m)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)"
DIST="dist"
APP="OrchestraTerm"
BIN="orchestraterm"
PKG="orchestraterm-$VERSION-macos-$ARCH"
APPDIR="$DIST/$APP.app"
DMGROOT="$DIST/dmg-root"
ICON_PNG="assets/AppIcon.png"
ICONSET="$DIST/AppIcon.iconset"
ICON_ICNS="$DIST/AppIcon.icns"

rm -rf "$DIST"
mkdir -p "$APPDIR/Contents/MacOS" "$APPDIR/Contents/Resources"

cargo build --release
cp "target/release/$BIN" "$APPDIR/Contents/MacOS/$BIN"
chmod +x "$APPDIR/Contents/MacOS/$BIN"

if [[ -f "$ICON_PNG" ]]; then
  mkdir -p "$ICONSET"
  sips -z 16 16 "$ICON_PNG" --out "$ICONSET/icon_16x16.png" >/dev/null
  sips -z 32 32 "$ICON_PNG" --out "$ICONSET/icon_16x16@2x.png" >/dev/null
  sips -z 32 32 "$ICON_PNG" --out "$ICONSET/icon_32x32.png" >/dev/null
  sips -z 64 64 "$ICON_PNG" --out "$ICONSET/icon_32x32@2x.png" >/dev/null
  sips -z 128 128 "$ICON_PNG" --out "$ICONSET/icon_128x128.png" >/dev/null
  sips -z 256 256 "$ICON_PNG" --out "$ICONSET/icon_128x128@2x.png" >/dev/null
  sips -z 256 256 "$ICON_PNG" --out "$ICONSET/icon_256x256.png" >/dev/null
  sips -z 512 512 "$ICON_PNG" --out "$ICONSET/icon_256x256@2x.png" >/dev/null
  sips -z 512 512 "$ICON_PNG" --out "$ICONSET/icon_512x512.png" >/dev/null
  sips -z 1024 1024 "$ICON_PNG" --out "$ICONSET/icon_512x512@2x.png" >/dev/null
  iconutil -c icns "$ICONSET" -o "$ICON_ICNS"
  cp "$ICON_ICNS" "$APPDIR/Contents/Resources/AppIcon.icns"
fi

cat > "$APPDIR/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>$APP</string>
  <key>CFBundleIdentifier</key><string>com.orchestraterm.app</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleExecutable</key><string>$BIN</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
</dict></plist>
PLIST

mkdir -p "$DMGROOT"
cp -R "$APPDIR" "$DMGROOT/"
ln -s /Applications "$DMGROOT/Applications"
if [[ -f "$ICON_ICNS" ]]; then
  cp "$ICON_ICNS" "$DMGROOT/.VolumeIcon.icns"
  if command -v SetFile >/dev/null 2>&1; then
    SetFile -a C "$DMGROOT" || true
  fi
fi

hdiutil create -volname "$APP" -srcfolder "$DMGROOT" -ov -format UDZO "$DIST/$PKG.dmg" >/dev/null
(
  cd "$DIST"
  shasum -a 256 "$PKG.dmg" > "$PKG.sha256"
)

echo "done: $DIST/$PKG.dmg"
