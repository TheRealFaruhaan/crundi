//! The browser host's loop: `crundi-browser-host --browser-host`, started by
//! the app (browser-link's proc.rs). It runs CEF and every web view, and nothing else: no window,
//! no GPUI. The app talks to it over its link (ipc.rs); frames go through
//! shared memory (shm.rs).
//!
//! Its life: it waits for `Hello` (on Windows the app puts it in its job
//! object first), starts CEF, says `Ready`, then pumps CEF and the app's
//! commands on its main thread until `Quit` (views closed, CEF shut down)
//! or until the link closes (the app is gone: it leaves at once, and CEF's
//! helpers follow it). On Linux it also dies with the app (PDEATHSIG); on
//! macOS a watchdog leaves when the app is gone (proc.rs `guard_parent`).

use crate::views;
use crate::ipc::{self, Cmd, Ev, Fd};
use std::{collections::VecDeque, sync::{mpsc, Mutex, OnceLock}, time::{Duration, Instant}};

static OUT: OnceLock<Mutex<mpsc::Sender<(Vec<u8>, Option<Fd>)>>> = OnceLock::new();

/// An event to the app.
pub(crate) fn emit(ev: &Ev) { emit_fd(ev, None) }

/// An event with an fd passed alongside (Linux: a frame segment's memfd).
pub(crate) fn emit_fd(ev: &Ev, fd: Option<Fd>) {
    if let Some(o) = OUT.get() { let _ = o.lock().unwrap().send((ipc::encode(ev), fd)); }
}

/// The host's main: never returns.
pub fn run() -> ! {
    crate::proc::guard_parent();
    let (mut send, mut recv) = match crate::proc::host_link() {
        Ok(l) => l,
        Err(e) => { eprintln!("[browser host] no link to the app: {e}"); std::process::exit(1) }
    };
    let (otx, orx) = mpsc::channel::<(Vec<u8>, Option<Fd>)>();
    let _ = OUT.set(Mutex::new(otx));
    std::thread::spawn(move || {
        for (b, fd) in orx { if send(&b, fd).is_err() { std::process::exit(0); } }
    });
    let (ctx, crx) = mpsc::channel::<Cmd>();
    std::thread::spawn(move || {
        let mut d = ipc::Decoder::default();
        let mut buf = vec![0u8; 64 * 1024];
        let mut fds = VecDeque::new();
        'read: loop {
            match recv(&mut buf, &mut fds) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    d.push(&buf[..n]);
                    while let Some(m) = d.next::<Cmd>() {
                        match m { Ok(c) => if ctx.send(c).is_err() { break 'read; }, Err(e) => { eprintln!("[browser host] a bad message: {e}"); break 'read; } }
                    }
                }
            }
        }
        // The app is gone (or broke the link): leave now; CEF's helpers
        // see the host go and leave too.
        std::process::exit(0);
    });
    match crx.recv_timeout(Duration::from_secs(30)) {
        Ok(Cmd::Hello { textures }) => views::set_textures(textures),
        _ => std::process::exit(0),
    }
    if let Err(why) = start_cef() {
        emit(&Ev::Failed { why });
        std::thread::sleep(Duration::from_millis(300));
        std::process::exit(1);
    }
    emit(&Ev::Ready { pid: std::process::id() });
    let mode = crate::gpu::mode();
    if crate::gpu::asked() != mode { eprintln!("[browser host] CRUNDI_BROWSER_GPU={}: not on this OS, the CPU path instead", crate::gpu::name(crate::gpu::asked())); }
    let mut stats_at = Instant::now();
    loop {
        if stats_at.elapsed() >= Duration::from_secs(2) { stats_at = Instant::now(); views::send_stats(); }
        match crx.recv_timeout(Duration::from_millis(4)) {
            Ok(c) => {
                if !views::handle(c) { quit(); }
                while let Ok(c) = crx.try_recv() { if !views::handle(c) { quit(); } }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => std::process::exit(0),
        }
        cef::do_message_loop_work();
    }
}

/// Asked to stop: every view closed, CEF shut down once they have gone
/// (else just out: shutting CEF down with a browser alive aborts).
fn quit() -> ! {
    views::close_all();
    let end = Instant::now() + Duration::from_millis(1500);
    while views::live() > 0 && Instant::now() < end {
        cef::do_message_loop_work();
        std::thread::sleep(Duration::from_millis(5));
    }
    if views::live() == 0 { cef::shutdown(); }
    std::process::exit(0);
}

fn start_cef() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    crate::mac::host_app();
    let args = cef::args::Args::new();
    let mut app = crate::cef_app::NativeApp::new();
    // CEF's files sit next to the binary (native-client/scripts/stage-cef.mjs).
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
    let cache = crate::common::cef_root();
    let _ = std::fs::create_dir_all(&cache);
    let mut settings = cef::Settings {
        no_sandbox: 1,
        windowless_rendering_enabled: 1,
        multi_threaded_message_loop: 0,
        external_message_pump: 0,
        root_cache_path: cef::CefString::from(cache.to_string_lossy().as_ref()),
        log_severity: cef::LogSeverity::WARNING,
        ..Default::default()
    };
    // macOS: the host is not the bundle's main executable, so CEF is told
    // where its helper is (the bundle's "<app> Helper.app", a copy of this
    // executable: stage-cef.mjs STAGE_HOST); it finds the GPU, Renderer…
    // variants beside it.
    #[cfg(target_os = "macos")]
    if let Some(helper) = mac_helper(&exe_dir) { settings.browser_subprocess_path = cef::CefString::from(helper.to_string_lossy().as_ref()); }
    // On macOS the framework finds its own resources; elsewhere they sit next to the binary.
    if !cfg!(target_os = "macos") {
        settings.resources_dir_path = cef::CefString::from(exe_dir.to_string_lossy().as_ref());
        settings.locales_dir_path = cef::CefString::from(exe_dir.join("locales").to_string_lossy().as_ref());
    }
    if cef::initialize(Some(args.as_main_args()), Some(&settings), Some(&mut app), std::ptr::null_mut()) == 1 { Ok(()) } else { Err("CEF did not start".into()) }
}

/// The bundle's base helper executable: Contents/Frameworks/<name> Helper.app/Contents/MacOS/<name> Helper.
#[cfg(target_os = "macos")]
fn mac_helper(exe_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let fw = exe_dir.parent()?.join("Frameworks");
    std::fs::read_dir(&fw).ok()?.flatten().find_map(|e| {
        let n = e.file_name().to_string_lossy().to_string();
        let stem = n.strip_suffix(" Helper.app")?.to_string();
        let exe = e.path().join("Contents").join("MacOS").join(format!("{stem} Helper"));
        exe.exists().then_some(exe)
    })
}
