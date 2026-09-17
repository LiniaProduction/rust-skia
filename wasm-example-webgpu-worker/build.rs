fn main() {
    println!("cargo:rerun-if-changed=src/shim.cpp");
    println!("cargo:rerun-if-env-changed=SKIA_EMDAWNWEBGPU_PKG_DIR");
    let mut build = cc::Build::new();
    build.cpp(true).std("c++20").file("src/shim.cpp");
    if let Ok(pkg_dir) = std::env::var("SKIA_EMDAWNWEBGPU_PKG_DIR") {
        build.flag(format!("-isystem{pkg_dir}/webgpu/include"));
        build.flag(format!("-isystem{pkg_dir}/webgpu_cpp/include"));
    }
    build.compile("shim");
}
