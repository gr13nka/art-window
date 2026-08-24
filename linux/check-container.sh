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
    --workdir /work \
    art-window-linux-check sh -c '
        CARGO_TARGET_DIR=/tmp/art-window-target cargo build --release --locked &&
        CARGO_TARGET_DIR=/tmp/art-window-target cargo test --all-targets --locked &&
        CARGO_TARGET_DIR=/tmp/art-window-target cargo clippy --all-targets --locked -- -D warnings &&
        cargo fmt --check
    '
