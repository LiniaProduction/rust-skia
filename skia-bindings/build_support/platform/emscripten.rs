use std::path::Path;

use super::{generic, prelude::*};

pub struct Emscripten;

impl PlatformDetails for Emscripten {
    fn uses_freetype(&self) -> bool {
        true
    }

    fn gn_args(&self, config: &BuildConfiguration, builder: &mut GnArgsBuilder) {
        let features = &config.features;
        let emsdk_base_dir = emsdk_base_dir();

        builder
            .arg("skia_gl_standard", quote("webgl"))
            .arg("skia_use_webgl", yes_if(features.gpu()))
            .arg("target_cpu", quote("wasm"))
            .arg("skia_emsdk_dir", quote(&emsdk_base_dir));

        // No Dawn is built on wasm: the WebGPU implementation is the browser's, and
        // shaders come from Skia's own WGSL code generator rather than Tint. What Skia
        // does need is a `webgpu/webgpu_cpp.h` recent enough for its Graphite Dawn
        // sources — Emscripten's built-in one is too old (no `DualSourceBlending`,
        // `CoreFeaturesAndLimits`, `R16Unorm`, ...), so the headers come from the
        // `emdawnwebgpu` package published with Dawn's releases.
        //
        // Deliberately not `is_canvaskit`: rust-skia's wasm support is built around
        // non-canvaskit builds — Skia's toolchain only emits the `.wasm.a` archive
        // names that `binaries_config` expects when `is_canvaskit` is false. The Dawn
        // include paths it would otherwise suppress are handled below instead.
        //
        // Include paths rather than `--use-port`: emcc merges EMCC_CFLAGS into every
        // invocation and rejects a port named twice, which it would be for anyone who
        // (correctly) puts `--use-port` in EMCC_CFLAGS to link the JS glue.
        if features.dawn() {
            let pkg_dir = emdawnwebgpu_pkg_dir();
            builder
                .cflag(format!("-isystem{pkg_dir}/webgpu/include"))
                .cflag(format!("-isystem{pkg_dir}/webgpu_cpp/include"));
        }

        // The custom embedded font manager is enabled by default on WASM, but depends
        // on the undefined symbol `SK_EMBEDDED_FONTS`. Enable the custom empty font
        // manager instead so typeface creation still works.
        // See https://github.com/rust-skia/rust-skia/issues/648
        builder
            .arg("skia_enable_fontmgr_custom_embedded", no())
            .arg("skia_enable_fontmgr_custom_empty", yes());
    }

    fn bindgen_args(&self, _target: &cargo::Target, builder: &mut BindgenArgsBuilder) {
        builder.arg("-nobuiltininc");

        // visibility=default, otherwise some types may be missing:
        // <https://github.com/rust-lang/rust-bindgen/issues/751#issuecomment-555735577>
        builder.arg("-fvisibility=default");

        // Bindgen drives libclang directly and the `cc` build of our own .cpp files does
        // not go through emcc's port machinery either, so both need the WebGPU headers
        // spelled out. These must come before Emscripten's sysroot, whose built-in
        // `webgpu/` headers are older and would otherwise win.
        if cfg!(feature = "dawn") {
            let pkg_dir = emdawnwebgpu_pkg_dir();
            builder.arg(format!("-isystem{pkg_dir}/webgpu/include"));
            builder.arg(format!("-isystem{pkg_dir}/webgpu_cpp/include"));
        }

        let emsdk_base_dir = emsdk_base_dir();

        let sysroot_include = format!("{emsdk_base_dir}/upstream/emscripten/cache/sysroot/include");
        if Path::new(&sysroot_include).is_dir() {
            // Newer emsdk versions expose headers via cache/sysroot and reject direct includes
            // from upstream/emscripten/system.
            let libcxx_include = format!("{sysroot_include}/c++/v1");
            if Path::new(&libcxx_include).is_dir() {
                builder.arg(format!("-isystem{libcxx_include}"));
            }
            builder.arg(format!("-isystem{sysroot_include}"));

            // For xlocale.h
            let compat_include = format!("{sysroot_include}/compat");
            if Path::new(&compat_include).is_dir() {
                builder.arg(format!("-isystem{compat_include}"));
            }
            return;
        }

        // Add C++ includes (otherwise build will fail with <cmath> not found)
        let mut add_sys_include = |path: &str| {
            builder.arg(format!(
                "-isystem{emsdk_base_dir}/upstream/emscripten/system/{path}",
            ));
        };

        add_sys_include("lib/libc/musl/arch/emscripten");
        add_sys_include("lib/libc/musl/arch/generic");
        add_sys_include("lib/libcxx/include");
        add_sys_include("lib/libc/musl/include");
        add_sys_include("include");
    }

    fn link_libraries(&self, features: &Features) -> Vec<String> {
        generic::link_libraries(features)
    }

    fn filter_platform_features(
        &self,
        _use_system_libraries: bool,
        mut features: Features,
    ) -> Features {
        features += feature::EMBED_FREETYPE;

        // `gl` and `dawn` may be enabled together: Skia only asserts that Dawn implies
        // Graphite, and CanvasKit ships them separately for packaging reasons rather
        // than because the combination does not build. A consumer that wants WebGPU
        // with a WebGL fallback in one wasm needs both.
        features
    }
}

/// Root of an unpacked `emdawnwebgpu_pkg` from a Dawn release.
///
/// A few hundred kilobytes of headers plus JS glue — not a Dawn checkout. Emscripten
/// ships its own `webgpu/` headers, but they lag Skia's Graphite Dawn sources.
fn emdawnwebgpu_pkg_dir() -> String {
    match cargo::env_var("SKIA_EMDAWNWEBGPU_PKG_DIR") {
        Some(val) => val,
        None => panic!(
            "the `dawn` feature needs SKIA_EMDAWNWEBGPU_PKG_DIR set to an unpacked \
             emdawnwebgpu_pkg from https://github.com/google/dawn/releases"
        ),
    }
}

fn emsdk_base_dir() -> String {
    match std::env::var("EMSDK") {
        Ok(val) => val,
        Err(_e) => panic!(
            "please set the EMSDK environment variable to the root of your Emscripten installation"
        ),
    }
}
