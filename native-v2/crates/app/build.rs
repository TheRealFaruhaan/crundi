fn main() {
    // The app links no CEF (only crundi-browser-host does, crates/browser-host).
    // Its own folder stays on its library path, for what is staged beside it.
    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-arg-bins=-Wl,-rpath,$ORIGIN");
    // Crundi's icon on the .exe (the taskbar, Explorer, Alt+Tab).
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/icon.ico");
        let mut r = winresource::WindowsResource::new();
        r.set_icon("assets/icon.ico");
        if let Err(e) = r.compile() { println!("cargo:warning=no icon on the .exe: {e}"); }
    }
}
