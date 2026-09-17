#!/usr/bin/env bash
# Build the worker example inside the wasm-dawn builder image with the flags the Linia
# renderer uses: pthreads, shared memory, EXPORT_ALL, wasm longjmp, build-std.
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
    -w "$REPO_ROOT/wasm-example-webgpu-worker" \
    -e SKIA_DEBUG="${SKIA_DEBUG:-0}" \
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
THREADS="-pthread -matomics -mbulk-memory -mmutable-globals -sUSE_PTHREADS=1"
export CFLAGS_wasm32_unknown_emscripten="$THREADS"
export CXXFLAGS_wasm32_unknown_emscripten="$THREADS"
PORT="--use-port=$SKIA_EMDAWNWEBGPU_PKG_DIR/emdawnwebgpu.port.py"
export EMCC_CFLAGS="$PORT $THREADS -sERROR_ON_UNDEFINED_SYMBOLS=0 -sMODULARIZE=1 -sEXPORT_ES6=1 -sEXPORT_NAME=createModule -sEXPORT_ALL=1 -sEXPORTED_RUNTIME_METHODS=WebGPU,HEAPU8 -sWASM_BIGINT=1 -sSTACK_SIZE=5MB -sALLOW_MEMORY_GROWTH=1 -sINITIAL_MEMORY=256MB -sMAXIMUM_MEMORY=4GB -sPTHREAD_POOL_SIZE=1 -sSUPPORT_LONGJMP=wasm -sDISABLE_EXCEPTION_CATCHING=1 -sDISABLE_EXCEPTION_THROWING=1 -fno-exceptions"
export EMCC_CXXFLAGS="$EMCC_CFLAGS"
export RUSTFLAGS="-C panic=abort -C target-feature=+atomics,+bulk-memory,+mutable-globals -C link-arg=-sSUPPORT_LONGJMP=wasm -C link-arg=-sDISABLE_EXCEPTION_CATCHING=1 -C link-arg=-sDISABLE_EXCEPTION_THROWING=1"
cargo +nightly build --release --target wasm32-unknown-emscripten -Z build-std=std,panic_abort
'

OUT=target/wasm32-unknown-emscripten/release
cp "$OUT/wasm-example-webgpu-worker.js" web/wasm_example_webgpu_worker.js
cp "$OUT/wasm_example_webgpu_worker.wasm" web/
shasum -a 256 web/wasm_example_webgpu_worker.wasm
