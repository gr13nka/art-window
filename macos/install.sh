#!/bin/sh
# Rebuilds Art Window from the current checkout and installs the new app bundle.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
install_dir=${ART_WINDOW_APP_DIR:-/Applications}
app_name="Art Window.app"
built_app="$root/target/$app_name"
installed_app="$install_dir/$app_name"

case "$install_dir" in
    /*) ;;
    *)
        echo "ART_WINDOW_APP_DIR must be an absolute path" >&2
        exit 1
        ;;
esac

if test "$(uname -s)" != Darwin; then
    echo "this installer only runs on macOS" >&2
    exit 1
fi
if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo is required; install Rust 1.88 or newer" >&2
    exit 1
fi
if ! command -v codesign >/dev/null 2>&1; then
    echo "codesign is required; install the Xcode command line tools" >&2
    exit 1
fi

"$root/macos/bundle.sh"

mkdir -p "$install_dir"

# Stop the installed copy before replacing its executable. This is harmless when
# the app is not running, and avoids leaving the old process behind after install.
if pgrep -x art-window >/dev/null 2>&1; then
    osascript -e 'tell application id "dev.artwindow" to quit' >/dev/null 2>&1 || true
fi

rm -rf "$installed_app"
ditto "$built_app" "$installed_app"

echo "installed $installed_app"
echo "open it with:  open '$installed_app'"
