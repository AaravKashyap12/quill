#!/usr/bin/env bash
# Build a local test app with a complete ad-hoc bundle signature. This does not
# use a certificate, change Keychain ACLs, grant permissions, or notarize an app.
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This local app build requires macOS." >&2
  exit 1
fi

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
pnpm tauri build --debug --bundles app --config '{"bundle":{"createUpdaterArtifacts":false}}'

app_bundle="$repo_root/apps/desktop/src-tauri/target/debug/bundle/macos/Quill.app"
bundle_id="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$app_bundle/Contents/Info.plist")"
if [[ "$bundle_id" != "dev.quill.dictation" ]]; then
  echo "Refusing to sign an unexpected app identifier: $bundle_id" >&2
  exit 1
fi

# Sign nested executable code first, then bind the executable, Info.plist, and
# resources as one app. The Rust linker's executable-only signature is not a
# valid bundle signature and must not be used for macOS permission testing.
codesign --force --sign - --identifier "$bundle_id.whisper-server" \
  "$app_bundle/Contents/MacOS/whisper-server"
codesign --force --sign - --identifier "$bundle_id" "$app_bundle"
codesign --verify --deep --strict --verbose=2 "$app_bundle"
echo "Verified local app: $app_bundle"
echo "Install this final build before granting permissions. Rebuilding changes its ad-hoc identity."
