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

# Ad-hoc codesign with a STABLE identifier.
#
# litecast stores AI API keys in the macOS Keychain. The Keychain ties an
# "Always Allow" decision to the calling binary's *code identity*. An unsigned
# binary (or one signed with a different/ephemeral identity on each build) gets
# a new identity every launch, so macOS re-prompts for Keychain access every
# time. Signing ad-hoc (`--sign -`) with a fixed `--identifier` gives the
# bundled app a consistent code identity across runs, so a single "Always Allow"
# sticks for the bundled app.
#
# This is best-effort: if `codesign` is unavailable the bundle is still usable
# (it will simply re-prompt), so we don't fail the build on a signing error.
IDENTIFIER="com.litecast.app"
ENTITLEMENTS="bundle/litecast.entitlements"
echo "Codesigning $APP (ad-hoc, identifier=$IDENTIFIER)"
if [ -f "$ENTITLEMENTS" ]; then
    codesign --force --deep --sign - \
        --identifier "$IDENTIFIER" \
        --entitlements "$ENTITLEMENTS" \
        "$APP" 2>/dev/null \
    || codesign --force --deep --sign - --identifier "$IDENTIFIER" "$APP" \
    || echo "warning: codesign failed; the app will re-prompt for Keychain access each launch"
else
    codesign --force --deep --sign - --identifier "$IDENTIFIER" "$APP" \
    || echo "warning: codesign failed; the app will re-prompt for Keychain access each launch"
fi

echo "Done: $APP"
echo "Run it with: open \"$APP\"   (first launch: right-click > Open to bypass Gatekeeper)"
echo "Or run the binary directly: \"$APP/Contents/MacOS/litecast\""
echo
echo "Keychain note: the bundled signed app prompts for API-key access only once"
echo "(click \"Always Allow\"). 'cargo run' re-signs each dev build, so it re-prompts."
