fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|target| target == "macos") {
        println!("cargo::rustc-cdylib-link-arg=-undefined");
        println!("cargo::rustc-cdylib-link-arg=dynamic_lookup");
    }
}
