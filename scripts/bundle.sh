#!/bin/sh
# Build litecast in release mode and assemble a macOS .app bundle.
set -e

cd "$(dirname "$0")/.."

echo "Building release binary..."
cargo build --release

TARGET_DIR="${CARGO_TARGET_DIR:-target}"
APP="$TARGET_DIR/litecast.app"
echo "Assembling $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
mkdir -p "$APP/Contents/Resources"

cp bundle/Info.plist "$APP/Contents/Info.plist"
cp "$TARGET_DIR/release/litecast" "$APP/Contents/MacOS/litecast"
chmod +x "$APP/Contents/MacOS/litecast"

echo "Done: $APP"
echo "Run it with: open \"$APP\"   (first launch: right-click > Open to bypass Gatekeeper)"
echo "Or run the binary directly: \"$APP/Contents/MacOS/litecast\""
