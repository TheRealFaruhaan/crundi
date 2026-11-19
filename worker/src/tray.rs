//! The tray app: icon + menu on the main thread (tao event loop, as
//! tray-icon recommends), the link on a tokio runtime in the background.
//!
//! Menu (spec): status line; Pause/Resume; Open work folder; Join a server...
//! (when not joined); Start at login; Quit.

use crate::config::Config;
use crate::link::{self, Control, LinkHandle, LinkOptions, State, Status};
use crate::service;
use anyhow::{Context, Result};
use std::path::PathBuf;
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// Is there a desktop to put a tray icon on?
pub fn desktop_available() -> bool {
    if cfg!(any(windows, target_os = "macos")) {
        return true;
    }
    std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some()
}

enum UserEvent {
    Menu(MenuEvent),
    Status(Status),
    Joined(Box<Config>),
    /// Another worker held the lock: try again.
    RetryLock,
}

struct Items {
    status: MenuItem,
    pause: MenuItem,
    open: MenuItem,
    join: Option<MenuItem>,
    autostart: CheckMenuItem,
}

fn build_menu(joined: bool) -> Result<(Menu, Items)> {
    let menu = Menu::new();
    let items = Items {
        status: MenuItem::with_id("status", "Starting...", false, None),
        pause: MenuItem::with_id("pause", "Pause", joined, None),
        open: MenuItem::with_id("open", "Open work folder", true, None),
        join: (!joined).then(|| MenuItem::with_id("join", "Join a server...", true, None)),
        autostart: CheckMenuItem::with_id("autostart", "Start at login", true, service::autostart_enabled(), None),
    };
    menu.append(&items.status)?;
    menu.append(&PredefinedMenuItem::separator())?;
    if let Some(j) = &items.join {
        menu.append(j)?;
    }
    menu.append(&items.pause)?;
    menu.append(&items.open)?;
    menu.append(&items.autostart)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&MenuItem::with_id("quit", "Quit", true, None))?;
    Ok((menu, items))
}

/// A 32x32 round icon: a ring in the state's colour with a dot in the middle.
pub fn make_icon_rgba(state: State, paused: bool) -> Vec<u8> {
    let (r, g, b) = match state {
        State::Online if paused => (0xE0, 0xA0, 0x30),
        State::Online => (0x2E, 0xB8, 0x6A),
        State::Connecting | State::Offline => (0x8A, 0x8F, 0x98),
        State::Unauthorized | State::Removed | State::Replaced | State::Conflict => (0xD0, 0x44, 0x44),
        State::NotJoined | State::Stopped | State::Updating => (0x5B, 0x7C, 0xF0),
    };
    const N: usize = 32;
    let mut px = vec![0u8; N * N * 4];
    let c = (N as f32 - 1.0) / 2.0;
    for y in 0..N {
        for x in 0..N {
            let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt();
            // Anti-aliased coverage of the outer disc, minus a gap ring, plus the dot.
            let cover = |radius: f32| (radius - d + 0.5).clamp(0.0, 1.0);
            let disc = cover(15.5);
            let hole = cover(11.0) - cover(7.0);
            let a = (disc - hole).clamp(0.0, 1.0);
            let i = (y * N + x) * 4;
            px[i] = r;
            px[i + 1] = g;
            px[i + 2] = b;
            px[i + 3] = (a * 255.0) as u8;
        }
    }
    px
}

fn icon(state: State, paused: bool) -> Option<Icon> {
    Icon::from_rgba(make_icon_rgba(state, paused), 32, 32).ok()
}

fn open_folder(path: &std::path::Path) {
    let _ = std::fs::create_dir_all(path);
    let prog = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(prog).arg(path).spawn();
}

/// A desktop notification for a job or chat starting or ending. tray-icon
/// has none: Windows and macOS use notify-rust; Linux runs `notify-send`
/// when it is installed (libnotify), else nothing is shown.
fn notify(v: &serde_json::Value) {
    let what = v.get("event").and_then(|e| e.as_str()).unwrap_or("");
    let state = v.get("state").and_then(|e| e.as_str()).unwrap_or("");
    let title = match (what, state) {
        ("job", "started") => "Crundi: a job started on this PC",
        ("job", _) => "Crundi: a job finished",
        ("chat", "started") if v.get("mode").is_some() => "Crundi: a terminal is running on this PC",
        ("chat", "started") => "Crundi: a chat is running on this PC",
        ("chat", _) if v.get("mode").is_some() => "Crundi: a terminal ended",
        ("chat", _) => "Crundi: a chat ended",
        _ => return,
    };
    let body = crate::app::event_line(v);
    #[cfg(any(windows, target_os = "macos"))]
    {
        let _ = notify_rust::Notification::new().summary(title).body(&body).show();
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        if let Some(p) = crate::util::find_program("notify-send") {
            let _ = std::process::Command::new(p).args(["-a", "Crundi worker", title, &body]).status();
        }
    }
}

struct App {
    rt: tokio::runtime::Runtime,
    proxy: EventLoopProxy<UserEvent>,
    config_path: PathBuf,
    custom_config: bool,
    kind: String,
    tray: Option<TrayIcon>,
    items: Option<Items>,
    joined: bool,
    link: Option<LinkHandle>,
    status: Status,
    setup: Option<crate::setup::SetupServer>,
    /// One worker per PC: held while the link runs.
    lock: Option<crate::lock::WorkerLock>,
    /// A retry of the lock is scheduled.
    retry_pending: bool,
}

impl App {
    fn config(&self) -> Config {
        Config::load(&self.config_path).unwrap_or_default()
    }

    fn start_link(&mut self, cfg: Config) {
        self.stop_link();
        self.joined = cfg.joined();
        self.status = Status::from_config(&cfg, if self.joined { State::Connecting } else { State::NotJoined });
        if !self.joined {
            return;
        }
        if self.lock.is_none() {
            let dir = crate::lock::lock_dir(&self.config_path);
            match crate::lock::acquire_waiting(&dir, &self.kind, crate::lock::start_wait()) {
                Ok(l) => self.lock = Some(l),
                Err(e) => {
                    // Another worker (the PC app's) runs here: show it, try again later.
                    self.status = Status::from_config(&cfg, State::Conflict);
                    self.status.error = Some(e.to_string());
                    if !self.retry_pending {
                        self.retry_pending = true;
                        let proxy = self.proxy.clone();
                        self.rt.spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                            let _ = proxy.send_event(UserEvent::RetryLock);
                        });
                    }
                    return;
                }
            }
        }
        let mut opts = LinkOptions::new(&self.kind, Some(self.config_path.clone()));
        let (ev_tx, mut ev_rx) = tokio::sync::mpsc::unbounded_channel::<serde_json::Value>();
        opts.events = Some(ev_tx);
        self.rt.spawn(async move {
            while let Some(v) = ev_rx.recv().await {
                tokio::task::spawn_blocking(move || notify(&v));
            }
        });
        let _guard = self.rt.enter();
        let h = link::spawn(cfg, opts);
        let mut rx = h.status.clone();
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            loop {
                let s = rx.borrow_and_update().clone();
                if proxy.send_event(UserEvent::Status(s)).is_err() {
                    break;
                }
                if rx.changed().await.is_err() {
                    break;
                }
            }
        });
        self.link = Some(h);
    }

    fn stop_link(&mut self) {
        if let Some(h) = self.link.take() {
            let _ = h.ctrl.send(Control::Quit);
            let _ = self.rt.block_on(async {
                tokio::time::timeout(std::time::Duration::from_secs(2), h.task).await
            });
        }
    }

    fn rebuild_menu(&mut self) {
        let Some(tray) = &self.tray else { return };
        if let Ok((menu, items)) = build_menu(self.joined) {
            tray.set_menu(Some(Box::new(menu)));
            self.items = Some(items);
        }
        self.refresh();
    }

    fn refresh(&self) {
        let s = &self.status;
        if let Some(items) = &self.items {
            items.status.set_text(s.line());
            items.pause.set_text(if s.paused { "Resume" } else { "Pause" });
            items.pause.set_enabled(self.joined);
        }
        if let Some(tray) = &self.tray {
            let _ = tray.set_icon(icon(s.state, s.paused));
            let _ = tray.set_tooltip(Some(format!("Crundi worker: {}", s.line())));
        }
    }

    fn on_status(&mut self, s: Status) {
        if s.state == State::Updating {
            // A signed update replaced the executable: start the new one.
            self.tray = None;
            self.lock = None;
            crate::update::restart();
        }
        let joined_now = !matches!(s.state, State::Unauthorized | State::Removed | State::NotJoined);
        self.status = s;
        if joined_now != self.joined {
            self.joined = joined_now;
            self.rebuild_menu();
        } else {
            self.refresh();
        }
    }

    fn open_setup(&mut self) {
        let proxy = self.proxy.clone();
        let started = crate::setup::start(
            self.config_path.clone(),
            self.kind.clone(),
            Box::new(move |cfg| {
                let _ = proxy.send_event(UserEvent::Joined(Box::new(cfg)));
            }),
        );
        match started {
            Ok(server) => {
                let _ = webbrowser::open(&server.url);
                self.setup = Some(server);
            }
            Err(e) => eprintln!("crundi-worker: could not start the setup page: {e:#}"),
        }
    }

    fn on_menu(&mut self, e: MenuEvent, flow: &mut ControlFlow) {
        let id = e.id;
        if id == "quit" {
            self.stop_link();
            self.tray = None;
            *flow = ControlFlow::Exit;
        } else if id == "pause" {
            if let Some(h) = &self.link {
                let _ = h.ctrl.send(Control::Pause(!self.status.paused));
            }
        } else if id == "open" {
            open_folder(&self.config().work_dir);
        } else if id == "join" {
            self.open_setup();
        } else if id == "autostart" {
            let want = !service::autostart_enabled();
            let custom = self.custom_config.then_some(self.config_path.as_path());
            if let Err(e) = service::set_autostart(want, custom) {
                eprintln!("crundi-worker: {e:#}");
            }
            if let Some(items) = &self.items {
                items.autostart.set_checked(service::autostart_enabled());
            }
        }
    }
}

/// Run the tray app. Returns the exit code.
pub fn run(config_path: PathBuf, custom_config: bool, kind: String) -> Result<i32> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .context("starting the runtime")?;

    #[allow(unused_mut)]
    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
    }
    let proxy = event_loop.create_proxy();
    {
        let p = proxy.clone();
        MenuEvent::set_event_handler(Some(move |e| {
            let _ = p.send_event(UserEvent::Menu(e));
        }));
    }

    let mut app = App {
        rt,
        proxy,
        config_path,
        custom_config,
        kind,
        tray: None,
        items: None,
        joined: false,
        link: None,
        status: Status::from_config(&Config::default(), State::NotJoined),
        setup: None,
        lock: None,
        retry_pending: false,
    };

    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(StartCause::Init) => {
                // The icon is made once the loop runs (needed on macOS).
                let cfg = app.config();
                let joined = cfg.joined();
                let never_joined = cfg.server.is_none();
                let (menu, items) = match build_menu(joined) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("crundi-worker: tray menu failed: {e}");
                        *flow = ControlFlow::ExitWithCode(1);
                        return;
                    }
                };
                match TrayIconBuilder::new()
                    .with_menu(Box::new(menu))
                    .with_tooltip("Crundi worker")
                    .with_icon(icon(State::Connecting, false).expect("icon"))
                    .build()
                {
                    Ok(t) => app.tray = Some(t),
                    Err(e) => {
                        eprintln!("crundi-worker: tray icon failed: {e}");
                        *flow = ControlFlow::ExitWithCode(1);
                        return;
                    }
                }
                app.items = Some(items);
                app.start_link(cfg);
                app.refresh();
                if never_joined {
                    app.open_setup();
                }
            }
            Event::UserEvent(UserEvent::Menu(e)) => app.on_menu(e, flow),
            Event::UserEvent(UserEvent::Status(s)) => app.on_status(s),
            Event::UserEvent(UserEvent::RetryLock) => {
                app.retry_pending = false;
                if app.link.is_none() {
                    let cfg = app.config();
                    app.start_link(cfg);
                    app.refresh();
                }
            }
            Event::UserEvent(UserEvent::Joined(cfg)) => {
                app.setup = None;
                app.start_link(*cfg);
                app.rebuild_menu();
            }
            _ => {}
        }
    });
}
