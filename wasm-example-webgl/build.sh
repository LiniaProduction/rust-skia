#!/usr/bin/env bash
# Build the WebGL (Ganesh) half of the comparison, in the same builder image.
#
#   ./wasm-example-webgl/build.sh
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
    -w "$REPO_ROOT/wasm-example-webgl" \
    --entrypoint bash \
    "$IMAGE" -c '
set -eux
export CARGO_HOME=/cargo-cache RUSTUP_HOME=/rustup-cache
source /emsdk/emsdk_env.sh
export EM_CACHE=/emsdk-cache
git -C / config --global --add safe.directory "*"

export SKIA_NINJA_COMMAND=/usr/bin/ninja
export FORCE_SKIA_BUILD=1
export CC_wasm32_unknown_emscripten=emcc
export CXX_wasm32_unknown_emscripten=em++
export AR_wasm32_unknown_emscripten=emar

# Same longjmp mode as the WebGPU example, for the same reason: it has to agree
# across Skia, the rebuilt std and the link.
export EMCC_CFLAGS="-sERROR_ON_UNDEFINED_SYMBOLS=0 -sSUPPORT_LONGJMP=wasm \
  -sMAX_WEBGL_VERSION=2 -sMODULARIZE=1 -sEXPORT_ES6=1 -sEXPORT_NAME=createModule \
  -sALLOW_MEMORY_GROWTH=1 -sINITIAL_MEMORY=256MB \
  -sEXPORTED_FUNCTIONS=_main,_init,_render,_resize \
  -sEXPORTED_RUNTIME_METHODS=GL"
export EMCC_CXXFLAGS="$EMCC_CFLAGS"

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
cp "$OUT/wasm-example-webgl.js" web/renderer.js
cp "$OUT/wasm_example_webgl.wasm" web/
echo "artifacts in wasm-example-webgl/web/"
