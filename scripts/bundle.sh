#!/bin/sh
# Build litecast and assemble a macOS .app bundle.
# LITECAST_PROFILE=debug uses target/debug/litecast (parity with cargo run).
set -e

cd "$(dirname "$0")/.."

TARGET_DIR="${CARGO_TARGET_DIR:-target}"
PROFILE="${LITECAST_PROFILE:-release}"

# CARGO_TARGET_DIR gotcha: some sandboxed/agent shells export CARGO_TARGET_DIR
# to a throwaway cache. If it is set, cargo writes the binary there and this
# script bundles from there too — which can pick up a stale or sandbox-only
# build. For a real, reproducible bundle, run this from a normal terminal:
#   env -u CARGO_TARGET_DIR ./scripts/bundle.sh
if [ -n "${CARGO_TARGET_DIR:-}" ]; then
    echo "warning: CARGO_TARGET_DIR is set to '$CARGO_TARGET_DIR'." >&2
    echo "         Building/bundling there instead of ./target. If this is a" >&2
    echo "         sandbox cache, re-run from a normal terminal with:" >&2
    echo "             env -u CARGO_TARGET_DIR ./scripts/bundle.sh" >&2
fi

if [ "$PROFILE" = "debug" ]; then
    echo "Building debug binary (LITECAST_PROFILE=debug)..."
    cargo build
    BIN="$TARGET_DIR/debug/litecast"
else
    echo "Building release binary..."
    cargo build --release
    BIN="$TARGET_DIR/release/litecast"
fi

APP="$TARGET_DIR/litecast.app"
echo "Assembling $APP from $BIN"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
mkdir -p "$APP/Contents/Resources"

if [ ! -f "$BIN" ]; then
    echo "error: binary not found at $BIN (build failed?)" >&2
    rm -rf "$APP"
    exit 1
fi

HOST_ARCH="$(uname -m)"
BIN_ARCH="$(file -b "$BIN" | awk '{print $NF}')"
case "$BIN_ARCH" in
    arm64|x86_64|universal) ;;
    *)
        echo "warning: could not parse binary arch from file(1) (got: $BIN_ARCH)" >&2
        ;;
esac
if [ "$HOST_ARCH" != "$BIN_ARCH" ] && [ "$BIN_ARCH" != "universal" ]; then
    echo "warning: host is $HOST_ARCH but binary is $BIN_ARCH" >&2
fi

cp bundle/Info.plist "$APP/Contents/Info.plist"
cp "$BIN" "$APP/Contents/MacOS/litecast"
chmod +x "$APP/Contents/MacOS/litecast"

# App icon. Contents/Resources/litecast.icns must match CFBundleIconFile
# (= "litecast") in Info.plist, otherwise macOS shows the generic placeholder.
if [ -f bundle/litecast.icns ]; then
    cp bundle/litecast.icns "$APP/Contents/Resources/litecast.icns"
    echo "Bundled icon: $APP/Contents/Resources/litecast.icns"
else
    echo "warning: bundle/litecast.icns missing; app will show the default icon" >&2
fi

# Fail loudly instead of leaving a "damaged or incomplete" bundle behind: macOS
# reports exactly that if Contents/MacOS/<CFBundleExecutable> is missing.
if [ ! -x "$APP/Contents/MacOS/litecast" ]; then
    echo "error: bundle executable missing after copy; aborting" >&2
    rm -rf "$APP"
    exit 1
fi

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

# Local dev: clear Gatekeeper quarantine so `open` works without right-click > Open.
xattr -cr "$APP" 2>/dev/null || true

# Nudge macOS to re-read the bundle's icon. Finder/Launchpad aggressively cache
# icons by bundle id + mtime; touching the bundle bumps its mtime so the new
# .icns is picked up on the next look.
touch "$APP" 2>/dev/null || true

echo "Done: $APP (profile=$PROFILE, arch=$BIN_ARCH)"
echo "Run it with: open \"$APP\"   (first launch: right-click > Open to bypass Gatekeeper)"
echo "Or run the binary directly: \"$APP/Contents/MacOS/litecast\""
if [ "$PROFILE" = "debug" ]; then
    echo "Debug log (debug build only): ~/Library/Application Support/litecast/debug-litecast.log"
else
    echo "Release build: debug logging is compiled out (no stderr spam, no log file)."
fi
echo
echo "Keychain note: the bundled signed app prompts for API-key access only once"
echo "(click \"Always Allow\"). 'cargo run' re-signs each dev build, so it re-prompts."
