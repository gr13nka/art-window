#!/bin/sh
# Builds "Art Window.app".
#
# A menu bar app on macOS is a directory, a property list and a binary; there is no
# step here worth a build tool. Ad-hoc signing is the last line because an unsigned
# binary that reads another program's database is exactly the shape macOS has spent
# several releases learning to distrust.
#
# --universal builds both Apple silicon and Intel and lipo's them into one executable,
# for a release artifact that runs on either Mac; without it the bundle holds only the
# host architecture, which is all a local install ever needs.
set -eu

cd "$(dirname "$0")/.."

universal=
for arg in "$@"; do
    case "$arg" in
        --universal) universal=1 ;;
        *)
            echo "unknown argument: $arg" >&2
            exit 1
            ;;
    esac
done

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)

APP="target/Art Window.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp macos/Info.plist "$APP/Contents/Info.plist"

if [ -n "$universal" ]; then
    rustup target add aarch64-apple-darwin x86_64-apple-darwin
    export MACOSX_DEPLOYMENT_TARGET=11.0
    cargo build --release --locked --target aarch64-apple-darwin
    cargo build --release --locked --target x86_64-apple-darwin
    lipo -create \
        target/aarch64-apple-darwin/release/art-window \
        target/x86_64-apple-darwin/release/art-window \
        -output "$APP/Contents/MacOS/art-window"
else
    cargo build --release --locked
    cp target/release/art-window "$APP/Contents/MacOS/art-window"
fi

/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $version" "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $version" "$APP/Contents/Info.plist"
codesign --force --sign - "$APP"

echo "built $APP"
echo "install it with:  cp -R '$APP' /Applications/"
