#!/bin/sh
# Runs the CLI inside an ad-hoc signed, App-Sandboxed bundle with the same
# entitlements as the Mac App Store build (minus the two Apple-restricted
# identifiers an ad-hoc signature cannot carry) and prints where it would put
# received files. Use it before every Mac App Store submission: the sandbox
# redirects $HOME into the container, which once made
# com.apple.security.files.downloads.read-write look unused to App Review.
set -eu
cd "$(dirname "$0")/.."
. "$HOME/.cargo/env" 2>/dev/null || true
cargo build -q -p lan-send-cli
work="${TMPDIR:-/tmp}/lan-send-sandbox-check"
rm -rf "$work"; mkdir -p "$work/Probe.app/Contents/MacOS"
cp target/debug/lan-send "$work/Probe.app/Contents/MacOS/Probe"
cat > "$work/Probe.app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.wangsheng.lansend.sandboxcheck</string>
<key>CFBundleName</key><string>Probe</string>
<key>CFBundleExecutable</key><string>Probe</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0</string>
<key>CFBundleVersion</key><string>0</string>
</dict></plist>
PLIST
cp apps/app/src-tauri/entitlements/mas.plist "$work/entitlements.plist"
/usr/libexec/PlistBuddy -c 'Delete :com.apple.application-identifier' "$work/entitlements.plist"
/usr/libexec/PlistBuddy -c 'Delete :com.apple.developer.team-identifier' "$work/entitlements.plist"
codesign --force --sign - --entitlements "$work/entitlements.plist" "$work/Probe.app" >/dev/null 2>&1
echo "sandboxed identity:"
"$work/Probe.app/Contents/MacOS/Probe" identity | sed 's/^/  /'
echo
case "$("$work/Probe.app/Contents/MacOS/Probe" identity | sed -n 's/^Receive dir: *//p')" in
  */Library/Containers/*) echo "FAIL: receive dir is inside the container; the downloads entitlement is not being used"; exit 1 ;;
  */Downloads) echo "OK: receive dir is the real Downloads folder" ;;
  *) echo "WARN: unexpected receive dir"; exit 1 ;;
esac
