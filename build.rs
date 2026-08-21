fn main() {
    println!("cargo::rustc-check-cfg=cfg(coverage)");
    println!("cargo::rustc-check-cfg=cfg(kani)");
    println!("cargo::rustc-check-cfg=cfg(miri)");
    if std::env::var_os("CARGO_FEATURE_PYTHON").is_some() {
        pyo3_build_config::use_pyo3_cfgs();
    }

    let is_macos = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|target| target == "macos");
    let is_extension = std::env::var_os("CARGO_FEATURE_EXTENSION_MODULE").is_some();

    if is_macos && is_extension {
        println!("cargo::rustc-cdylib-link-arg=-undefined");
        println!("cargo::rustc-cdylib-link-arg=dynamic_lookup");
    }
}
