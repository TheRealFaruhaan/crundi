fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent-dark".into());
    slint_build::compile_with_config("ui/main.slint", config).expect("Slint UI");
    // libcef and its resources sit next to the binary (scripts/stage-cef.sh), so
    // the dynamic linker looks there first, on Linux.
    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-arg-bins=-Wl,-rpath,$ORIGIN");
}
