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

# Codesign with a STABLE code identity.
#
# Two macOS permissions are tied to the app's *code identity* (its designated
# requirement, DR): the Keychain "Always Allow" decision for API keys, and —
# crucially — the **Accessibility (TCC)** grant that window management needs.
#
# With pure ad-hoc signing (`--sign -`), the DR is just `cdhash H"..."`, i.e. the
# hash of THIS exact binary. Every rebuild changes the binary, changes the
# cdhash, and therefore changes the DR. macOS then treats the freshly-built app
# as a *different* app than the one you authorized: the old "litecast" row stays
# visible and toggled on in System Settings, but AXIsProcessTrusted() returns
# false because the current binary's DR no longer matches the authorized one.
# That is the infamous "re-grant Accessibility after every rebuild" problem.
#
# The robust fix is to sign with a STABLE self-signed certificate. When signed
# with a named identity, the DR becomes roughly
#   identifier "com.litecast.app" and certificate leaf = H"<cert hash>"
# which depends on the certificate, NOT the binary hash — so it stays constant
# across rebuilds and the Accessibility/Keychain grants keep applying.
#
# Selection order:
#   1. $LITECAST_SIGN_ID  — explicit identity name (overrides everything).
#   2. a self-signed cert named "litecast-dev" in the keychain (auto-detected).
#   3. ad-hoc fallback (`--sign -`) — works, but see the re-grant caveat above.
#
# To create the stable cert once (no admin needed), see scripts/make-signing-cert.sh
# or run: ./scripts/make-signing-cert.sh
IDENTIFIER="com.litecast.app"
ENTITLEMENTS="bundle/litecast.entitlements"
STABLE_CERT_NAME="litecast-dev"

SIGN_ID="${LITECAST_SIGN_ID:-}"
if [ -z "$SIGN_ID" ]; then
    # Auto-detect a stable self-signed identity that is valid for code signing.
    if security find-identity -p codesigning 2>/dev/null | grep -q "\"$STABLE_CERT_NAME\""; then
        SIGN_ID="$STABLE_CERT_NAME"
    fi
fi

# sign_with: run codesign with the given identity; echo nothing, return status.
sign_with() {
    _id="$1"
    if [ -f "$ENTITLEMENTS" ]; then
        codesign --force --deep --sign "$_id" \
            --identifier "$IDENTIFIER" --entitlements "$ENTITLEMENTS" "$APP" 2>/dev/null \
        || codesign --force --deep --sign "$_id" --identifier "$IDENTIFIER" "$APP" 2>/dev/null
    else
        codesign --force --deep --sign "$_id" --identifier "$IDENTIFIER" "$APP" 2>/dev/null
    fi
}

SIGNED_MODE="ad-hoc"
if [ -n "$SIGN_ID" ]; then
    echo "Codesigning $APP (stable identity: $SIGN_ID, identifier=$IDENTIFIER)"
    if sign_with "$SIGN_ID"; then
        SIGNED_MODE="stable"
    else
        echo "warning: signing with \"$SIGN_ID\" failed (key not authorized for codesign?)." >&2
        echo "         Falling back to ad-hoc. See: ./scripts/make-signing-cert.sh" >&2
    fi
fi

if [ "$SIGNED_MODE" != "stable" ]; then
    echo "Codesigning $APP (ad-hoc, identifier=$IDENTIFIER)"
    sign_with - || echo "warning: codesign failed; the app may re-prompt for Keychain/Accessibility access"
    echo "  note: ad-hoc signatures change every rebuild, so macOS may require you to" >&2
    echo "        re-authorize Accessibility (remove litecast with '-', then re-add) after" >&2
    echo "        each bundle. To sign with a STABLE identity and avoid that, run once:" >&2
    echo "            ./scripts/make-signing-cert.sh" >&2
fi

# Report the designated requirement so the signing mode is visible at a glance.
# A stable identity yields an `... and certificate leaf = H"..."` DR (constant
# across rebuilds); ad-hoc yields `cdhash H"..."` (changes every rebuild).
DR="$(codesign -dr - "$APP" 2>&1 | sed -n 's/^#* *designated => //p')"
[ -n "$DR" ] && echo "  designated requirement: $DR"

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
