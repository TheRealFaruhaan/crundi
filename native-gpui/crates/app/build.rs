fn main() {
    // The app links no CEF (only crundi-browser-host does, crates/browser-host).
    // Its own folder stays on its library path, for what is staged beside it.
    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-arg-bins=-Wl,-rpath,$ORIGIN");
}
