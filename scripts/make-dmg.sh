#!/bin/sh
# Packs dist/HushUp.app (from bundle-app.sh) into dist/HushUp-<version>.dmg
# with an Applications shortcut to drag onto.
set -eu
cd "$(dirname "$0")/.."

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
STAGE=dist/dmg
DMG=dist/HushUp-$VERSION.dmg
rm -rf "$STAGE" "$DMG"
mkdir -p "$STAGE"
cp -R dist/HushUp.app "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "HushUp $VERSION" -srcfolder "$STAGE" -fs HFS+ -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"
echo "built $DMG"
