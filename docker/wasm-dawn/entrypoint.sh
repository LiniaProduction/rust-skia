#!/bin/bash
set -e
set -x

export CARGO_HOME=/cargo-cache
export RUSTUP_HOME=/rustup-cache

# The repo is mounted at its host path, not at a fixed location, so derive
# everything from where this script actually lives.
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
export REPO_ROOT

source /emsdk/emsdk_env.sh
source "$REPO_ROOT/docker/wasm-dawn/env.sh"

# The repo is bind-mounted from the host, so its files belong to a different uid.
# Run this from / — inside the repo a stale gitdir would make even `config --global` fail.
git -C / config --global --add safe.directory '*'

cd "$REPO_ROOT"

# build.sh guarantees the submodule is checked out on the host; the container only
# needs git to work well enough for Skia's own `git-sync-deps`.
git -C skia-bindings/skia rev-parse --git-dir >/dev/null || {
    echo "git cannot read the Skia submodule inside the container" >&2
    exit 1
}

rustup target add wasm32-unknown-emscripten

echo "=== Building skia-safe for wasm32-unknown-emscripten (features: $SKIA_FEATURES) ==="

# The library build is what proves the toolchain: it compiles Skia with
# `skia_enable_graphite=true skia_use_dawn=true`, runs Bindgen over the binding
# sources against the emdawnwebgpu headers, and compiles our own .cpp files.
cargo build \
    -p skia-safe \
    --lib \
    --release \
    --target wasm32-unknown-emscripten \
    --features "$SKIA_FEATURES"

echo "=== Build finished ==="
ls -la "$CARGO_TARGET_ROOT/wasm32-unknown-emscripten/release/"*.rlib 2>/dev/null | head
