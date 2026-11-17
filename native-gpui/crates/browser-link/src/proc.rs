//! The browser host as a process: starting it so that it dies with the app
//! on every OS, the host's own guard against an app that went away, the
//! host's PID on disk, and reaping what a crashed run left behind.
//!
//! The host is its own executable, `crundi-browser-host` (crates/browser-host),
//! next to the app's (on macOS in the bundle's Contents/MacOS): only it links
//! libcef, so the app never maps CEF. It is started with `--browser-host`;
//! CEF starts it again for its helpers (`--type=…`).
//!
//! - Linux: the host gets PR_SET_PDEATHSIG (SIGKILL) before it runs, and
//!   leads a process group of its own (CEF's helpers are in it).
//! - Windows: the app's job object, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE; the
//!   host is put in it before it gets `Hello` (so before CEF starts any
//!   helper), and the helpers inherit it.
//! - macOS: the host leaves on EOF of its link, and a watchdog thread
//!   leaves when its parent is gone.
//! - Everywhere: the host leaves when its link closes (ipc.rs), and CEF's
//!   helpers leave when the host does (their Mojo link closes).

use crate::ipc::{RecvFn, SendFn};
use std::{io, path::{Path, PathBuf}, process::{Child, Command, Stdio}};

/// The flag the app starts the host with (also what marks a host process).
pub const FLAG: &str = "--browser-host";
/// The host's executable, next to the app's.
pub const HOST_EXE: &str = if cfg!(windows) { "crundi-browser-host.exe" } else { "crundi-browser-host" };

/// Where the host's executable is: next to this one's.
pub fn host_exe() -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let dir = exe.parent().ok_or_else(|| io::Error::other("no folder for the app's binary"))?;
    let host = dir.join(HOST_EXE);
    if host.exists() { Ok(host) } else { Err(io::Error::new(io::ErrorKind::NotFound, format!("{} is not next to the app (tools/build.sh builds it)", host.display()))) }
}
/// The host's PID file, in the client's config folder.
pub const PID_FILE: &str = "browser-host.pid";

pub struct Spawned { pub child: Child, pub send: SendFn, pub recv: RecvFn }

/// Start the host (`crundi-browser-host --browser-host`), its link made.
pub fn spawn_host() -> io::Result<Spawned> {
    let mut cmd = Command::new(host_exe()?);
    cmd.arg(FLAG).env("CRUNDI_HOST_PARENT", std::process::id().to_string()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
    spawn_os(cmd)
}

#[cfg(unix)]
fn spawn_os(mut cmd: Command) -> io::Result<Spawned> {
    use std::os::unix::{net::UnixStream, process::CommandExt};
    let (app_end, host_end) = UnixStream::pair()?;
    cmd.stdin(Stdio::from(std::os::fd::OwnedFd::from(host_end)));
    // A group of its own: a stale host is reaped with its helpers.
    cmd.process_group(0);
    #[cfg(target_os = "linux")]
    {
        let parent = std::process::id() as i32;
        // After fork, before exec: only async-signal-safe calls.
        unsafe {
            cmd.pre_exec(move || {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL as libc::c_ulong, 0, 0, 0);
                if libc::getppid() != parent { libc::_exit(0); }
                Ok(())
            });
        }
    }
    let child = cmd.spawn()?;
    let r = app_end.try_clone()?;
    Ok(Spawned { child, send: crate::ipc::unix::sender(app_end), recv: crate::ipc::unix::receiver(r) })
}

#[cfg(windows)]
fn spawn_os(mut cmd: Command) -> io::Result<Spawned> {
    use std::os::windows::{io::AsRawHandle, process::CommandExt};
    // CREATE_NO_WINDOW: no console for the host.
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).creation_flags(0x0800_0000);
    let mut child = cmd.spawn()?;
    let job = job();
    if !job.is_null() {
        let ok = unsafe { windows_sys::Win32::System::JobObjects::AssignProcessToJobObject(job, child.as_raw_handle() as _) };
        if ok == 0 { eprintln!("[browser] the host is not in the job: {}", io::Error::last_os_error()); }
    }
    let stdin = child.stdin.take().ok_or_else(|| io::Error::other("no stdin"))?;
    let stdout = child.stdout.take().ok_or_else(|| io::Error::other("no stdout"))?;
    Ok(Spawned { child, send: crate::ipc::pipe::sender(stdin), recv: crate::ipc::pipe::receiver(stdout) })
}

/// The app's job object (made once, kept open for the app's life: when the
/// app ends, however it ends, Windows closes it and kills what is in it).
#[cfg(windows)]
fn job() -> windows_sys::Win32::Foundation::HANDLE {
    use std::sync::OnceLock;
    use windows_sys::Win32::System::JobObjects::*;
    static JOB: OnceLock<usize> = OnceLock::new();
    *JOB.get_or_init(|| unsafe {
        let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if h.is_null() { return 0; }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(h, JobObjectExtendedLimitInformation, &info as *const _ as *const core::ffi::c_void, std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32);
        h as usize
    }) as windows_sys::Win32::Foundation::HANDLE
}

// ─── The host's side ───

/// The host's link to the app (its stdin; on Windows stdin and stdout).
pub fn host_link() -> io::Result<(SendFn, RecvFn)> {
    #[cfg(unix)]
    {
        use std::os::fd::FromRawFd;
        // The socket moves off fd 0 (close-on-exec), and fd 0 becomes
        // /dev/null: nothing the host starts can hold the link open.
        let fd = unsafe { libc::fcntl(0, libc::F_DUPFD_CLOEXEC, 3) };
        if fd < 0 { return Err(io::Error::last_os_error()); }
        unsafe {
            let null = libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY);
            if null >= 0 { libc::dup2(null, 0); libc::close(null); }
        }
        let s = unsafe { std::os::unix::net::UnixStream::from_raw_fd(fd) };
        let r = s.try_clone()?;
        Ok((crate::ipc::unix::sender(s), crate::ipc::unix::receiver(r)))
    }
    #[cfg(not(unix))]
    {
        Ok((crate::ipc::pipe::sender(std::io::stdout()), crate::ipc::pipe::receiver(std::io::stdin())))
    }
}

/// The host leaves when the app is gone, whatever the link says.
pub fn guard_parent() {
    let parent: u32 = std::env::var("CRUNDI_HOST_PARENT").ok().and_then(|p| p.parse().ok()).unwrap_or(0);
    #[cfg(target_os = "linux")]
    unsafe {
        libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL as libc::c_ulong, 0, 0, 0);
        if parent != 0 && libc::getppid() as u32 != parent { std::process::exit(0); }
    }
    #[cfg(not(target_os = "linux"))]
    {
        if parent == 0 { return; }
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if !alive(parent) { std::process::exit(0); }
            #[cfg(unix)]
            if unsafe { libc::getppid() } as u32 != parent { std::process::exit(0); }
        });
    }
}

// ─── Processes ───

/// Is process `pid` there?
pub fn alive(pid: u32) -> bool {
    if pid == 0 { return false; }
    #[cfg(unix)]
    {
        let r = unsafe { libc::kill(pid as i32, 0) };
        r == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::*};
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() { return false; }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code);
        CloseHandle(h);
        ok != 0 && code == 259 // STILL_ACTIVE
    }
}

/// Is `pid` a browser host (the host's executable, or an older app's
/// binary started as one; not some other process that took the number since)?
pub fn is_host(pid: u32) -> bool {
    if !alive(pid) { return false; }
    let app_name = std::env::current_exe().ok().and_then(|e| e.file_name().map(|n| n.to_string_lossy().to_string())).unwrap_or_default();
    let names = [HOST_EXE.to_string(), app_name];
    #[cfg(target_os = "linux")]
    {
        let Ok(raw) = std::fs::read(format!("/proc/{pid}/cmdline")) else { return false };
        let args: Vec<String> = raw.split(|b| *b == 0).map(|a| String::from_utf8_lossy(a).to_string()).collect();
        let first = args.first().map(|a| Path::new(a).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).unwrap_or_default();
        args.iter().any(|a| a == FLAG) && names.contains(&first)
    }
    #[cfg(target_os = "macos")]
    {
        let out = Command::new("ps").args(["-p", &pid.to_string(), "-o", "command="]).output();
        out.map(|o| { let s = String::from_utf8_lossy(&o.stdout); s.contains(FLAG) && names.iter().any(|n| s.contains(n.as_str())) }).unwrap_or(false)
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::*};
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() { return false; }
        let mut buf = vec![0u16; 1024];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut n);
        CloseHandle(h);
        let path = String::from_utf16_lossy(&buf[..n as usize]).to_lowercase();
        ok != 0 && names.iter().any(|x| !x.is_empty() && path.ends_with(&x.to_lowercase()))
    }
}

/// The processes under `pid` (its children, theirs, …), deepest last.
#[allow(dead_code)]
pub fn descendants(pid: u32) -> Vec<u32> {
    let pairs: Vec<(u32, u32)> = {
        #[cfg(target_os = "linux")]
        {
            std::fs::read_dir("/proc").map(|d| d.flatten().filter_map(|e| {
                let p: u32 = e.file_name().to_string_lossy().parse().ok()?;
                let stat = std::fs::read_to_string(format!("/proc/{p}/stat")).ok()?;
                // "pid (comm) state ppid …": comm may hold spaces and parentheses.
                let rest = &stat[stat.rfind(')')? + 2..];
                let ppid: u32 = rest.split(' ').nth(1)?.parse().ok()?;
                Some((p, ppid))
            }).collect()).unwrap_or_default()
        }
        #[cfg(target_os = "macos")]
        {
            let out = Command::new("ps").args(["-A", "-o", "pid=,ppid="]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
            out.lines().filter_map(|l| { let mut it = l.split_whitespace(); Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?)) }).collect()
        }
        #[cfg(windows)]
        { Vec::new() }
    };
    let mut out = vec![];
    let mut todo = vec![pid];
    while let Some(p) = todo.pop() {
        for (c, pp) in &pairs { if *pp == p && !out.contains(c) { out.push(*c); todo.push(*c); } }
    }
    out
}

/// Kill the host `pid` and everything under it (its group, its descendants).
pub fn kill_tree(pid: u32) {
    if pid == 0 { return; }
    #[cfg(unix)]
    {
        let under = descendants(pid);
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
            libc::kill(pid as i32, libc::SIGKILL);
            for p in under { libc::kill(p as i32, libc::SIGKILL); }
        }
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::*};
        let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if !h.is_null() { TerminateProcess(h, 1); CloseHandle(h); }
    }
}

// ─── What a crashed run leaves ───

pub fn pid_path(config_dir: &Path) -> PathBuf { config_dir.join(PID_FILE) }

pub fn write_pid(config_dir: &Path, pid: u32) {
    let _ = std::fs::create_dir_all(config_dir);
    let _ = std::fs::write(pid_path(config_dir), format!("{pid}\n"));
}

pub fn remove_pid(config_dir: &Path) { let _ = std::fs::remove_file(pid_path(config_dir)); }

/// At the app's start: a host the last run left (it crashed, or was
/// killed before it could stop it) is killed, and CEF's stale locks go.
/// Returns the PID reaped.
pub fn reap_stale(config_dir: &Path, cef_root: &Path) -> Option<u32> {
    let pid: Option<u32> = std::fs::read_to_string(pid_path(config_dir)).ok().and_then(|s| s.trim().parse().ok());
    let reaped = pid.filter(|p| is_host(*p)).inspect(|p| {
        eprintln!("[browser] a browser host from an earlier run ({p}) is still there: stopping it");
        kill_tree(*p);
        for _ in 0..40 { if !alive(*p) { break; } std::thread::sleep(std::time::Duration::from_millis(50)); }
    });
    remove_pid(config_dir);
    clear_stale_locks(cef_root);
    reaped
}

/// Chromium's profile locks whose owner is gone (it crashed or was killed):
/// SingletonLock, -Socket and -Cookie (a link to "<host>-<pid>"), Windows'
/// "lockfile"; in the root cache folder and each network's profile.
pub fn clear_stale_locks(root: &Path) {
    let mut dirs = vec![root.to_path_buf()];
    if let Ok(d) = std::fs::read_dir(root) {
        for e in d.flatten() { if e.file_name().to_string_lossy().starts_with("drawer-") { dirs.push(e.path()); } }
    }
    for dir in dirs {
        let lock = dir.join("SingletonLock");
        let stale = match std::fs::read_link(&lock) {
            Ok(t) => t.to_string_lossy().rsplit('-').next().and_then(|p| p.parse::<u32>().ok()).is_none_or(|p| !alive(p)),
            Err(_) => lock.exists(),
        };
        if stale {
            for n in ["SingletonLock", "SingletonSocket", "SingletonCookie"] { let _ = std::fs::remove_file(dir.join(n)); }
        }
        // Windows: held open while its owner runs, so removing it fails then.
        let _ = std::fs::remove_file(dir.join("lockfile"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_is_alive_and_not_a_host() {
        assert!(alive(std::process::id()));
        assert!(!is_host(std::process::id()));
        assert!(!alive(0));
    }

    #[cfg(unix)]
    #[test]
    fn a_stale_lock_goes_a_live_one_stays() {
        let d = std::env::temp_dir().join(format!("crundi-locks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("drawer-srv")).unwrap();
        // A dead owner (PID 0x7ffffff0 is never there) in the root; ours in the profile.
        std::os::unix::fs::symlink("somehost-2147483632", d.join("SingletonLock")).unwrap();
        std::fs::write(d.join("SingletonCookie"), "x").unwrap();
        std::os::unix::fs::symlink(format!("somehost-{}", std::process::id()), d.join("drawer-srv/SingletonLock")).unwrap();
        clear_stale_locks(&d);
        assert!(std::fs::symlink_metadata(d.join("SingletonLock")).is_err());
        assert!(!d.join("SingletonCookie").exists());
        assert!(std::fs::symlink_metadata(d.join("drawer-srv/SingletonLock")).is_ok());
        let _ = std::fs::remove_dir_all(&d);
    }
}
