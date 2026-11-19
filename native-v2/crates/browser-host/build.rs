fn main() {
    // CEF's library sits next to the binary (stage-cef): find it there.
    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-arg-bins=-Wl,-rpath,$ORIGIN");
}
