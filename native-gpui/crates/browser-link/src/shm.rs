//! A view's frame segment: shared memory the host writes a BGRA frame into
//! and the app reads it out of (ipc.rs: `Shm`, `Paint`, `Ack`).
//!
//! - Linux: a memfd (never in /dev/shm), its fd passed over the socket;
//!   gone once both sides unmap it.
//! - macOS: shm_open by a short name; the app unlinks the name as soon as
//!   it has mapped it, the host when it lets the segment go.
//! - Windows: a named file mapping; gone once both sides close it.
//!
//! A host that dies takes its side with it; the app lets go of its side
//! when it sees the link close.

use crate::ipc::Fd;
use std::io;

pub struct Segment {
    ptr: *mut u8,
    cap: usize,
    #[cfg(target_os = "macos")]
    name: Option<std::ffi::CString>,
    #[cfg(windows)]
    handle: windows_sys::Win32::Foundation::HANDLE,
}

// The mapping is plain memory; each side uses it from one thread at a time.
unsafe impl Send for Segment {}

impl Segment {
    pub fn cap(&self) -> usize { self.cap }
    pub fn bytes(&self) -> &[u8] { unsafe { std::slice::from_raw_parts(self.ptr, self.cap) } }
    pub fn bytes_mut(&mut self) -> &mut [u8] { unsafe { std::slice::from_raw_parts_mut(self.ptr, self.cap) } }
}

// ─── Linux ───

#[cfg(target_os = "linux")]
impl Segment {
    /// The host's side: a new segment of `cap` bytes for view `id` (its
    /// `epoch`th), and how the app opens it (the fd to pass).
    pub fn create(cap: usize, _id: u64, _gen: u32) -> io::Result<(Segment, Option<Fd>, String)> {
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        let cap = cap.max(1);
        unsafe {
            let fd = libc::memfd_create(c"crundi-frame".as_ptr(), libc::MFD_CLOEXEC);
            if fd < 0 { return Err(io::Error::last_os_error()); }
            let fd = OwnedFd::from_raw_fd(fd);
            if libc::ftruncate(fd.as_raw_fd(), cap as libc::off_t) != 0 { return Err(io::Error::last_os_error()); }
            let ptr = map(fd.as_raw_fd(), cap, true)?;
            Ok((Segment { ptr, cap }, Some(fd), String::new()))
        }
    }

    /// The app's side: map the segment the host made (read only).
    pub fn open(cap: usize, fd: Option<Fd>, _name: &str) -> io::Result<Segment> {
        use std::os::fd::AsRawFd;
        let fd = fd.ok_or_else(|| io::Error::other("no fd came with the segment"))?;
        let ptr = unsafe { map(fd.as_raw_fd(), cap.max(1), false)? };
        Ok(Segment { ptr, cap: cap.max(1) })
    }
}

#[cfg(unix)]
unsafe fn map(fd: i32, cap: usize, write: bool) -> io::Result<*mut u8> {
    let prot = if write { libc::PROT_READ | libc::PROT_WRITE } else { libc::PROT_READ };
    let p = unsafe { libc::mmap(std::ptr::null_mut(), cap, prot, libc::MAP_SHARED, fd, 0) };
    if p == libc::MAP_FAILED { return Err(io::Error::last_os_error()); }
    Ok(p as *mut u8)
}

#[cfg(unix)]
impl Drop for Segment {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.ptr as *mut libc::c_void, self.cap); }
        #[cfg(target_os = "macos")]
        if let Some(n) = &self.name { unsafe { libc::shm_unlink(n.as_ptr()); } }
    }
}

// ─── macOS ───

#[cfg(target_os = "macos")]
impl Segment {
    pub fn create(cap: usize, id: u64, epoch: u32) -> io::Result<(Segment, Option<Fd>, String)> {
        // At most 31 characters (PSHMNAMLEN).
        let name = format!("/cr{:x}-{:x}-{:x}", std::process::id(), id, epoch);
        let c = std::ffi::CString::new(name.clone()).map_err(io::Error::other)?;
        let cap = cap.max(1);
        unsafe {
            libc::shm_unlink(c.as_ptr());
            let fd = libc::shm_open(c.as_ptr(), libc::O_CREAT | libc::O_EXCL | libc::O_RDWR, 0o600 as libc::c_uint);
            if fd < 0 { return Err(io::Error::last_os_error()); }
            let r = if libc::ftruncate(fd, cap as libc::off_t) != 0 { Err(io::Error::last_os_error()) } else { map(fd, cap, true) };
            libc::close(fd);
            match r {
                Ok(ptr) => Ok((Segment { ptr, cap, name: Some(c) }, None, name)),
                Err(e) => { libc::shm_unlink(c.as_ptr()); Err(e) }
            }
        }
    }

    pub fn open(cap: usize, _fd: Option<Fd>, name: &str) -> io::Result<Segment> {
        let c = std::ffi::CString::new(name).map_err(io::Error::other)?;
        let cap = cap.max(1);
        unsafe {
            let fd = libc::shm_open(c.as_ptr(), libc::O_RDONLY, 0 as libc::c_uint);
            if fd < 0 { return Err(io::Error::last_os_error()); }
            let r = map(fd, cap, false);
            libc::close(fd);
            // Mapped: the name is no longer needed by anyone.
            libc::shm_unlink(c.as_ptr());
            Ok(Segment { ptr: r?, cap, name: None })
        }
    }
}

// ─── Windows ───

#[cfg(windows)]
impl Segment {
    pub fn create(cap: usize, id: u64, epoch: u32) -> io::Result<(Segment, Option<Fd>, String)> {
        use windows_sys::Win32::{Foundation::INVALID_HANDLE_VALUE, System::Memory::*};
        let name = format!("Local\\crundi-{}-{id}-{epoch}", std::process::id());
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let cap = cap.max(1);
        unsafe {
            let h = CreateFileMappingW(INVALID_HANDLE_VALUE, std::ptr::null(), PAGE_READWRITE, ((cap as u64) >> 32) as u32, cap as u32, wide.as_ptr());
            if h.is_null() { return Err(io::Error::last_os_error()); }
            let v = MapViewOfFile(h, FILE_MAP_ALL_ACCESS, 0, 0, cap);
            if v.Value.is_null() { let e = io::Error::last_os_error(); windows_sys::Win32::Foundation::CloseHandle(h); return Err(e); }
            Ok((Segment { ptr: v.Value as *mut u8, cap, handle: h }, None, name))
        }
    }

    pub fn open(cap: usize, _fd: Option<Fd>, name: &str) -> io::Result<Segment> {
        use windows_sys::Win32::System::Memory::*;
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let cap = cap.max(1);
        unsafe {
            let h = OpenFileMappingW(FILE_MAP_READ, 0, wide.as_ptr());
            if h.is_null() { return Err(io::Error::last_os_error()); }
            let v = MapViewOfFile(h, FILE_MAP_READ, 0, 0, cap);
            if v.Value.is_null() { let e = io::Error::last_os_error(); windows_sys::Win32::Foundation::CloseHandle(h); return Err(e); }
            Ok(Segment { ptr: v.Value as *mut u8, cap, handle: h })
        }
    }
}

#[cfg(windows)]
impl Drop for Segment {
    fn drop(&mut self) {
        use windows_sys::Win32::System::Memory::{UnmapViewOfFile, MEMORY_MAPPED_VIEW_ADDRESS};
        unsafe {
            UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS { Value: self.ptr as *mut core::ffi::c_void });
            windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_sides_see_the_bytes() {
        let (mut host, fd, name) = Segment::create(64 * 64 * 4, 3, 1).unwrap();
        host.bytes_mut()[..4].copy_from_slice(&[1, 2, 3, 4]);
        let app = Segment::open(64 * 64 * 4, fd, &name).unwrap();
        assert_eq!(&app.bytes()[..4], &[1, 2, 3, 4]);
        host.bytes_mut()[4] = 9;
        assert_eq!(app.bytes()[4], 9);
    }
}
