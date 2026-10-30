#!/bin/sh
# Builds dist/HushUp.app: the Swift menu bar app with the Rust CLI inside it.
# Icons live in app/Resources; regenerate with `swift scripts/make-icons.swift app/Resources`.
# UNIVERSAL=1 builds arm64 + x86_64 (needs both rustup targets), as the release CI does.
set -eu
cd "$(dirname "$0")/.."

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
if [ "${UNIVERSAL:-}" = 1 ]; then
    cargo build --release --target aarch64-apple-darwin
    cargo build --release --target x86_64-apple-darwin
    mkdir -p target/release
    lipo -create -output target/release/hushup \
        target/aarch64-apple-darwin/release/hushup target/x86_64-apple-darwin/release/hushup
    SWIFT_ARCHS="--arch arm64 --arch x86_64"
else
    cargo build --release
    SWIFT_ARCHS=""
fi
# shellcheck disable=SC2086 # word-splitting the arch flags is intended
swift build -c release --package-path app $SWIFT_ARCHS
SWIFT_BIN=$(swift build -c release --package-path app $SWIFT_ARCHS --show-bin-path)

APP=dist/HushUp.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$SWIFT_BIN/HushUp" "$APP/Contents/MacOS/"
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
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>LSMinimumSystemVersion</key><string>13.0</string>
    <key>LSUIElement</key><true/>
</dict>
</plist>
EOF
codesign --force --deep --sign - "$APP"
echo "built $APP"
