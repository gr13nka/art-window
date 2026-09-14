#!/bin/sh
# Sync Android sources to ios_macmini, build there, and optionally fetch the APK.
#
#   ./remote.sh            tests and assembles on the Mac mini
#   ./remote.sh check      compiles Kotlin only
#   ./remote.sh apk        tests, assembles, and copies android/out/ArtWindow.apk back
#   ./remote.sh <tasks...> runs custom Gradle tasks remotely
set -eu

HOST=ios_macmini
DEST=ArtWindow
HERE=$(cd "$(dirname "$0")" && pwd)

# Excluded build directories survive --delete, keeping the mini's Gradle state warm.
# local.properties is machine-specific; the remote SDK is supplied explicitly below.
rsync -az --delete \
    --exclude '.git/' --exclude '.DS_Store' --exclude 'target/' \
    --exclude '.gradle/' --exclude '.kotlin/' --exclude 'build/' \
    --exclude 'local.properties' --exclude '/android/out/' \
    "$HERE/" "$HOST:$DEST/"

case "${1-build}" in
    build) TASKS='testDebugUnitTest assembleDebug' ;;
    check) TASKS='compileDebugKotlin' ;;
    apk) TASKS='testDebugUnitTest assembleDebug'; FETCH=1 ;;
    *) TASKS="$*" ;;
esac

# The JDK is unpacked rather than registered with java_home on the Mac mini.
# Always stop Gradle after the build: this project sizes the wrapper for one build.
# shellcheck disable=SC2029
ssh "$HOST" "cd '$DEST/android' || exit 1
export JAVA_HOME=\"\$HOME/opt/jdk-17\" ANDROID_HOME=\"\$HOME/Library/Android/sdk\"
./gradlew --console=plain $TASKS
STATUS=\$?
./gradlew --stop
exit \$STATUS"

[ -n "${FETCH-}" ] || exit 0
APK="$HERE/android/out/ArtWindow.apk"
mkdir -p "$HERE/android/out"
rsync -az "$HOST:$DEST/android/app/build/outputs/apk/debug/app-debug.apk" "$APK"
echo "$APK"
