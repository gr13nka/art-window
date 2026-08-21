#!/bin/sh
# Builds "Art Window.app".
#
# A menu bar app on macOS is a directory, a property list and a binary; there is no
# step here worth a build tool. Ad-hoc signing is the last line because an unsigned
# binary that reads another program's database is exactly the shape macOS has spent
# several releases learning to distrust.
set -eu

cd "$(dirname "$0")/.."
cargo build --release

APP="target/Art Window.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp macos/Info.plist "$APP/Contents/Info.plist"
cp target/release/art-window "$APP/Contents/MacOS/art-window"
codesign --force --sign - "$APP"

echo "built $APP"
echo "install it with:  cp -R '$APP' /Applications/"
