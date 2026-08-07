# Builder image for the wasm32-unknown-emscripten + Graphite/Dawn (WebGPU) build.
#
# Mirrors the toolchain of the consumer project's builder (clang 18, pinned emsdk,
# ninja) so results transfer, and adds the two things this build needs on top:
# Python 3 for Skia's `git-sync-deps`, and the `emdawnwebgpu` Emscripten port that
# supplies `webgpu/webgpu_cpp.h`.
#
# Emscripten 4.x dropped the legacy `-sUSE_WEBGPU` binding, and Skia does not build
# Dawn from source for wasm (`third_party/dawn/BUILD.gn` leaves the include dirs
# empty for Emscripten). The port package below is the whole WebGPU dependency:
# a few hundred kilobytes of headers and JS glue, no Dawn checkout.

# Pin the Debian release, not just the Rust version: `rust:1-slim` follows Debian
# stable and has moved to trixie, which drops `software-properties-common` and so
# breaks the LLVM apt script. Bookworm is also what the consumer project's builder
# image is based on, so this keeps the two toolchains comparable.
ARG RUST_VERSION=1-slim-bookworm
FROM rust:${RUST_VERSION} AS base

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    gnupg \
    lsb-release \
    ninja-build \
    pkg-config \
    python3 \
    software-properties-common \
    unzip \
    wget \
    xz-utils && \
    rm -rf /var/lib/apt/lists/*

# Clang 18 via the official LLVM script, same as the consumer project's builder.
RUN wget https://apt.llvm.org/llvm.sh && \
    chmod +x llvm.sh && \
    ./llvm.sh 18 && \
    rm llvm.sh && \
    update-alternatives --install /usr/bin/clang clang /usr/bin/clang-18 100 && \
    update-alternatives --install /usr/bin/clang++ clang++ /usr/bin/clang++-18 100

ENV EMSCRIPTEN_VER=4.0.10
RUN git clone https://github.com/emscripten-core/emsdk.git \
    && (cd emsdk && ./emsdk install ${EMSCRIPTEN_VER}) \
    && (cd emsdk && ./emsdk activate ${EMSCRIPTEN_VER})

RUN chmod -R 777 /emsdk
ENV EMSDK=/emsdk

# The `emdawnwebgpu` package, published with Dawn's releases: headers and JS glue,
# no Dawn checkout. Emscripten does ship its own `webgpu/` headers, but they lag —
# Skia m151's Graphite Dawn sources want `DualSourceBlending`, `CoreFeaturesAndLimits`,
# `R16Unorm` and friends, none of which exist in Emscripten 4.x's built-in binding.
#
# Skia m151 pins dawn@7807dcbd (2026-06-28); the oldest published package is
# 2026-07-15, so an exact match does not exist. This is the knob to turn if the
# WebGPU API drifts: override with --build-arg.
ARG EMDAWNWEBGPU_VER=v20260715.205337
RUN mkdir -p /emdawnwebgpu && cd /emdawnwebgpu && \
    curl -fsSL -O "https://github.com/google/dawn/releases/download/${EMDAWNWEBGPU_VER}/emdawnwebgpu_pkg-${EMDAWNWEBGPU_VER}.zip" && \
    unzip -q "emdawnwebgpu_pkg-${EMDAWNWEBGPU_VER}.zip" && \
    rm "emdawnwebgpu_pkg-${EMDAWNWEBGPU_VER}.zip" && \
    test -f /emdawnwebgpu/emdawnwebgpu_pkg/emdawnwebgpu.port.py && \
    test -f /emdawnwebgpu/emdawnwebgpu_pkg/webgpu_cpp/include/webgpu/webgpu_cpp.h

ENV SKIA_EMDAWNWEBGPU_PKG_DIR=/emdawnwebgpu/emdawnwebgpu_pkg

SHELL ["/bin/bash", "-c"]

WORKDIR /app

RUN mkdir -p /cargo-cache /rustup-cache
ENV CARGO_HOME=/cargo-cache
ENV RUSTUP_HOME=/rustup-cache

# CARGO_HOME/RUSTUP_HOME above point at fresh directories, so this re-downloads the
# whole toolchain rather than reusing the one baked into the base image. That is a
# lot of network for one layer, and a single dropped connection used to kill the
# entire image build — hence the retries.
RUN for i in 1 2 3 4 5; do \
        rustup default stable && \
        rustup target add wasm32-unknown-emscripten && \
        break; \
        echo "rustup attempt $i failed, retrying in 10s"; \
        sleep 10; \
    done; \
    rustup target list --installed | grep -q wasm32-unknown-emscripten

# Fresh Docker volumes inherit these cache permissions from the builder image.
RUN mkdir -p /cargo-cache/tmp /rustup-cache/tmp /cargo-target /emsdk-cache && \
    chmod -R 777 /cargo-cache /rustup-cache /cargo-target /emsdk-cache

# There is deliberately no stage that bakes a warm Skia build into the image.
# The repo's .dockerignore excludes skia-bindings/skia, so a COPY-based prewarm
# would not even have Skia to build, and the caches survive in named volumes
# across runs anyway. Nothing is COPYed here, so the build context stays empty.
