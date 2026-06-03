#!/bin/sh
# Create a STABLE self-signed code-signing certificate for litecast.
#
# Why: macOS ties the Accessibility (TCC) grant and the Keychain "Always Allow"
# decision to the app's code-signing *designated requirement* (DR). Ad-hoc
# signing (`codesign --sign -`) makes the DR a plain `cdhash H"..."` that changes
# on EVERY rebuild, so macOS keeps treating each rebuilt app as a brand-new app
# and you must re-authorize Accessibility after every build.
#
# Signing with a fixed self-signed certificate makes the DR depend on the
# certificate instead of the binary hash, so it stays constant across rebuilds
# and the grants keep applying. This script creates that certificate in your
# *login* keychain (no admin/sudo required) and is safe to re-run (it skips if
# the cert already exists). After running it once, ./scripts/bundle.sh will
# auto-detect and use it.
#
# This script WILL prompt for your macOS login password once (for the keychain
# trust + key-access steps). That is expected, and only happens during setup,
# never per build.
set -e

CERT_NAME="${LITECAST_SIGN_ID:-litecast-dev}"
# The PKCS#12 transport password. It only protects the temporary .p12 file
# during import and is never needed again; a fixed value is fine.
P12_PASS="litecast"

if security find-identity -p codesigning 2>/dev/null | grep -q "\"$CERT_NAME\""; then
    echo "Signing identity \"$CERT_NAME\" already exists and is usable. Nothing to do."
    echo "bundle.sh will use it automatically."
    exit 0
fi

WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

cat > "$WORKDIR/cert.cnf" <<EOF
[ req ]
distinguished_name = dn
x509_extensions = v3
prompt = no
[ dn ]
CN = $CERT_NAME
[ v3 ]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
EOF

echo "Generating self-signed code-signing certificate \"$CERT_NAME\"..."
openssl req -x509 -newkey rsa:2048 -nodes \
    -keyout "$WORKDIR/key.pem" -out "$WORKDIR/cert.pem" -days 3650 \
    -config "$WORKDIR/cert.cnf" >/dev/null 2>&1

# Modern macOS `security import` cannot verify the strong-MAC PKCS#12 that recent
# OpenSSL produces by default, so emit a legacy-format p12 (SHA1/3DES) with a
# transport password.
openssl pkcs12 -export -legacy \
    -inkey "$WORKDIR/key.pem" -in "$WORKDIR/cert.pem" \
    -out "$WORKDIR/identity.p12" -passout "pass:$P12_PASS" \
    -name "$CERT_NAME" >/dev/null 2>&1

LOGIN_KEYCHAIN="$(security login-keychain | tr -d ' "')"
echo "Importing into login keychain: $LOGIN_KEYCHAIN"
# -A makes the key usable by any app; -T /usr/bin/codesign additionally lists
# codesign explicitly. Either way the partition list below is what lets codesign
# actually use the key without erroring out (errSecInternalComponent).
security import "$WORKDIR/identity.p12" -k "$LOGIN_KEYCHAIN" \
    -P "$P12_PASS" -A -T /usr/bin/codesign >/dev/null 2>&1

# A self-signed cert is untrusted by default (CSSMERR_TP_NOT_TRUSTED), which
# keeps it out of `find-identity -v`. Trust it for the code-signing policy only.
echo "Trusting the certificate for code signing (you may be prompted for your"
echo "login password)..."
security add-trusted-cert -d -r trustAsRoot \
    -p codeSign -k "$LOGIN_KEYCHAIN" "$WORKDIR/cert.pem" >/dev/null 2>&1 || \
    echo "note: add-trusted-cert did not complete; codesign may still work via the" \
         "partition list below."

# Allow codesign to use the private key non-interactively from now on. This
# prompts for your login password once (enter it, click Always Allow if asked).
echo "Authorizing codesign to use the key (enter your login password if prompted)..."
security set-key-partition-list -S apple-tool:,apple:,codesign: \
    -s -k "$(read -rs -p '  login password: ' pw </dev/tty; echo "$pw"; echo >/dev/tty)" \
    "$LOGIN_KEYCHAIN" >/dev/null 2>&1 || \
    echo "note: could not set the key partition list; if codesign reports" \
         "errSecInternalComponent, open Keychain Access, find \"$CERT_NAME\"," \
         "and set its private key Access Control to allow codesign."

echo
if security find-identity -p codesigning 2>/dev/null | grep -q "\"$CERT_NAME\""; then
    echo "Done. Stable signing identity \"$CERT_NAME\" is installed and usable."
    echo "Now run:  env -u CARGO_TARGET_DIR ./scripts/bundle.sh"
    echo
    echo "IMPORTANT: because the signature changes from ad-hoc to this new identity,"
    echo "you must re-grant Accessibility ONE more time (System Settings > Privacy &"
    echo "Security > Accessibility: remove the old litecast with '-', re-add the new"
    echo "target/litecast.app, toggle it on). After that, rebuilds keep the grant."
else
    echo "Identity \"$CERT_NAME\" is installed but not yet showing as code-signing-valid."
    echo "If 'bundle.sh' still signs ad-hoc, open Keychain Access, double-click the"
    echo "\"$CERT_NAME\" certificate, expand Trust, and set \"Code Signing\" to"
    echo "\"Always Trust\", then re-run bundle.sh."
fi
