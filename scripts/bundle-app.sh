#!/bin/sh
# Builds dist/HushUp.app: the Swift menu bar app with the Rust CLI inside it.
# Icons live in app/Resources; regenerate with `swift scripts/make-icons.swift app/Resources`.
set -eu
cd "$(dirname "$0")/.."

cargo build --release
swift build -c release --package-path app

APP=dist/HushUp.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp app/.build/release/HushUp "$APP/Contents/MacOS/"
# Resources, not MacOS: "hushup" and "HushUp" collide on case-insensitive volumes
cp target/release/hushup app/Resources/* "$APP/Contents/Resources/"
cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>HushUp</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundleIdentifier</key><string>io.github.hushup.app</string>
    <key>CFBundleName</key><string>HushUp</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>0.1.0</string>
    <key>LSMinimumSystemVersion</key><string>13.0</string>
    <key>LSUIElement</key><true/>
</dict>
</plist>
EOF
codesign --force --deep --sign - "$APP"
echo "built $APP"
