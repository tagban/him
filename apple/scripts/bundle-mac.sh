#!/usr/bin/env bash
# Builds "HIM Modern.app" (the modern macOS build) into apple/build/.
#   scripts/bundle-mac.sh            ad-hoc signed, for running here
#   SIGN="Developer ID Application: ..." scripts/bundle-mac.sh   signed for others (hardened runtime)
set -euo pipefail
cd "$(dirname "$0")/.."
CONFIG="${CONFIG:-release}"
if ! swift build -c "$CONFIG" > build.log 2>&1; then
  grep -E "error" build.log >&2
  echo "build failed (apple/build.log)" >&2
  exit 1
fi
BIN="$(swift build -c "$CONFIG" --show-bin-path)/HIMMac"
APP="build/HIM Modern.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/HIM"
[ -f Resources/AppIcon.icns ] && cp Resources/AppIcon.icns "$APP/Contents/Resources/"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' ../him-ffi/Cargo.toml | head -1)"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>com.tagban.him.modern</string>
  <key>CFBundleName</key><string>HIM</string>
  <key>CFBundleDisplayName</key><string>HIM</string>
  <key>CFBundleExecutable</key><string>HIM</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.social-networking</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>MIT License</string>
</dict>
</plist>
PLIST
if [ -n "${SIGN:-}" ]; then
  codesign --force --options runtime --timestamp --sign "$SIGN" "$APP"
else
  codesign --force --sign - "$APP"
fi
echo "built: $APP"
