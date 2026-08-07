#!/usr/bin/env bash
# Build the WebGPU example inside the wasm-dawn builder image, then copy the wasm and
# its JS loader next to the page.
#
#   ./wasm-example-webgpu/build.sh
#   (cd wasm-example-webgpu/web && python3 -m http.server)
set -euo pipefail

cd "$(dirname "$0")"
REPO_ROOT="$(cd .. && pwd)"
IMAGE="${IMAGE:-rust-skia-wasm-dawn}"

GIT_COMMON_DIR="$(cd "$REPO_ROOT" && git rev-parse --path-format=absolute --git-common-dir)"

docker run --rm \
    -v "$REPO_ROOT:$REPO_ROOT" \
    -v "$GIT_COMMON_DIR:$GIT_COMMON_DIR" \
    -v rust-skia-cargo-cache:/cargo-cache \
    -v rust-skia-rustup-cache:/rustup-cache \
    -v rust-skia-emsdk-cache:/emsdk-cache \
    -w "$REPO_ROOT/wasm-example-webgpu" \
    -e EM_CACHE=/emsdk-cache \
    --entrypoint bash \
    "$IMAGE" -c '
set -eux
export CARGO_HOME=/cargo-cache RUSTUP_HOME=/rustup-cache
source /emsdk/emsdk_env.sh
git -C / config --global --add safe.directory "*"

# Deliberately not sourcing docker/wasm-dawn/env.sh: it is written for the library
# build and expects REPO_ROOT. Everything needed is set explicitly here instead.
# emsdk_env.sh clears EM_CACHE, so restore it afterwards to keep the cache volume.
export EM_CACHE=/emsdk-cache
export SKIA_NINJA_COMMAND=/usr/bin/ninja
export FORCE_SKIA_BUILD=1
export CC_wasm32_unknown_emscripten=emcc
export CXX_wasm32_unknown_emscripten=em++
export AR_wasm32_unknown_emscripten=emar

# --use-port is needed at link time for the JS glue, and belongs only here: passing it
# from skia-bindings as well would make emcc reject the port as declared twice.
PORT="--use-port=$SKIA_EMDAWNWEBGPU_PKG_DIR/emdawnwebgpu.port.py"
# Deliberately no -sASYNCIFY and no EXPORTED_RUNTIME_METHODS=WebGPU,JsValStore, both
# of which CanvasKit passes: JsValStore belongs to the legacy Emscripten WebGPU
# binding and is absent from emdawnwebgpu, and ASYNCIFY conflicts with the
# -fwasm-exceptions that rustc passes. Nothing here blocks on an async call: JS hands
# over a ready device and GetCurrentTexture is synchronous.
# (No apostrophes in this block -- it lives inside a single-quoted bash -c script.)
# SUPPORT_LONGJMP has to agree across Skia, the Rust standard library and the link.
# The prebuilt std for this target uses the wasm variant, so everything else follows
# it: Skia is compiled with the same flag below (this EMCC_CFLAGS applies to its build
# too), and std is rebuilt from source with panic=abort so no legacy SjLj remains.
export EMCC_CFLAGS="$PORT -sERROR_ON_UNDEFINED_SYMBOLS=0 -sSUPPORT_LONGJMP=wasm \
  -sMODULARIZE=1 -sEXPORT_ES6=1 -sEXPORT_NAME=createModule \
  -sALLOW_MEMORY_GROWTH=1 -sINITIAL_MEMORY=256MB \
  -sEXPORTED_FUNCTIONS=_main,_init,_render,_resize"
export EMCC_CXXFLAGS="$EMCC_CFLAGS"

# build-std needs a nightly toolchain and rust-src; both land in the rustup volume,
# so this is only slow the first time.
# Not "rustup default nightly": the cargo-cache volume hides the image CARGO_HOME,
# so rustup cannot place its proxies there. "cargo +nightly" needs none of that, and
# with -Z build-std the standard library is compiled from rust-src rather than
# downloaded, so no target needs adding either.
# nightly + rust-src already live in the rustup volume; installing here would fail
# because rustup wants to write proxies into the CARGO_HOME the volume hides.

export RUSTFLAGS="-C panic=abort"
cargo +nightly build --release --target wasm32-unknown-emscripten -Z build-std=std,panic_abort
'

OUT=target/wasm32-unknown-emscripten/release
# rustc normalises the crate name for the wasm file (underscores) but not for
# the JS loader (hyphens), and the loader fetches the wasm by that exact name,
# so only the JS gets renamed.
cp "$OUT/wasm-example-webgpu.js" web/renderer.js
cp "$OUT/wasm_example_webgpu.wasm" web/
echo "artifacts in wasm-example-webgpu/web/"
