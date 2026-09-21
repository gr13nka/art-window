#!/bin/sh
# Builds a release tarball: a prebuilt binary plus install.sh, so a release
# installs without a Rust toolchain (GTK 3 is still needed at runtime).
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
version=$(cd "$root" && sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
if [ -z "$version" ]; then
    echo "could not read version from $root/Cargo.toml" >&2
    exit 1
fi
build_target=${CARGO_TARGET_DIR:-"$root/target"}
case "$build_target" in
    /*) ;;
    *) build_target="$root/$build_target" ;;
esac

(cd "$root" && cargo build --release --locked)

name="art-window-$version-linux-x86_64"
stage_dir=$(mktemp -d)
trap 'rm -rf "$stage_dir"' EXIT HUP INT TERM
package_dir="$stage_dir/$name"
mkdir -p "$package_dir"

cp -p "$build_target/release/art-window" "$package_dir/art-window"
cp -p "$root/linux/install.sh" "$package_dir/install.sh"
cp -p "$root/linux/dev.artwindow.desktop.in" "$package_dir/dev.artwindow.desktop.in"
cp -p "$root/linux/dev.artwindow.svg" "$package_dir/dev.artwindow.svg"

dist_dir="$root/target/dist"
mkdir -p "$dist_dir"
archive="$dist_dir/$name.tar.gz"
tar -czf "$archive" -C "$stage_dir" "$name"

echo "$archive"
