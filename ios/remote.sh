#!/bin/sh
# Sync the repo to ios_macmini, generate the Xcode project there, and build for iOS.
#
#   ios/remote.sh          runs the framework's tests on the iPhone 17 Pro simulator
#   ios/remote.sh test     the same
#   ios/remote.sh build    builds the app and widget for a device, signed with the mini's team
#   ios/remote.sh install  builds, then installs on a paired device ($DEVICE, else the first)
set -eu

HOST=ios_macmini
DEST=ArtWindow
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(dirname "$HERE")

# The whole repo goes over, not just ios/: the catalogue is bundled from
# ../catalogue and the version is read from Cargo.toml. Build directories survive
# --delete so the mini's derived data stays warm; the .xcodeproj is regenerated.
rsync -az --delete \
    --exclude '.git/' --exclude '.DS_Store' --exclude 'target/' \
    --exclude '.gradle/' --exclude '.kotlin/' --exclude 'build/' \
    --exclude 'local.properties' --exclude '/android/out/' \
    --exclude 'DerivedData/' --exclude '/ios/*.xcodeproj/' \
    "$ROOT/" "$HOST:$DEST/"

# Cargo.toml's [package] version is the only version there is. The build number is
# derived as major*10000+minor*100+patch, the same as Android's versionCode.
VERSION=$(sed -n '/^\[package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -n 1)
BUILD=$(echo "$VERSION" | awk -F. '{ print $1 * 10000 + $2 * 100 + $3 }')
VERSIONING="MARKETING_VERSION=$VERSION CURRENT_PROJECT_VERSION=$BUILD"

SUBCOMMAND=${1-test}
case "$SUBCOMMAND" in
    test)
        ACTION="test -destination 'platform=iOS Simulator,name=iPhone 17 Pro'" ;;
    build|install)
        # DEVELOPMENT_TEAM comes from the mini's own environment, never from the repo.
        ACTION="build -destination 'generic/platform=iOS' -allowProvisioningUpdates DEVELOPMENT_TEAM=\"\${DEVELOPMENT_TEAM-}\"" ;;
    *)
        echo "usage: ios/remote.sh [test|build|install]" >&2
        exit 2 ;;
esac

# XcodeGen is unpacked under ~/opt on the mini rather than installed system-wide.
# shellcheck disable=SC2029
ssh "$HOST" "cd '$DEST/ios' || exit 1
\$HOME/opt/xcodegen/bin/xcodegen --spec project.yml || exit 1
xcodebuild -project ArtWindow.xcodeproj -scheme ArtWindow -derivedDataPath DerivedData \\
    $VERSIONING $ACTION"

[ "$SUBCOMMAND" = install ] || exit 0

# devicectl lists paired devices as a table; the identifier is the third column
# from the right of each connected row, so pick the first UUID-shaped field.
# shellcheck disable=SC2029
ssh "$HOST" "cd '$DEST/ios' || exit 1
DEVICE=\${DEVICE-\$(xcrun devicectl list devices 2>/dev/null \\
    | grep -Eo '[0-9A-F]{8}-([0-9A-F]{4}-){3}[0-9A-F]{12}' | head -n 1)}
[ -n \"\$DEVICE\" ] || { echo 'no paired device; set DEVICE' >&2; exit 1; }
xcrun devicectl device install app --device \"\$DEVICE\" \\
    DerivedData/Build/Products/Debug-iphoneos/ArtWindow.app"
