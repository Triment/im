#!/usr/bin/env bash
# Generates an Android release keystore and prints the values to store as GitHub secrets.
# Usage: scripts/gen-android-keystore.sh [output.jks] [alias]
# KEEP THE .jks AND ITS PASSWORDS SAFE: apps signed with a lost key can never be updated in place.
set -euo pipefail
out="${1:-release-keystore.jks}"
alias="${2:-xmp}"
if [[ -e "$out" ]]; then echo "refusing to overwrite existing $out" >&2; exit 1; fi
read -r -s -p "Keystore password (min 6 chars): " pass; echo
[[ ${#pass} -ge 6 ]] || { echo "password too short" >&2; exit 1; }
keytool -genkeypair -v -keystore "$out" -storetype PKCS12 -alias "$alias" -keyalg RSA -keysize 4096 \
  -validity 36500 -storepass "$pass" -keypass "$pass" \
  -dname "CN=XMP, OU=Mobile, O=XMP, C=CN" >/dev/null
echo
echo "Created $out. Add these repository secrets (Settings -> Secrets and variables -> Actions):"
echo "  ANDROID_KEYSTORE_BASE64   = $(base64 -w0 "$out" 2>/dev/null || base64 "$out" | tr -d '\n')"
echo "  ANDROID_KEYSTORE_PASSWORD = (the password you just typed)"
echo "  ANDROID_KEY_ALIAS         = $alias"
echo "  ANDROID_KEY_PASSWORD      = (same as the keystore password)"
echo
echo "Certificate fingerprint (for your records):"
keytool -list -v -keystore "$out" -alias "$alias" -storepass "$pass" | grep -E "SHA256:" | head -1
