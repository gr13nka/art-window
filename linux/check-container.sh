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
        CARGO_TARGET_DIR=/target cargo build --release --locked &&
        CARGO_TARGET_DIR=/target cargo test --all-targets --locked &&
        CARGO_TARGET_DIR=/target cargo clippy --all-targets --locked -- -D warnings &&
        cargo fmt --check
    '
