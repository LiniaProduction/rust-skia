#!/usr/bin/env bash
# Build environment for the wasm32-unknown-emscripten + Graphite/Dawn build.
# Sourced by entrypoint.sh; keep it side-effect free so it can also be sourced
# by hand inside an interactive container.

export CARGO_TARGET_ROOT="${CARGO_TARGET_ROOT:-${REPO_ROOT:?REPO_ROOT must be set}/target}"
export CARGO_TARGET_DIR="$CARGO_TARGET_ROOT"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-$(nproc)}"
export RUST_BACKTRACE=full

# Skia's own build. `FORCE_SKIA_BUILD` matters because no prebuilt binaries exist
# for this feature combination: the `dawn` feature changes the binaries key, so
# the download would always miss and we would rather fail loudly than silently
# fall back.
export SKIA_NINJA_COMMAND=/usr/bin/ninja
export FORCE_SKIA_BUILD=1

# `dawn` implies `graphite`, and skia-bindings drops `gl` on Emscripten when
# `dawn` is on: WebGL and WebGPU are mutually exclusive in one wasm build.
export SKIA_FEATURES="${SKIA_FEATURES:-dawn,textlayout,svg}"

if [ -z "${SKIA_EMDAWNWEBGPU_PKG_DIR:-}" ]; then
    echo "SKIA_EMDAWNWEBGPU_PKG_DIR is not set (the image should set it)" >&2
    exit 1
fi

# EMCC_CFLAGS is the single owner of `--use-port`: it is what links the JS glue into
# the final binary. skia-bindings deliberately passes only include paths, because emcc
# merges EMCC_CFLAGS into every invocation and errors out on a port named twice.
# `-sASYNCIFY` mirrors CanvasKit — requesting a WebGPU device is asynchronous.
export CC_wasm32_unknown_emscripten=emcc
export CXX_wasm32_unknown_emscripten=em++
export AR_wasm32_unknown_emscripten=emar
export EMCC_CFLAGS="${EMCC_CFLAGS:--sERROR_ON_UNDEFINED_SYMBOLS=0} --use-port=$SKIA_EMDAWNWEBGPU_PKG_DIR/emdawnwebgpu.port.py -sASYNCIFY"
export EMCC_CXXFLAGS="$EMCC_CFLAGS"
