fn main() {
    println!("cargo:rerun-if-changed=src/webgpu_shim.cpp");
    println!("cargo:rerun-if-env-changed=SKIA_EMDAWNWEBGPU_PKG_DIR");

    let mut build = cc::Build::new();
    build.cpp(true).std("c++20").file("src/webgpu_shim.cpp");

    // emcc resolves `--use-port` itself, but the `cc` invocation here does not go
    // through the port machinery, so point it at the same headers skia-bindings uses.
    if let Ok(pkg_dir) = std::env::var("SKIA_EMDAWNWEBGPU_PKG_DIR") {
        build.flag(format!("-isystem{pkg_dir}/webgpu/include"));
        build.flag(format!("-isystem{pkg_dir}/webgpu_cpp/include"));
    } else {
        println!(
            "cargo:warning=SKIA_EMDAWNWEBGPU_PKG_DIR is unset; \
             the WebGPU headers will have to come from the toolchain"
        );
    }

    build.compile("webgpu_shim");
}
