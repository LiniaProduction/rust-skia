#!/usr/bin/env bash
# Build skia-safe for wasm32-unknown-emscripten with the Graphite/Dawn backend,
# inside the builder image.
#
#   ./docker/wasm-dawn/build.sh
#   SKIA_FEATURES=dawn,textlayout ./docker/wasm-dawn/build.sh
#   PLATFORM=linux/amd64 ./docker/wasm-dawn/build.sh   # match the consumer's CI arch
#
# Caches live in named Docker volumes, so a second run reuses the Skia build.
set -euo pipefail

cd "$(dirname "$0")"
REPO_ROOT="$(cd ../.. && pwd)"

IMAGE="${IMAGE:-rust-skia-wasm-dawn}"
# Empty means "host architecture", which on Apple Silicon builds natively instead
# of emulating x86_64 — hours faster. Set PLATFORM=linux/amd64 to reproduce the
# consumer project's CI architecture exactly.
PLATFORM="${PLATFORM:-}"

file_hash() {
    if command -v md5sum >/dev/null 2>&1; then
        md5sum "$1" | cut -d' ' -f1
    else
        md5 -q "$1"
    fi
}

if [ ! -f "$REPO_ROOT/skia-bindings/skia/DEPS" ]; then
    echo "The Skia submodule is not checked out. On the host, run:" >&2
    echo "  git submodule update --init --depth 1 skia-bindings/skia" >&2
    exit 1
fi

# Skia's build runs `git-sync-deps`, so git has to work inside the container, and
# git paths here are unforgiving:
#
#   - in a worktree, .git is a *file* holding an absolute host path;
#   - the Skia submodule's .git holds a path relative to the worktree, so it only
#     resolves at the worktree's original depth in the filesystem.
#
# Mounting the repo at its host path (rather than at /app) satisfies both, and the
# real git directory has to be mounted too because it lives outside the worktree.
# For a plain clone this is simply the repo and its own .git.
GIT_COMMON_DIR="$(cd "$REPO_ROOT" && git rev-parse --path-format=absolute --git-common-dir)"

# macOS ships bash 3.2, where `set -u` treats the expansion of an empty array as
# an unbound variable. Every use below is guarded with `${a[@]+"${a[@]}"}`, which
# expands to nothing when the array is empty and works on bash 3.2 and 5.x alike.
PLATFORM_ARGS=()
if [ -n "$PLATFORM" ]; then
    PLATFORM_ARGS=(--platform "$PLATFORM")
fi

HASH_FILE=".docker_hash"
CURRENT_HASH="$(file_hash Dockerfile)"
if [ ! -f "$HASH_FILE" ] || [ "$(cat "$HASH_FILE")" != "$CURRENT_HASH" ]; then
    echo "=== Dockerfile changed, rebuilding the image ==="
    # The image COPYs nothing, so the context is this directory rather than the
    # repo root — no point shipping hundreds of megabytes of Skia to the daemon.
    docker build ${PLATFORM_ARGS[@]+"${PLATFORM_ARGS[@]}"} --target base -f Dockerfile -t "$IMAGE" .
    echo "$CURRENT_HASH" > "$HASH_FILE"
fi

docker run --rm \
    ${PLATFORM_ARGS[@]+"${PLATFORM_ARGS[@]}"} \
    -v "$REPO_ROOT:$REPO_ROOT" \
    -v "$GIT_COMMON_DIR:$GIT_COMMON_DIR" \
    -v rust-skia-cargo-cache:/cargo-cache \
    -v rust-skia-rustup-cache:/rustup-cache \
    -v rust-skia-emsdk-cache:/emsdk-cache \
    -w "$REPO_ROOT" \
    -e EM_CACHE=/emsdk-cache \
    -e SKIA_FEATURES="${SKIA_FEATURES:-}" \
    -e CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-}" \
    --entrypoint bash \
    "$IMAGE" "$REPO_ROOT/docker/wasm-dawn/entrypoint.sh"
