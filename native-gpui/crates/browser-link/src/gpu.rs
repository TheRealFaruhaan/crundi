//! The GPU path: Chromium draws on the GPU and hands each frame over as a
//! shared texture (CEF's accelerated off-screen rendering:
//! `shared_texture_enabled`, `OnAcceleratedPaint`) instead of a CPU buffer.
//!
//! Off by default; `CRUNDI_BROWSER_GPU=1` turns it on (Windows and macOS;
//! Linux stays on the CPU path: shared memory). GPUI 0.3.12 cannot draw a
//! texture that another process (or another device) made (README, "GPU
//! textures"), so the frame still reaches the app through the view's
//! shared-memory segment: the host reads the texture back into it right in
//! the callback (one GPU-to-CPU copy, in place of CEF's software
//! compositor's copy). What the GPU buys now is Chromium's own GPU raster
//! and compositing (WebGL, video, CSS effects on the GPU).
//!
//! `CRUNDI_BROWSER_GPU=texture` (Windows) also hands each texture to the
//! app: the host duplicates CEF's NT handle into the app's process
//! (`Ev::Texture`), the app opens it on its own D3D11 device
//! (`OpenSharedResource1`), checks it, counts it (the driver's
//! `browserHost.gpu`) and closes it. That is the whole path a renderer able
//! to take an external texture would use; the picture is still the
//! segment's.
//!
//! - Windows: CEF's texture is a DXGI NT handle valid in the callback;
//!   the host opens it on a D3D11 device of its own, copies it into a
//!   staging texture and maps that (`Reader`).
//! - macOS: CEF's texture is an IOSurface (BGRA); the host locks it read
//!   only and copies its rows (`Reader`). Handing it to the app would need
//!   its mach port (IOSurfaceCreateMachPort) over a bootstrap service; not
//!   done while GPUI can't draw a BGRA surface (its `surface()` takes only
//!   NV12 CVPixelBuffers).

/// What CRUNDI_BROWSER_GPU asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// The CPU path (software compositing, frames to shared memory).
    Off,
    /// Chromium on the GPU, frames read back into shared memory.
    Gpu,
    /// As `Gpu`, and each texture handed to the app too (Windows).
    Texture,
}

/// CRUNDI_BROWSER_GPU as asked (whatever the OS).
pub fn asked() -> Mode {
    match std::env::var("CRUNDI_BROWSER_GPU").unwrap_or_default().to_ascii_lowercase().as_str() {
        "1" | "on" | "true" | "gpu" => Mode::Gpu,
        "texture" | "textures" => Mode::Texture,
        _ => Mode::Off,
    }
}

/// The mode this run uses: `Off` on Linux whatever was asked; `Texture`
/// only on Windows.
pub fn mode() -> Mode {
    match asked() {
        Mode::Off => Mode::Off,
        _ if !cfg!(any(windows, target_os = "macos")) => Mode::Off,
        Mode::Texture if cfg!(windows) => Mode::Texture,
        _ => Mode::Gpu,
    }
}

pub fn name(m: Mode) -> &'static str {
    match m { Mode::Off => "off", Mode::Gpu => "gpu", Mode::Texture => "texture" }
}

/// Copy `h` rows of `row` bytes from `src` (rows `pitch` apart) into `dst`
/// (packed).
pub fn copy_rows(src: *const u8, pitch: usize, row: usize, h: usize, dst: &mut [u8]) {
    if dst.len() < row * h || src.is_null() { return; }
    for y in 0..h {
        let s = unsafe { std::slice::from_raw_parts(src.add(y * pitch), row) };
        dst[y * row..(y + 1) * row].copy_from_slice(s);
    }
}

#[cfg(windows)]
pub use win::{duplicate_into, AppTextures, Reader};

#[cfg(target_os = "macos")]
pub use mac::Reader;

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use windows::core::Interface;
    use windows::Win32::Foundation::{HANDLE, HMODULE};
    use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1};
    use windows::Win32::Graphics::Direct3D11::*;

    fn device() -> Result<(ID3D11Device1, ID3D11DeviceContext), String> {
        let mut dev: Option<ID3D11Device> = None;
        let mut ctx: Option<ID3D11DeviceContext> = None;
        unsafe {
            D3D11CreateDevice(
                None, D3D_DRIVER_TYPE_HARDWARE, HMODULE::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_1]),
                D3D11_SDK_VERSION, Some(&mut dev), None, Some(&mut ctx),
            ).map_err(|e| format!("no D3D11 device: {e}"))?;
        }
        let dev = dev.ok_or("no D3D11 device")?;
        let ctx = ctx.ok_or("no D3D11 context")?;
        let dev1: ID3D11Device1 = dev.cast().map_err(|e| format!("no ID3D11Device1: {e}"))?;
        Ok((dev1, ctx))
    }

    fn desc(t: &ID3D11Texture2D) -> D3D11_TEXTURE2D_DESC {
        let mut d = D3D11_TEXTURE2D_DESC::default();
        unsafe { t.GetDesc(&mut d) };
        d
    }

    /// The host's reader of CEF's shared textures: a D3D11 device and a
    /// staging texture kept per size.
    pub struct Reader {
        dev: Option<(ID3D11Device1, ID3D11DeviceContext)>,
        staging: Option<(ID3D11Texture2D, u32, u32)>,
        pub error: String,
    }

    impl Default for Reader { fn default() -> Self { Self::new() } }

    impl Reader {
        pub fn new() -> Reader { Reader { dev: None, staging: None, error: String::new() } }

        fn open(&mut self, handle: *mut c_void) -> Result<ID3D11Texture2D, String> {
            if self.dev.is_none() { self.dev = Some(device()?); }
            let (dev, _) = self.dev.as_ref().unwrap();
            unsafe { dev.OpenSharedResource1::<ID3D11Texture2D>(HANDLE(handle)) }.map_err(|e| format!("OpenSharedResource1: {e}"))
        }

        /// The frame's size, then its pixels (BGRA, packed) into what
        /// `place(w, h)` gives (None: skip it). Only inside CEF's callback:
        /// the handle is CEF's, valid until it returns.
        ///
        /// # Safety
        /// `handle` is CEF's shared texture handle for this callback.
        pub unsafe fn read(&mut self, handle: *mut c_void, place: &mut dyn FnMut(u32, u32) -> Option<*mut [u8]>) -> Result<(u32, u32), String> {
            let tex = self.open(handle)?;
            let d = desc(&tex);
            let (w, h) = (d.Width, d.Height);
            let Some(dst) = place(w, h) else { return Ok((w, h)) };
            let (dev, ctx) = self.dev.as_ref().unwrap();
            if self.staging.as_ref().is_none_or(|s| s.1 != w || s.2 != h) {
                let sd = D3D11_TEXTURE2D_DESC {
                    Width: w, Height: h, MipLevels: 1, ArraySize: 1, Format: d.Format,
                    SampleDesc: d.SampleDesc, Usage: D3D11_USAGE_STAGING, BindFlags: 0,
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32, MiscFlags: 0,
                };
                let mut st: Option<ID3D11Texture2D> = None;
                unsafe { dev.CreateTexture2D(&sd, None, Some(&mut st)) }.map_err(|e| format!("staging texture: {e}"))?;
                self.staging = Some((st.ok_or("no staging texture")?, w, h));
            }
            let st = &self.staging.as_ref().unwrap().0;
            unsafe {
                ctx.CopyResource(st, &tex);
                let mut m = D3D11_MAPPED_SUBRESOURCE::default();
                ctx.Map(st, 0, D3D11_MAP_READ, 0, Some(&mut m)).map_err(|e| format!("Map: {e}"))?;
                let dst = &mut *dst;
                super::copy_rows(m.pData as *const u8, m.RowPitch as usize, w as usize * 4, h as usize, dst);
                ctx.Unmap(st, 0);
            }
            Ok((w, h))
        }
    }

    /// CEF's handle duplicated into process `pid` (the app): the value it
    /// has there. The app closes it.
    ///
    /// # Safety
    /// `handle` is a live handle of this process.
    pub unsafe fn duplicate_into(pid: u32, handle: *mut c_void) -> Result<u64, String> {
        use windows_sys::Win32::{Foundation::{CloseHandle, DuplicateHandle, DUPLICATE_SAME_ACCESS}, System::Threading::{GetCurrentProcess, OpenProcess, PROCESS_DUP_HANDLE}};
        unsafe {
            let target = OpenProcess(PROCESS_DUP_HANDLE, 0, pid);
            if target.is_null() { return Err(format!("OpenProcess: {}", std::io::Error::last_os_error())); }
            let mut out: windows_sys::Win32::Foundation::HANDLE = std::ptr::null_mut();
            let ok = DuplicateHandle(GetCurrentProcess(), handle as _, target, &mut out, 0, 0, DUPLICATE_SAME_ACCESS);
            let e = std::io::Error::last_os_error();
            CloseHandle(target);
            if ok == 0 { return Err(format!("DuplicateHandle: {e}")); }
            Ok(out as usize as u64)
        }
    }

    /// The app's side: a texture the host handed over, opened on the app's
    /// own D3D11 device (as a renderer able to draw it would), its size
    /// read, the handle closed.
    #[derive(Default)]
    pub struct AppTextures {
        dev: Option<(ID3D11Device1, ID3D11DeviceContext)>,
    }

    impl AppTextures {
        /// # Safety
        /// `handle` is a handle the host duplicated into this process; it is closed here.
        pub unsafe fn open(&mut self, handle: u64) -> Result<(u32, u32), String> {
            let h = handle as usize as *mut c_void;
            let r = (|| {
                if self.dev.is_none() { self.dev = Some(device()?); }
                let (dev, _) = self.dev.as_ref().unwrap();
                let tex: ID3D11Texture2D = unsafe { dev.OpenSharedResource1(HANDLE(h)) }.map_err(|e| format!("OpenSharedResource1: {e}"))?;
                let d = desc(&tex);
                Ok((d.Width, d.Height))
            })();
            unsafe { windows_sys::Win32::Foundation::CloseHandle(h as _); }
            r
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;

    #[link(name = "IOSurface", kind = "framework")]
    unsafe extern "C" {
        fn IOSurfaceGetWidth(s: *const c_void) -> usize;
        fn IOSurfaceGetHeight(s: *const c_void) -> usize;
        fn IOSurfaceGetBytesPerRow(s: *const c_void) -> usize;
        fn IOSurfaceGetBaseAddress(s: *const c_void) -> *mut c_void;
        fn IOSurfaceLock(s: *const c_void, options: u32, seed: *mut u32) -> i32;
        fn IOSurfaceUnlock(s: *const c_void, options: u32, seed: *mut u32) -> i32;
    }
    const READ_ONLY: u32 = 1;

    /// The host's reader of CEF's IOSurfaces (BGRA).
    #[derive(Default)]
    pub struct Reader { pub error: String }

    impl Reader {
        pub fn new() -> Reader { Reader::default() }

        /// As Windows' `Reader::read`.
        ///
        /// # Safety
        /// `surface` is CEF's IOSurfaceRef for this callback.
        pub unsafe fn read(&mut self, surface: *mut c_void, place: &mut dyn FnMut(u32, u32) -> Option<*mut [u8]>) -> Result<(u32, u32), String> {
            if surface.is_null() { return Err("no IOSurface".into()); }
            unsafe {
                let (w, h) = (IOSurfaceGetWidth(surface) as u32, IOSurfaceGetHeight(surface) as u32);
                let Some(dst) = place(w, h) else { return Ok((w, h)) };
                let mut seed = 0u32;
                if IOSurfaceLock(surface, READ_ONLY, &mut seed) != 0 { return Err("IOSurfaceLock failed".into()); }
                let base = IOSurfaceGetBaseAddress(surface) as *const u8;
                super::copy_rows(base, IOSurfaceGetBytesPerRow(surface), w as usize * 4, h as usize, &mut *dst);
                IOSurfaceUnlock(surface, READ_ONLY, &mut seed);
                Ok((w, h))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_packed() {
        // Two rows of 2 px (8 bytes) in rows 12 bytes apart.
        let src: Vec<u8> = (0..24).collect();
        let mut dst = vec![0u8; 16];
        copy_rows(src.as_ptr(), 12, 8, 2, &mut dst);
        assert_eq!(&dst[..8], &src[..8]);
        assert_eq!(&dst[8..], &src[12..20]);
    }

    #[test]
    fn linux_stays_on_the_cpu_path() {
        if cfg!(target_os = "linux") { assert_eq!(mode(), Mode::Off); }
    }
}
