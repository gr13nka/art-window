#!/bin/sh
# Packages a universal "Art Window.app" as a DMG for GitHub releases: drag-to-Applications,
# the way people expect to install a Mac app they downloaded rather than built.
set -eu

cd "$(dirname "$0")/.."

./macos/bundle.sh --universal

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)

staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT
ditto "target/Art Window.app" "$staging/Art Window.app"
ln -s /Applications "$staging/Applications"

mkdir -p target/dist
dmg="target/dist/Art-Window-$version-macos.dmg"
hdiutil create -volname "Art Window" -srcfolder "$staging" -format UDZO -ov "$dmg"

echo "built $dmg"
