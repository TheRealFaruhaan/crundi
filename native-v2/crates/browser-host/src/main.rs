//! The browser host's executable, `crundi-browser-host`: Chromium (CEF) and
//! every web view of the app, in a process of its own (README, "The browser
//! host"). The app (crates/webview) starts it with `--browser-host` when a
//! page is first needed; CEF starts it again for each of its helper
//! processes (`--type=…`), which run and leave in `main`.
//!
//! Only this executable links libcef: the app never maps it.
//!
//! `cef::*` and the link's types share names (Rect, Cursor…): CEF's code is
//! in `host`, `views`, `mac`.

mod host;
#[cfg(target_os = "macos")]
mod mac;
mod views;

pub(crate) use crundi_browser_link::{common, gpu, ipc, proc, shm};

mod cef_app {
    use cef::*;
    wrap_app! {
        pub struct NativeApp;
        impl App {
            fn on_before_command_line_processing(&self, process_type: Option<&CefString>, command_line: Option<&mut CommandLine>) {
                if process_type.map(|t| !t.to_string().is_empty()).unwrap_or(false) { return; }
                let Some(cl) = command_line else { return };
                // The CPU path by default (the Slint client's): software
                // compositing, frames to on_paint as buffers. The GPU path
                // (gpu.rs, CRUNDI_BROWSER_GPU) keeps the GPU: frames come to
                // on_accelerated_paint as shared textures.
                let gpu = crate::gpu::mode() != crate::gpu::Mode::Off;
                let switches: &[&str] = if gpu { &["enable-begin-frame-scheduling"] } else { &["disable-gpu", "disable-gpu-compositing", "enable-begin-frame-scheduling"] };
                for s in switches { cl.append_switch(Some(&CefString::from(*s))); }
                // Under the test driver no keyring: cookies' key in the
                // profile itself. A suite's private session bus has no
                // secret service, and the first page that needs cookies
                // waited 25 s and more on the portal's RetrieveSecret
                // (FINDINGS-ELECTRON 2); nor is the person's keyring read.
                if std::env::var_os("CRUNDI_TEST_PORT").is_some() {
                    cl.append_switch_with_value(Some(&CefString::from("password-store")), Some(&CefString::from("basic")));
                }
            }
        }
    }
}

fn main() {
    let host = std::env::args().any(|a| a == proc::FLAG);
    let helper = std::env::args().any(|a| a.starts_with("--type="));
    #[cfg(target_os = "macos")]
    {
        let loader = mac::load_framework(helper);
        std::mem::forget(loader);
    }
    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
    let args = cef::args::Args::new();
    let mut app = cef_app::NativeApp::new();
    let code = cef::execute_process(Some(args.as_main_args()), Some(&mut app), std::ptr::null_mut());
    if code >= 0 { std::process::exit(code); }
    if host { host::run(); }
    if !helper { eprintln!("crundi-browser-host: the app starts this (with {}); it waits for the app's Hello on its link", proc::FLAG); }
    std::process::exit(2);
}
