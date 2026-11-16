fn main() {
    // ComponentContainer is experimental in Slint 1.18.
    unsafe { std::env::set_var("SLINT_ENABLE_EXPERIMENTAL_FEATURES", "1"); }
    let config = slint_build::CompilerConfiguration::new().with_style("fluent-dark".into());
    slint_build::compile_with_config("ui/host.slint", config).expect("Slint UI");
}
