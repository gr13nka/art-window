#!/bin/sh
# Builds the debug APK from the current checkout and installs it on a connected device.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
apk="$root/app/build/outputs/apk/debug/app-debug.apk"

if ! command -v adb >/dev/null 2>&1; then
    echo "adb is required; install the Android SDK platform-tools" >&2
    exit 1
fi
if [ "$(adb get-state 2>/dev/null || true)" != "device" ]; then
    echo "no device attached; connect one (or start an emulator) and enable USB debugging" >&2
    exit 1
fi

# Gradle 8.10 cannot run on the newest JDKs, and a JAVA_HOME left pointing at a
# removed JDK fails before anything builds. Fall back to macOS's own record of JDK 17.
if [ ! -x "${JAVA_HOME:-}/bin/java" ] && [ -x /usr/libexec/java_home ]; then
    JAVA_HOME=$(/usr/libexec/java_home -v 17)
    export JAVA_HOME
fi

"$root/gradlew" -p "$root" assembleDebug

adb install -r "$apk"
adb shell am start -n dev.artwindow/.MainActivity

echo "installed and launched dev.artwindow"
