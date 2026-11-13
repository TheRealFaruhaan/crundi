//! CEF's message loop, run from the main thread's own loop.
//!
//! CEF's work spins the OS event loop itself. When that happens inside
//! winit's event handling, winit is re-entered: on macOS it aborts, and on
//! Windows it stalls. So on those two the pump fires straight from the OS loop,
//! outside winit's handler (a CFRunLoop timer; a Win32 thread timer), and calls
//! into CEF from Slint callbacks wait for it (browser::defer). On Linux a Slint
//! timer is fine.

use std::sync::atomic::{AtomicBool, Ordering};

static PUMPING: AtomicBool = AtomicBool::new(false);

/// One round: the deferred calls, then CEF's own work. Never re-entered.
pub fn pump_once() {
    if PUMPING.swap(true, Ordering::Acquire) {
        return;
    }
    crate::browser::run_deferred();
    cef::do_message_loop_work();
    PUMPING.store(false, Ordering::Release);
}

/// Start pumping every [ms]. Keep what it returns alive (Linux: the Slint timer).
pub fn start(ms: u64) -> Option<slint::Timer> {
    #[cfg(target_os = "macos")]
    {
        crate::mac::start_run_loop_timer(ms);
        None
    }
    #[cfg(target_os = "windows")]
    {
        win::start(ms);
        None
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let t = slint::Timer::default();
        t.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(ms), pump_once);
        Some(t)
    }
}

#[cfg(target_os = "windows")]
mod win {
    use std::ffi::c_void;

    #[link(name = "user32")]
    extern "system" {
        fn SetTimer(hwnd: *mut c_void, id: usize, elapse: u32, timer_proc: Option<unsafe extern "system" fn(*mut c_void, u32, usize, u32)>) -> usize;
    }

    unsafe extern "system" fn tick(_hwnd: *mut c_void, _msg: u32, _id: usize, _time: u32) {
        super::pump_once();
    }

    /// A thread timer: DispatchMessage calls [tick] directly (also inside
    /// Windows' own modal loops, such as a window being dragged).
    pub fn start(ms: u64) {
        unsafe { SetTimer(std::ptr::null_mut(), 0, ms as u32, Some(tick)) };
    }
}
