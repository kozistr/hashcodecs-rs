fn main() {
    println!("cargo::rustc-check-cfg=cfg(coverage)");
    println!("cargo::rustc-check-cfg=cfg(kani)");
    println!("cargo::rustc-check-cfg=cfg(miri)");
    println!("cargo::rustc-check-cfg=cfg(hashcodecs_memoryview_shim)");
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=src/bindings/memoryview.c");
    #[cfg(feature = "python")]
    {
        pyo3_build_config::use_pyo3_cfgs();
        build_memoryview_shim();
    }

    let is_macos = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|target| target == "macos");
    let is_extension = std::env::var_os("CARGO_FEATURE_EXTENSION_MODULE").is_some();

    if is_macos && is_extension {
        println!("cargo::rustc-cdylib-link-arg=-undefined");
        println!("cargo::rustc-cdylib-link-arg=dynamic_lookup");
    }
}

#[cfg(feature = "python")]
fn build_memoryview_shim() {
    use pyo3_build_config::{GilUsed, PythonAbiKind, PythonImplementation};

    let config = pyo3_build_config::get();
    if config.implementation() != PythonImplementation::CPython
        || !matches!(
            config.target_abi().kind(),
            PythonAbiKind::VersionSpecific(_)
        )
        || matches!(
            config.target_abi().kind(),
            PythonAbiKind::VersionSpecific(GilUsed::FreeThreaded)
        )
        || std::env::var_os("HOST") != std::env::var_os("TARGET")
    {
        return;
    }
    let Some(python) = config.executable() else {
        return;
    };
    let Ok(output) = std::process::Command::new(python)
        .args([
            "-c",
            "import sysconfig; print(sysconfig.get_paths()['include'])",
        ])
        .output()
    else {
        return;
    };
    if !output.status.success() {
        return;
    }
    let Ok(include) = String::from_utf8(output.stdout) else {
        return;
    };
    let include = include.trim();
    if include.is_empty() {
        return;
    }

    let mut shim = cc::Build::new();
    shim.file("src/bindings/memoryview.c")
        .include(include)
        .warnings(true);
    if std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|target| target == "windows") {
        // PyO3 uses raw-dylib imports on Windows; suppress Python.h's automatic
        // import-library directive so the shim follows the same link strategy.
        shim.define("Py_NO_ENABLE_SHARED", None);
        shim.define("Py_NO_LINK_LIB", None);
    }
    shim.compile("hashcodecs_memoryview");
    println!("cargo::rustc-cfg=hashcodecs_memoryview_shim");
}
