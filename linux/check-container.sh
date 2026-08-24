#!/bin/sh
set -eu

case "$(uname -m)" in
    x86_64) platform=linux/amd64 ;;
    arm64|aarch64) platform=linux/arm64 ;;
    *) echo "unsupported host architecture: $(uname -m)" >&2; exit 1 ;;
esac

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

docker build --platform "$platform" \
    --tag art-window-linux-check \
    --file "$root/linux/Dockerfile.check" \
    "$root/linux"

docker run --rm --platform "$platform" \
    --mount "type=bind,source=$root,target=/work,readonly" \
    --mount "type=volume,source=art-window-cargo-registry,target=/usr/local/cargo/registry" \
    --mount "type=volume,source=art-window-linux-target,target=/target" \
    --workdir /work \
    art-window-linux-check sh -c '
        set -eu
        CARGO_TARGET_DIR=/target cargo build --release --locked
        CARGO_TARGET_DIR=/target cargo test --all-targets --locked
        CARGO_TARGET_DIR=/target cargo clippy --all-targets --locked -- -D warnings
        cargo fmt --check
        sh -n linux/check-container.sh linux/install.sh

        install_root=$(mktemp -d)
        CARGO_TARGET_DIR=/target \
            ART_WINDOW_PREFIX="$install_root" \
            XDG_DATA_HOME="$install_root/share" \
            ./linux/install.sh
        test -x "$install_root/bin/art-window"
        grep -F "Exec=\"$install_root/bin/art-window\"" \
            "$install_root/share/applications/dev.artwindow.desktop"
        gdk-pixbuf-thumbnailer -s 64 linux/dev.artwindow.svg \
            "$install_root/dev.artwindow.png"
        test -s "$install_root/dev.artwindow.png"
    '
