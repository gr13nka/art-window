#!/bin/sh
# Installs a user-local GNOME launcher and icon, either from a release
# tarball's prebuilt binary or by building the checkout with cargo.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
home_dir=${HOME:?HOME is not set}
prefix=${ART_WINDOW_PREFIX:-"$home_dir/.local"}
data_home=${XDG_DATA_HOME:-"$prefix/share"}
binary_dir="$prefix/bin"
applications_dir="$data_home/applications"
icons_dir="$data_home/icons/hicolor/scalable/apps"
binary="$binary_dir/art-window"
desktop="$applications_dir/dev.artwindow.desktop"

if [ -f "$script_dir/art-window" ]; then
    # Release tarball layout: the binary sits next to this script.
    built_binary="$script_dir/art-window"
else
    # Checkout layout: no prebuilt binary, so build one.
    root=$(CDPATH= cd -- "$script_dir/.." && pwd)
    build_target=${CARGO_TARGET_DIR:-"$root/target"}
    case "$build_target" in
        /*) ;;
        *) build_target="$root/$build_target" ;;
    esac

    if ! command -v cargo >/dev/null 2>&1; then
        echo "cargo is required; install Rust 1.88 or newer" >&2
        exit 1
    fi
    if ! command -v pkg-config >/dev/null 2>&1 \
        || ! pkg-config --exists gtk+-3.0 dbus-1; then
        echo "GTK 3 and D-Bus development files are required" >&2
        echo "Debian/Ubuntu: sudo apt install libgtk-3-dev libdbus-1-dev pkg-config" >&2
        echo "Arch Linux: sudo pacman -S --needed pkgconf gtk3 dbus" >&2
        echo "NixOS: nix-shell -p rustc cargo pkg-config gtk3 dbus --run './linux/install.sh'" >&2
        exit 1
    fi

    (cd "$root" && cargo build --release --locked)
    built_binary="$build_target/release/art-window"
fi

mkdir -p "$binary_dir" "$applications_dir" "$icons_dir"
install -m 0755 "$built_binary" "$binary"
install -m 0644 "$script_dir/dev.artwindow.svg" "$icons_dir/dev.artwindow.svg"

# Quote the absolute executable as one desktop-entry Exec argument. The desktop
# grammar consumes one layer of backslashes and the Exec grammar consumes the
# second, matching the autostart entry the program writes for itself.
escaped=$(printf '%s' "$binary" | sed 's/[\\"`$]/\\&/g; s/\\/\\\\/g')
exec_value="\"$escaped\""
desktop_tmp=$(mktemp)
trap 'rm -f "$desktop_tmp"' EXIT HUP INT TERM
while IFS= read -r line || test -n "$line"; do
    case "$line" in
        'Exec=@EXEC@') printf 'Exec=%s\n' "$exec_value" ;;
        *) printf '%s\n' "$line" ;;
    esac
done < "$script_dir/dev.artwindow.desktop.in" > "$desktop_tmp"
install -m 0644 "$desktop_tmp" "$desktop"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$applications_dir" >/dev/null 2>&1 || true
fi

echo "installed $binary"
echo "installed $desktop"
echo "open Art Window from GNOME, or run: $binary"
