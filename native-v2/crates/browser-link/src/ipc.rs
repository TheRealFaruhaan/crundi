//! The browser host's protocol: commands from the app (`Cmd`), events from
//! the host (`Ev`), each a length-prefixed JSON message on one local link.
//!
//! The link: on Linux and macOS a Unix socket pair, the host's end as its
//! stdin (both ways on one socket; on Linux a frame segment's memfd travels
//! with its `Shm` event as SCM_RIGHTS). On Windows the host's stdin carries
//! the commands and its stdout the events (anonymous pipes).
//!
//! A message: [u32 LE length][JSON]. Pixels never go through the link: the
//! host writes a view's frame into the view's shared-memory segment
//! (shm.rs) and sends `Paint`; the app copies it out and answers `Ack`, and
//! only then may the host write the next one.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::{collections::VecDeque, io};

/// A proxy a network's profile goes out through (net.rs), with its sign-in.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Proxy { pub port: u16, pub user: String, pub pass: String }

/// What the app asks of the host. `id` is a view (one browser), made by `Open`.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "c")]
pub enum Cmd {
    /// The first command: the host may start CEF (on Windows only once it
    /// is in the app's job object). `textures`: the app takes GPU textures
    /// too (gpu.rs `Mode::Texture`: `Ev::Texture`).
    Hello { #[serde(default)] textures: bool },
    /// A view at `url`, `w` x `h` CSS px at `dpr`. `frame`: a chat's or a
    /// panel's live frame (transparent, no menus or dialogs, new windows to
    /// the system's browser, the referrer `origin` on its first request);
    /// else a drawer tab's page or a hidden browser, on `network`'s profile.
    Open { id: u64, url: String, frame: bool, network: String, proxy: Option<Proxy>, origin: String, w: i32, h: i32, dpr: f32 },
    Close { id: u64 },
    /// The view's size; `screen`: its scale changed too.
    Size { id: u64, w: i32, h: i32, dpr: f32, screen: bool },
    Navigate { id: u64, url: String },
    /// Back (-1), forward (1), reload (0).
    History { id: u64, h: i32 },
    /// 0 move, 1 down, 2 up, 3 leave; button 0 left, 1 middle, 2 right.
    Mouse { id: u64, kind: i32, x: i32, y: i32, button: i32, clicks: i32, modifiers: u32 },
    Wheel { id: u64, x: i32, y: i32, dx: i32, dy: i32 },
    /// kind: 0 raw key down, 2 key up, 3 char (CEF's cef_key_event_type_t).
    Key { id: u64, kind: i32, modifiers: u32, code: i32, ch: u16 },
    /// Text an input method committed.
    Ime { id: u64, text: String },
    Focus { id: u64, on: bool },
    /// The view is not on screen (a tab behind another, the drawer closed,
    /// an embed scrolled away, a hidden browser): Chromium's WasHidden, its
    /// frame rate down to 1, and no frames sent until it is shown again
    /// (then one at once).
    Hidden { id: u64, on: bool },
    /// A DevTools protocol call; the answer is `CdpResult` with this `call`.
    Cdp { id: u64, call: i32, method: String, params: Value },
    /// Script in the view's main frame (no answer).
    Js { id: u64, code: String },
    /// DevTools for the view, in a window of its own (or closed when open).
    DevTools { id: u64 },
    /// The app has taken the view's last frame out of its segment.
    Ack { id: u64 },
    /// Cookies in every profile (one site and its subdomains, or all).
    ClearCookies { site: Option<String> },
    /// Where a download the host told of (`Ev::Download`) is saved: the
    /// whole path the person picked, or "" to cancel it.
    DownloadTo { id: u64, dl: u32, path: String },
    /// Close every view and leave.
    Quit,
}

/// What the host tells the app.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "e")]
pub enum Ev {
    /// CEF started.
    Ready { pid: u32 },
    /// CEF did not start: the host leaves.
    Failed { why: String },
    /// The view's frames are now in a new segment of `cap` bytes: on Linux
    /// its memfd came with this message; elsewhere `name` opens it.
    Shm { id: u64, cap: usize, name: String },
    /// A frame is in the segment: BGRA, `w` x `h` device px.
    Paint { id: u64, w: u32, h: u32 },
    Loading { id: u64, loading: bool, back: bool, fwd: bool },
    /// A main-frame load failed (not ERR_ABORTED).
    LoadError { id: u64, code: i32, text: String },
    /// A main-frame load ended with this HTTP status.
    LoadEnd { id: u64, status: i32 },
    Title { id: u64, title: String },
    Address { id: u64, url: String },
    Cursor { id: u64, kind: i32 },
    Console { id: u64, level: String, text: String, source: String, line: i32 },
    /// A link or script asked for a new window (none was opened).
    Popup { id: u64, url: String },
    Scrolled { id: u64 },
    /// A DevTools Network.* event.
    Net { id: u64, method: String, params: Value },
    CdpResult { id: u64, call: i32, ok: bool, result: Value },
    /// The view's page process ended (crashed, killed, out of memory).
    Gone { id: u64, status: i32 },
    /// The view is closed and its segment let go.
    Closed { id: u64 },
    /// The page started a download (held until `Cmd::DownloadTo`): its
    /// suggested file name and address (app/main.js wireDownloads).
    Download { id: u64, dl: u32, name: String, url: String },
    /// A download ended: saved at `path` (ok) or cancelled or failed.
    Downloaded { id: u64, dl: u32, path: String, ok: bool },
    /// A GPU frame's texture (gpu.rs `Mode::Texture`, Windows): CEF's NT
    /// handle duplicated into the app's process, `w` x `h`. The app closes it.
    Texture { id: u64, handle: u64, w: u32, h: u32 },
    /// The host's frame counters (every 2 s while they move): frames sent,
    /// frames not sent because nothing in them changed, frames of hidden
    /// views not sent, bytes copied into segments, GPU frames read back,
    /// and the GPU path's mode and last error.
    Stats { sent: u64, unchanged: u64, hidden: u64, copied: u64, gpu: u64, mode: String, error: String },
}

/// Most a message may be (a CDP answer with a PDF or a screenshot in it).
const MAX: usize = 256 * 1024 * 1024;

pub fn encode<T: Serialize>(m: &T) -> Vec<u8> {
    let body = serde_json::to_vec(m).unwrap_or_default();
    let mut b = Vec::with_capacity(4 + body.len());
    b.extend_from_slice(&(body.len() as u32).to_le_bytes());
    b.extend_from_slice(&body);
    b
}

/// Bytes in, whole messages out.
#[derive(Default)]
pub struct Decoder { buf: Vec<u8> }

impl Decoder {
    pub fn push(&mut self, d: &[u8]) { self.buf.extend_from_slice(d); }
    /// The next whole message: None when it hasn't all come yet; Err when
    /// the stream is broken (the link is then dropped).
    pub fn next<T: DeserializeOwned>(&mut self) -> Option<Result<T, String>> {
        if self.buf.len() < 4 { return None; }
        let n = u32::from_le_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
        if n > MAX { return Some(Err(format!("a message of {n} bytes"))); }
        if self.buf.len() < 4 + n { return None; }
        let r = serde_json::from_slice::<T>(&self.buf[4..4 + n]).map_err(|e| e.to_string());
        self.buf.drain(..4 + n);
        Some(r)
    }
}

/// A file descriptor passed with a message (Linux), else nothing.
#[cfg(unix)]
pub type Fd = std::os::fd::OwnedFd;
#[cfg(not(unix))]
pub type Fd = ();

/// One end's sending half: the bytes, and an fd to pass with them.
pub type SendFn = Box<dyn FnMut(&[u8], Option<Fd>) -> io::Result<()> + Send>;
/// One end's receiving half: bytes into the buffer (0: the other end is
/// gone), any fds that came with them onto the queue.
pub type RecvFn = Box<dyn FnMut(&mut [u8], &mut VecDeque<Fd>) -> io::Result<usize> + Send>;

#[cfg(unix)]
pub mod unix {
    //! A Unix socket's halves, with SCM_RIGHTS on Linux.
    use super::*;
    use std::io::{Read, Write};
    use std::os::fd::AsRawFd;
    use std::os::unix::net::UnixStream;

    pub fn sender(s: UnixStream) -> SendFn {
        let mut s = s;
        Box::new(move |b: &[u8], fd: Option<Fd>| {
            match fd {
                None => s.write_all(b),
                Some(fd) => {
                    let sent = send_with_fd(&s, b, fd.as_raw_fd())?;
                    drop(fd);
                    s.write_all(&b[sent..])
                }
            }
        })
    }

    pub fn receiver(s: UnixStream) -> RecvFn {
        let mut s = s;
        Box::new(move |buf: &mut [u8], fds: &mut VecDeque<Fd>| {
            if cfg!(target_os = "linux") { recv_with_fds(&s, buf, fds) } else { s.read(buf) }
        })
    }

    /// sendmsg with one fd as SCM_RIGHTS: the bytes sent.
    fn send_with_fd(s: &UnixStream, b: &[u8], fd: i32) -> io::Result<usize> {
        unsafe {
            let mut iov = libc::iovec { iov_base: b.as_ptr() as *mut libc::c_void, iov_len: b.len() };
            let space = libc::CMSG_SPACE(std::mem::size_of::<i32>() as u32) as usize;
            let mut cbuf = vec![0u8; space];
            let mut msg: libc::msghdr = std::mem::zeroed();
            msg.msg_iov = &mut iov;
            msg.msg_iovlen = 1;
            msg.msg_control = cbuf.as_mut_ptr() as *mut libc::c_void;
            msg.msg_controllen = space as _;
            let c = libc::CMSG_FIRSTHDR(&msg);
            (*c).cmsg_level = libc::SOL_SOCKET;
            (*c).cmsg_type = libc::SCM_RIGHTS;
            (*c).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<i32>() as u32) as _;
            std::ptr::write_unaligned(libc::CMSG_DATA(c) as *mut i32, fd);
            loop {
                let n = libc::sendmsg(s.as_raw_fd(), &msg, 0);
                if n >= 0 { return Ok(n as usize); }
                let e = io::Error::last_os_error();
                if e.kind() != io::ErrorKind::Interrupted { return Err(e); }
            }
        }
    }

    /// recvmsg: the bytes read, and every fd that came with them.
    #[allow(unused_variables)]
    fn recv_with_fds(s: &UnixStream, buf: &mut [u8], fds: &mut VecDeque<Fd>) -> io::Result<usize> {
        use std::os::fd::FromRawFd;
        unsafe {
            let mut iov = libc::iovec { iov_base: buf.as_mut_ptr() as *mut libc::c_void, iov_len: buf.len() };
            let space = libc::CMSG_SPACE((16 * std::mem::size_of::<i32>()) as u32) as usize;
            let mut cbuf = vec![0u8; space];
            let mut msg: libc::msghdr = std::mem::zeroed();
            msg.msg_iov = &mut iov;
            msg.msg_iovlen = 1;
            msg.msg_control = cbuf.as_mut_ptr() as *mut libc::c_void;
            msg.msg_controllen = space as _;
            #[cfg(target_os = "linux")]
            let flags = libc::MSG_CMSG_CLOEXEC;
            #[cfg(not(target_os = "linux"))]
            let flags = 0;
            let n = loop {
                let n = libc::recvmsg(s.as_raw_fd(), &mut msg, flags);
                if n >= 0 { break n as usize; }
                let e = io::Error::last_os_error();
                if e.kind() != io::ErrorKind::Interrupted { return Err(e); }
            };
            let mut c = libc::CMSG_FIRSTHDR(&msg);
            while !c.is_null() {
                if (*c).cmsg_level == libc::SOL_SOCKET && (*c).cmsg_type == libc::SCM_RIGHTS {
                    let data = libc::CMSG_DATA(c) as *const i32;
                    let count = ((*c).cmsg_len as usize - libc::CMSG_LEN(0) as usize) / std::mem::size_of::<i32>();
                    for k in 0..count { fds.push_back(std::os::fd::OwnedFd::from_raw_fd(std::ptr::read_unaligned(data.add(k)))); }
                }
                c = libc::CMSG_NXTHDR(&msg, c);
            }
            Ok(n)
        }
    }
}

/// Plain pipes' halves (Windows: the host's stdin and stdout).
#[allow(dead_code)]
pub mod pipe {
    use super::*;
    use std::io::{Read, Write};

    pub fn sender(mut w: impl Write + Send + 'static) -> SendFn {
        Box::new(move |b: &[u8], _fd: Option<Fd>| { w.write_all(b)?; w.flush() })
    }
    pub fn receiver(mut r: impl Read + Send + 'static) -> RecvFn {
        Box::new(move |buf: &mut [u8], _fds: &mut VecDeque<Fd>| r.read(buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip_in_pieces() {
        let a = encode(&Cmd::Navigate { id: 7, url: "https://example.com/".into() });
        let b = encode(&Ev::Paint { id: 7, w: 10, h: 20 });
        let mut d = Decoder::default();
        d.push(&a[..3]);
        assert!(d.next::<Cmd>().is_none());
        d.push(&a[3..]);
        match d.next::<Cmd>() { Some(Ok(Cmd::Navigate { id: 7, url })) => assert_eq!(url, "https://example.com/"), x => panic!("{x:?}") }
        d.push(&b);
        match d.next::<Ev>() { Some(Ok(Ev::Paint { id: 7, w: 10, h: 20 })) => {} x => panic!("{x:?}") }
        assert!(d.next::<Ev>().is_none());
    }

    /// A segment's memfd passed with its message, on a real socket pair.
    #[cfg(target_os = "linux")]
    #[test]
    fn an_fd_goes_with_its_message() {
        use std::os::unix::net::UnixStream;
        let (a, b) = UnixStream::pair().unwrap();
        let mut tx = unix::sender(a);
        let mut rx = unix::receiver(b);
        let (seg, fd, _) = crate::shm::Segment::create(4096, 1, 1).unwrap();
        let mut seg = seg;
        seg.bytes_mut()[..3].copy_from_slice(b"abc");
        tx(&encode(&Ev::Title { id: 1, title: "first".into() }), None).unwrap();
        tx(&encode(&Ev::Shm { id: 1, cap: 4096, name: String::new() }), fd).unwrap();
        let mut fds = VecDeque::new();
        let mut d = Decoder::default();
        let mut buf = vec![0u8; 65536];
        let mut got = vec![];
        while got.len() < 2 {
            let n = rx(&mut buf, &mut fds).unwrap();
            d.push(&buf[..n]);
            while let Some(m) = d.next::<Ev>() { got.push(m.unwrap()); }
        }
        assert!(matches!(got[1], Ev::Shm { cap: 4096, .. }));
        let fd = fds.pop_front().expect("the fd came");
        let m = crate::shm::Segment::open(4096, Some(fd), "").unwrap();
        assert_eq!(&m.bytes()[..3], b"abc");
    }
}
