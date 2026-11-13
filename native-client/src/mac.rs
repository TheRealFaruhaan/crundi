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
