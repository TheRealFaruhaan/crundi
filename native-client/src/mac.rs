//! macOS: CEF lives in a framework inside the app bundle and has to be loaded
//! by hand, and Chromium insists that NSApp implements its CrAppProtocol
//! (isHandlingSendEvent / setHandlingSendEvent:). The NSApplication here is
//! winit's own subclass (Slint runs on winit), so the two methods and the
//! protocols are added to that class at runtime, once winit has created it.

use objc2::{
    ffi, msg_send,
    runtime::{AnyClass, AnyObject, Bool, Imp, Sel},
    sel,
};
use std::{
    ffi::CStr,
    sync::atomic::{AtomicBool, Ordering},
};

/// Load the Chromium Embedded Framework from the bundle. `helper` for the
/// helper apps, which sit three levels deeper. Keep the loader alive for the
/// life of the process.
pub fn load_framework(helper: bool) -> cef::library_loader::LibraryLoader {
    let exe = std::env::current_exe().expect("current exe");
    let loader = cef::library_loader::LibraryLoader::new(&exe, helper);
    if !loader.load() {
        eprintln!("[native] could not load the Chromium Embedded Framework (run from the .app bundle: scripts/bundle-mac.sh)");
        std::process::exit(1);
    }
    loader
}

static HANDLING_SEND_EVENT: AtomicBool = AtomicBool::new(false);

unsafe extern "C-unwind" fn is_handling_send_event(_this: *mut AnyObject, _cmd: Sel) -> Bool {
    Bool::new(HANDLING_SEND_EVENT.load(Ordering::Relaxed))
}

unsafe extern "C-unwind" fn set_handling_send_event(_this: *mut AnyObject, _cmd: Sel, value: Bool) {
    HANDLING_SEND_EVENT.store(value.as_bool(), Ordering::Relaxed);
}

/// Make NSApp (winit's NSApplication subclass) conform to CefAppProtocol.
/// Call after Slint has created its window (so NSApp exists) and before
/// cef::initialize.
pub fn adopt_cef_app_protocol() {
    unsafe {
        let ns_app_class = AnyClass::get(c"NSApplication").expect("NSApplication");
        let app: *mut AnyObject = msg_send![ns_app_class, sharedApplication];
        let cls = ffi::object_getClass(app as *const AnyObject) as *mut AnyClass;
        let get: Imp = std::mem::transmute(is_handling_send_event as unsafe extern "C-unwind" fn(*mut AnyObject, Sel) -> Bool);
        let set: Imp = std::mem::transmute(set_handling_send_event as unsafe extern "C-unwind" fn(*mut AnyObject, Sel, Bool));
        // "c@:" returns BOOL (signed char); "v@:c" takes one.
        ffi::class_addMethod(cls, sel!(isHandlingSendEvent), get, c"c@:".as_ptr());
        ffi::class_addMethod(cls, sel!(setHandlingSendEvent:), set, c"v@:c".as_ptr());
        for name in [c"CrAppProtocol", c"CrAppControlProtocol", c"CefAppProtocol"] {
            let p = ffi::objc_getProtocol(name.as_ptr());
            if !p.is_null() {
                ffi::class_addProtocol(cls, p);
            }
        }
        let class_name = CStr::from_ptr(ffi::object_getClassName(app as *const AnyObject));
        eprintln!("[native] NSApp ({}) adopted CefAppProtocol", class_name.to_string_lossy());
    }
}

// ─── CEF's message loop on macOS ───
//
// Elsewhere a Slint timer calls do_message_loop_work. On macOS that timer runs
// inside winit's event handler, and CEF's work spins the run loop itself, which
// hands winit an event while it is still handling one: winit aborts ("tried to
// handle event while another event is currently being handled"). So here the
// pump is a CFRunLoop timer: it fires from the run loop directly, outside
// winit's handler, and never re-enters itself.

use std::ffi::c_void;

#[repr(C)]
struct CFRunLoopTimerContext {
    version: isize,
    info: *mut c_void,
    retain: *const c_void,
    release: *const c_void,
    copy_description: *const c_void,
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRunLoopGetMain() -> *mut c_void;
    fn CFAbsoluteTimeGetCurrent() -> f64;
    fn CFRunLoopTimerCreate(
        allocator: *const c_void,
        fire_date: f64,
        interval: f64,
        flags: usize,
        order: isize,
        callout: extern "C" fn(*mut c_void, *mut c_void),
        context: *mut CFRunLoopTimerContext,
    ) -> *mut c_void;
    fn CFRunLoopAddTimer(run_loop: *mut c_void, timer: *mut c_void, mode: *const c_void);
    static kCFRunLoopCommonModes: *const c_void;
}

static PUMPING: AtomicBool = AtomicBool::new(false);

extern "C" fn pump(_timer: *mut c_void, _info: *mut c_void) {
    if PUMPING.swap(true, Ordering::Acquire) {
        return;
    }
    crate::browser::run_deferred();
    cef::do_message_loop_work();
    PUMPING.store(false, Ordering::Release);
}

/// Pump CEF every [interval_ms] from the main run loop (in every run-loop mode,
/// so it keeps going during window drags and menus).
pub fn start_pump(interval_ms: u64) {
    let secs = interval_ms as f64 / 1000.0;
    unsafe {
        let timer = CFRunLoopTimerCreate(std::ptr::null(), CFAbsoluteTimeGetCurrent() + secs, secs, 0, 0, pump, std::ptr::null_mut());
        CFRunLoopAddTimer(CFRunLoopGetMain(), timer, kCFRunLoopCommonModes);
    }
}
