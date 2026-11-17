//! The first-run setup page: a tiny HTTP server on 127.0.0.1 (random port,
//! one-time token in the URL) with a form for server, join code, name and
//! work folder. Submitting it performs the join.

use crate::config::Config;
use crate::join::{join_and_save, JoinArgs};
use anyhow::Result;
use serde::Deserialize;
use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long the page stays up if nobody uses it.
const LIFETIME: Duration = Duration::from_secs(30 * 60);

pub struct SetupServer {
    pub url: String,
    stop: Arc<AtomicBool>,
}

impl SetupServer {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Drop for SetupServer {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Form {
    server: String,
    code: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    work_dir: String,
}

type OnJoined = Box<dyn Fn(Config) + Send + 'static>;

/// Start the page. `on_joined` runs (on the page's thread) after a join is saved.
pub fn start(config_path: PathBuf, kind: String, on_joined: OnJoined) -> Result<SetupServer> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    let token = crate::util::random_hex(16);
    let url = format!("http://127.0.0.1:{port}/?t={token}");
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    std::thread::Builder::new().name("setup-page".into()).spawn(move || {
        let started = Instant::now();
        while !stop2.load(Ordering::SeqCst) && started.elapsed() < LIFETIME {
            match listener.accept() {
                Ok((stream, _)) => {
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                    let done = serve(stream, &token, port, &config_path, &kind, &on_joined);
                    if done {
                        break; // the token is single use
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(_) => std::thread::sleep(Duration::from_millis(100)),
            }
        }
    })?;
    Ok(SetupServer { url, stop })
}

fn respond(mut s: &TcpStream, status: &str, ctype: &str, body: &str) {
    let _ = write!(
        s,
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Frame-Options: DENY\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = s.flush();
}

/// Serve one request; true when a join succeeded (the page is then done).
fn serve(stream: TcpStream, token: &str, port: u16, config_path: &PathBuf, kind: &str, on_joined: &OnJoined) -> bool {
    let mut r = BufReader::new(&stream);
    let mut line = String::new();
    if r.read_line(&mut line).is_err() {
        return false;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("").to_string();
    let mut len = 0usize;
    let mut host = String::new();
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
            break;
        }
        let lower = h.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            len = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = lower.strip_prefix("host:") {
            host = v.trim().to_string();
        }
    }
    // Only our own address: a page on another site cannot reach us by DNS tricks.
    if host != format!("127.0.0.1:{port}") && host != format!("localhost:{port}") {
        respond(&stream, "403 Forbidden", "text/plain", "forbidden");
        return false;
    }
    let (path, query) = target.split_once('?').unwrap_or((target.as_str(), ""));
    let ok_token = query.split('&').any(|kv| kv == format!("t={token}"));
    if !ok_token {
        respond(&stream, "403 Forbidden", "text/plain", "This setup link has expired. Use Join a server... in the tray menu again.");
        return false;
    }
    match (method.as_str(), path) {
        ("GET", "/") => {
            let cfg = Config::load(config_path).unwrap_or_default();
            respond(&stream, "200 OK", "text/html; charset=utf-8", &page(&cfg));
            false
        }
        ("POST", "/join") => {
            if len > 64 * 1024 {
                respond(&stream, "413 Payload Too Large", "text/plain", "too large");
                return false;
            }
            let mut body = vec![0u8; len];
            if r.read_exact(&mut body).is_err() {
                return false;
            }
            let form: Form = match serde_json::from_slice(&body) {
                Ok(f) => f,
                Err(e) => {
                    respond(&stream, "400 Bad Request", "application/json", &json!({"ok": false, "error": e.to_string()}).to_string());
                    return false;
                }
            };
            let wd = form.work_dir.trim();
            let res = join_and_save(
                config_path,
                JoinArgs {
                    server: &form.server,
                    code: &form.code,
                    name: Some(form.name.as_str()).filter(|n| !n.trim().is_empty()),
                    work_dir: (!wd.is_empty()).then(|| PathBuf::from(wd)),
                    kind,
                },
            );
            match res {
                Ok(cfg) => {
                    let body = json!({"ok": true, "name": cfg.name}).to_string();
                    respond(&stream, "200 OK", "application/json", &body);
                    on_joined(cfg);
                    true
                }
                Err(e) => {
                    respond(&stream, "200 OK", "application/json", &json!({"ok": false, "error": format!("{e:#}")}).to_string());
                    false
                }
            }
        }
        _ => {
            respond(&stream, "404 Not Found", "text/plain", "not found");
            false
        }
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn page(cfg: &Config) -> String {
    let server = cfg.server.clone().unwrap_or_default();
    let name = cfg.display_name();
    let wd = cfg.work_dir.display().to_string();
    PAGE.replace("{server}", &esc(&server))
        .replace("{name}", &esc(&name))
        .replace("{workdir}", &esc(&wd))
}

const PAGE: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Join a Crundi server</title>
<style>
  :root { color-scheme: light dark; --fg:#1d1d1f; --mute:#6b6b70; --bg:#f6f6f7; --card:#fff; --line:#d9d9de; --acc:#3b6cf6; --bad:#c62828; }
  @media (prefers-color-scheme: dark) { :root { --fg:#ececef; --mute:#9a9aa2; --bg:#141416; --card:#1d1d20; --line:#33333a; --acc:#6d8ff8; --bad:#ef7070; } }
  body { margin:0; font:15px/1.45 system-ui, -apple-system, Segoe UI, sans-serif; background:var(--bg); color:var(--fg); }
  main { max-width:420px; margin:8vh auto; background:var(--card); border:1px solid var(--line); border-radius:10px; padding:28px; }
  h1 { font-size:20px; margin:0 0 4px; } p { color:var(--mute); margin:0 0 20px; }
  label { display:block; font-weight:600; margin:14px 0 6px; font-size:13px; }
  input { width:100%; box-sizing:border-box; padding:9px 10px; border:1px solid var(--line); border-radius:6px; background:transparent; color:inherit; font:inherit; }
  input:focus { outline:2px solid var(--acc); outline-offset:-1px; }
  #code { letter-spacing:.15em; text-transform:uppercase; font-family:ui-monospace, Consolas, monospace; }
  button { margin-top:22px; width:100%; padding:10px; border:0; border-radius:6px; background:var(--acc); color:#fff; font:inherit; font-weight:600; cursor:pointer; }
  button:disabled { opacity:.6; cursor:default; }
  #msg { margin-top:14px; min-height:1.4em; } .bad { color:var(--bad); }
</style></head>
<body><main>
<h1>Join a Crundi server</h1>
<p>Make a join code on the server: Workers, Add a PC. It works once, for 10 minutes.</p>
<form id="f">
  <label for="server">Server address</label>
  <input id="server" name="server" required placeholder="https://crundi.example.com" value="{server}">
  <label for="code">Join code</label>
  <input id="code" name="code" required maxlength="12" autocomplete="off" placeholder="ABCD2345">
  <label for="name">Name for this PC</label>
  <input id="name" name="name" value="{name}">
  <label for="workDir">Work folder</label>
  <input id="workDir" name="workDir" value="{workdir}">
  <button id="go" type="submit">Join</button>
  <div id="msg"></div>
</form>
</main>
<script>
const f = document.getElementById('f'), msg = document.getElementById('msg'), go = document.getElementById('go');
f.addEventListener('submit', async (e) => {
  e.preventDefault();
  go.disabled = true; msg.className = ''; msg.textContent = 'Joining...';
  const body = Object.fromEntries(new FormData(f).entries());
  try {
    const r = await fetch('/join' + location.search, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
    const j = await r.json();
    if (j.ok) { f.innerHTML = '<h1>Joined</h1><p>This PC is now a worker called <b></b>. You can close this page.</p>'; f.querySelector('b').textContent = j.name || ''; return; }
    msg.className = 'bad'; msg.textContent = j.error || 'Join failed';
  } catch (err) { msg.className = 'bad'; msg.textContent = 'Could not reach the worker: ' + err; }
  go.disabled = false;
});
</script>
</body></html>
"#;
