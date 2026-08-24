#!/bin/sh
set -eu

case "$(uname -m)" in
    x86_64) platform=linux/amd64 ;;
    arm64|aarch64) platform=linux/arm64 ;;
    *) echo "unsupported host architecture: $(uname -m)" >&2; exit 1 ;;
esac

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

docker run --rm --platform "$platform" \
    --mount "type=bind,source=$root,target=/work,readonly" \
    --workdir /work \
    rust:1.75-bookworm sh -c '
        apt-get update &&
        apt-get install --yes --no-install-recommends libgtk-3-dev libdbus-1-dev pkg-config &&
        CARGO_TARGET_DIR=/tmp/art-window-target cargo build --release --locked &&
        CARGO_TARGET_DIR=/tmp/art-window-target cargo test --all-targets --locked &&
        CARGO_TARGET_DIR=/tmp/art-window-target cargo clippy --all-targets --locked -- -D warnings &&
        cargo fmt --check
    '
