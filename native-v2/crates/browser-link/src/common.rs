//! What the app and the browser host share that needs no CEF: the devices a
//! page can emulate, the cache folders, the clock.

/// A device the panel can emulate. Index 0 is "Responsive" (no emulation: the
/// page is the panel's own size).
pub struct Device {
    pub name: &'static str,
    pub w: i32,
    pub h: i32,
    pub dpr: f32,
    pub mobile: bool,
    pub ua: &'static str,
}

/// The drawer's devices (app/vendor/crundi-browser-drawer.js DEVICES), in its order.
pub const DEVICES: &[Device] = &[
    Device { name: "Responsive", w: 0, h: 0, dpr: 1.0, mobile: false, ua: "" },
    Device { name: "iPhone SE", w: 375, h: 667, dpr: 2.0, mobile: true, ua: "Mozilla/5.0 (iPhone; CPU iPhone OS 16_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.0 Mobile/15E148 Safari/604.1" },
    Device { name: "iPhone 12 Pro", w: 390, h: 844, dpr: 3.0, mobile: true, ua: "Mozilla/5.0 (iPhone; CPU iPhone OS 16_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.0 Mobile/15E148 Safari/604.1" },
    Device { name: "iPhone 15 Pro", w: 393, h: 852, dpr: 3.0, mobile: true, ua: "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1" },
    Device { name: "iPhone 16 Pro", w: 393, h: 852, dpr: 3.0, mobile: true, ua: "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1" },
    Device { name: "iPhone 16 Pro Max", w: 430, h: 932, dpr: 3.0, mobile: true, ua: "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1" },
    Device { name: "iPad Mini", w: 768, h: 1024, dpr: 2.0, mobile: true, ua: "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1" },
    Device { name: "iPad Air", w: 820, h: 1180, dpr: 2.0, mobile: true, ua: "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1" },
    Device { name: "iPad Pro", w: 1024, h: 1366, dpr: 2.0, mobile: true, ua: "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1" },
    Device { name: "Pixel 5", w: 393, h: 851, dpr: 2.75, mobile: true, ua: "Mozilla/5.0 (Linux; Android 11; Pixel 5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/90.0.4430.91 Mobile Safari/537.36" },
    Device { name: "Pixel 7", w: 412, h: 915, dpr: 2.625, mobile: true, ua: "Mozilla/5.0 (Linux; Android 13; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36" },
    Device { name: "Pixel 8 Pro", w: 430, h: 932, dpr: 2.625, mobile: true, ua: "Mozilla/5.0 (Linux; Android 14; Pixel 8 Pro) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36" },
    Device { name: "Galaxy S22", w: 360, h: 800, dpr: 3.0, mobile: true, ua: "Mozilla/5.0 (Linux; Android 13; SM-S901B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36" },
    Device { name: "Galaxy S24 Ultra", w: 412, h: 915, dpr: 2.625, mobile: true, ua: "Mozilla/5.0 (Linux; Android 14; SM-S928B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36" },
    Device { name: "Galaxy Z Fold 5", w: 344, h: 882, dpr: 2.8, mobile: true, ua: "Mozilla/5.0 (Linux; Android 13; SM-F946B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/114.0.0.0 Mobile Safari/537.36" },
    Device { name: "Galaxy Z Flip 5", w: 264, h: 844, dpr: 2.8, mobile: true, ua: "Mozilla/5.0 (Linux; Android 13; SM-F731B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/114.0.0.0 Mobile Safari/537.36" },
    Device { name: "Surface Pro 9", w: 1200, h: 1800, dpr: 2.0, mobile: true, ua: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36" },
    Device { name: "Desktop 1280", w: 1280, h: 800, dpr: 1.0, mobile: false, ua: "" },
    Device { name: "Full HD 1920", w: 1920, h: 1080, dpr: 1.0, mobile: false, ua: "" },
];

impl Device {
    /// The menu's label: "Responsive", or "iPhone SE  375x667".
    pub fn label(&self) -> String { if self.w == 0 { self.name.to_string() } else { format!("{}  {}x{}", self.name, self.w, self.h) } }
}

/// The CEF profile folder of a network ("" for this PC's: CEF's global one).
pub fn profile_dir(network: &str) -> String {
    match network {
        "client" | "" => String::new(),
        "server" => "drawer-srv".into(),
        n if n.starts_with("mem:") => String::new(),
        n => format!("drawer-hp-{}", n.trim_start_matches("hp:")),
    }
}

/// CEF's root cache folder (a network's profile must be inside it);
/// CRUNDI_CEF_CACHE gives each client running at once a profile of its own.
pub fn cef_root() -> std::path::PathBuf {
    if let Some(c) = std::env::var_os("CRUNDI_CEF_CACHE") { return c.into(); }
    let cache: std::path::PathBuf = std::env::var_os("XDG_CACHE_HOME").map(Into::into)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(Into::into))
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    cache.join("crundi-v2")
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// CEF's cursor kinds folded into the ones a view shows (the host sends
/// these numbers; view.rs `cursor_style` maps them to GPUI's).
pub mod cursor {
    pub const ARROW: i32 = 0;
    pub const HAND: i32 = 1;
    pub const IBEAM: i32 = 2;
    pub const CROSS: i32 = 3;
    pub const MOVE: i32 = 4;
    pub const WAIT: i32 = 5;
    pub const NOT_ALLOWED: i32 = 6;
    pub const EW: i32 = 7;
    pub const NS: i32 = 8;
    pub const NESW: i32 = 9;
    pub const NWSE: i32 = 10;
    pub const GRAB: i32 = 11;
    pub const GRABBING: i32 = 12;
    pub const HELP: i32 = 13;
    pub const NONE: i32 = 14;
}
