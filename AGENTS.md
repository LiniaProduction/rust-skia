# AGENTS.md - rust-skia development notes

## Repository structure

Cargo workspace (`resolver = "3"`, edition 2024, MSRV 1.85). Crates:

- `skia-bindings/` - Low-level C++ bindings + the entire Skia build. Contains:
  - `skia/` - Git submodule pointing to `rust-skia/skia` fork (tagged e.g. `m148-0.95.1`)
  - `src/bindings.cpp` - C wrapper functions for Skia C++ APIs (parsed by bindgen)
  - `src/shaper.cpp` - C wrappers for SkShaper / modules
  - `Cargo.toml` - `[package.metadata] skia = "m148-X.Y.Z"` must match submodule tag
  - `build_support/` - Build configuration (e.g. `binaries_config.rs` for platform-specific logic)
- `skia-safe/` - Safe Rust wrappers over `skia-bindings`. Mirrors Skia's include structure:
  - `src/core/` -> `include/core/`
  - `src/gpu/ganesh/` -> `include/gpu/ganesh/`
  - `src/modules/shaper/` -> `modules/skshaper/include/`
  - `src/modules/paragraph/` -> `modules/skparagraph/include/`
  - etc.
- `skia-org/` - Example renderer binary (`default-run = "skia-org"`); ports of skia.org examples to PNG/PDF/SVG via CPU or GPU drivers.
- `skia-svg-macros/` - Proc macros used by the `svg` feature. Published separately (`make publish-svg-macros`).
- `mk-workflows/` - Generator for `.github/workflows/*.yaml`. The workflow files are generated; edit `mk-workflows/` and run `make workflows` instead of hand-editing them.
- `comment-converter/` - Tooling for converting Skia Doxygen comments to Rustdoc.

`skia-bindings` and `skia-safe` share a lockstep version (`skia-safe` depends on `=` the exact `skia-bindings` version).

## Build and test

Everything is feature-driven, and features on `skia-safe` forward to `skia-bindings`. **Changing the feature set changes the binaries key**, which triggers a prebuilt-binary re-download or a full Skia build (tens of minutes). Pick one feature set and stay on it while iterating.

```bash
# The macOS full-feature set used for QA (see Makefile `test-macos`)
cargo build -p skia-safe --features "all-macos,ureq"
cargo test  -p skia-safe --features "all-macos,ureq" --lib
cargo test  -p skia-safe --features "all-macos,ureq" --tests
cargo build -p skia-safe --features "all-macos,ureq" --examples
```

Aggregate feature sets: `all-macos`, `all-linux`, `all-windows`. Default features are `binary-cache,embed-icudtl,pdf,jpeg`.

```bash
# Single test (integration tests live in skia-safe/tests/, unit tests inline in src/)
cargo test -p skia-safe --features "all-macos,ureq" --test codec -- --nocapture
cargo test -p skia-safe --features "all-macos,ureq" some_test_name

# What CI runs (see .github/workflows/*-qa.yaml)
cargo clippy --release --features "<set>" --all-targets -- -D warnings
cargo test --all --release --features "<set>" --all-targets -- --nocapture

# Examples / the example renderer
cargo run --example gl-window --features gl        # add x11 on Linux
cargo run -- [OUTPUT_DIR]                          # skia-org, CPU drivers
cargo run --features gl -- [OUTPUT_DIR] --driver gl
```

Other useful targets:

- `make doc` - build docs with the macOS doc feature set; use it to verify intra-doc links.
- `make diff-api` - public API diff of `skia-safe` against the latest crates.io release (`cargo public-api`).
- `make locate-bindings` / `make open-bindings` - find the most recent generated `bindings.rs`.
- `just check-skia-submodule-tag` - verifies the `skia` submodule tag matches `[package.metadata] skia`.
- `make workflows` - regenerate the GitHub workflow files after changing `mk-workflows/`.

Build-script environment variables (full table in `skia-bindings/README.md`): `SKIA_DEBUG=1` (Skia is built in release even for debug profiles otherwise), `SKIA_BINARIES_URL` (offline prebuilt binaries), `SKIA_SOURCE_DIR`, `SKIA_GN_ARGS`, `SKIA_NINJA_COMMAND` / `SKIA_GN_COMMAND`, `SKIA_USE_SYSTEM_LIBRARIES`, `FORCE_SKIA_BUILD` / `FORCE_SKIA_BINARIES_DOWNLOAD`. Platform prerequisites (LLVM, Python 3, Ninja) and cross-compilation setup are in the top-level `README.md`.

If a rebuild produces stale-looking linker errors after switching targets, `touch skia-bindings/build.rs` to force a rebuild.

## Binding generation

- Bindgen parses the `.cpp` files in `skia-bindings/src/` to discover `extern "C"` functions and types.
- Touch `skia-bindings/src/bindings.cpp` and run `cargo check -p skia-bindings` to force regeneration.
- Generated bindings land in `target/*/build/skia-bindings-*/out/skia/bindings.rs`.
- C++ `enum class` variants get their `k` prefix stripped automatically by bindgen.
- C++ namespaced types like `SkShapers::CT::LineBreakMode` become `SkShapers_CT_LineBreakMode`.

## Wrapper architecture (`skia-safe/src/prelude.rs`)

All safe types are one of three generic wrappers over a bindgen type, chosen by the C++ type's ownership model:

- `Handle<N>` - by-value C++ type with a destructor; `N: NativeDrop`. Optionally `NativeClone`, `NativePartialEq`, `NativeHash`.
- `RCHandle<N>` - `SkRefCnt`-derived type behind a pointer; `N: NativeRefCounted`, usually via `NativeRefCountedBase`. Cloning is a refcount bump.
- `RefHandle<N>` - non-refcounted pointer with a destructor.

Supporting traits: `NativeAccess` (`native()` / `native_mut()`), `NativeBase<Base>` (unsafe upcast used to model C++ inheritance, asserted with `require_base_type!`), `NativeTransmutable<NT>` (bit-for-bit identical layout; the safe type is a plain Rust struct), `ConditionallySend`/`Sendable` (moving non-`Send` handles across threads when `unique()`).

Everything from `src/core/` is re-exported at the crate root (`skia_safe::Canvas`, ...), while `gpu`, `graphite`, `svg`, `skottie`, and `utils` stay namespaced. `src/interop/` holds the bridges to `SkString`, `SkStream`, and `std::vector`-alikes.

## Common patterns in skia-safe

- **Enum wrapping:** `pub type Foo = skia_bindings::SkFoo;` + `variant_name!(Foo::SomeVariant);`
- **Struct wrapping:** `pub type Foo = Handle<SkFoo>;` or `pub type Foo = RCHandle<SkFoo>;`
- **native_transmutable!** for types that are bit-for-bit compatible.
- **require_type_equality!** to verify base class assumptions at compile time (e.g. `SkPixelRef_INHERITED` == `SkRefCnt`).
- **variant_name!** to verify a variant exists at compile time (catches bindgen name changes).
- Platform-gated code uses `#[cfg(any(target_os = "macos", target_os = "ios"))]`.
- Functions returning `Option<Self>` via `Self::from_ptr(unsafe { ... })` - returns `None` if the C function returns null (used for platform-optional features like CoreText).

## Code conventions

Full list in `.github/copilot-instructions.md`; the ones that bite most often:

- Keep Rust type, method, and `Debug` field ordering aligned with the upstream C++ header. Nested C++ types go directly below their parent Rust type.
- Keep diffs strictly scoped; don't refactor adjacent working code or add trait impls (`Clone`, `Debug`, `Default`) that weren't there.
- Derive `Debug` for public types, `Debug` first in the derive list. Prefer `pub`; `pub(crate)` only when the containing module is already crate-public.
- Reduced-feature builds must keep compiling - gate optional code and tests.
- FFI: never pass C++ class types (e.g. `SkFontStyle`) by value across `extern "C"`; use pointers/out-parameters, and placement-new into uninitialized out-parameters of non-trivial types.
- `cargo fmt` and `cargo clippy` clean. If a Clippy suggestion doesn't make sense, `#[allow(clippy::*)]` it with a comment explaining why.
- Deprecations use `since = "0.0.0"` unless a specific version fits better.
- Ask before creating new Markdown files; edits to existing ones are fine.

## Skills

- `.github/skills/cpp-to-rust-documentation/SKILL.md` - porting Doxygen comments to Rustdoc.
- `.github/skills/skia-milestone-update/SKILL.md` - the milestone update procedure.

## Skia milestone update checklist

See the [Template: Skia Milestone Update PR](https://github.com/rust-skia/rust-skia/wiki/Template:-Skia-Milestone-Update-PR) wiki page.

Version numbering: Each milestone bump increments the minor version (e.g. 0.95.0 -> 0.96.0).

For Skia submodule milestone include/API diffs, use direct
`git -C skia-bindings/skia diff OLD_TAG..NEW_TAG -- ...` commands. Do not use
`make diff-skia` for this; that target only compares rust-skia-specific commits
in the Skia submodule against master (it is the "Do the `rust-skia:` commits ...
match with `master`" checklist item, not an include-diff tool).

## Release notes

For rust-skia release notes format and authoring rules, use:

- `.github/release-notes-guidelines.md`
