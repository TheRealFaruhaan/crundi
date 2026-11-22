//! Local mode's screens in the window (app/index.html): the status screen
//! (#status-screen: Mini, the name, the status dot and words, the server's
//! log, Start / Stop / Restart) while the local server is not up and signed
//! in to, and the first-run setup wizard (#setup-screen: seven steps, Back,
//! Skip on the optional tunnel step, Next / "Save & Start", "Restore from a
//! backup" instead). One cached view, shown by the sign-in view.

use crate::local::{self, Local, Setup};
use crundi_theme::T;
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};

pub const SETUP_TOTAL: usize = 7;
/// The optional Cloudflare tunnel step (Skip).
pub const SETUP_SKIPPABLE: usize = 4;

pub struct LocalScreen {
    local: Entity<Local>,
    pub step: usize,
    token: Entity<w::InputState>,
    username: Entity<w::InputState>,
    projects: Entity<w::InputState>,
    tunnel_token: Entity<w::InputState>,
    tunnel_url: Entity<w::InputState>,
    single: bool,
    auto_update: bool,
    auto_launch: bool,
    /// The step whose first box takes the keys on the next frame.
    focus_step: Option<usize>,
    _subs: Vec<Subscription>,
}

impl LocalScreen {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let local = local::local(cx);
        let inp = |ph: &'static str, window: &mut Window, cx: &mut Context<Self>| cx.new(|cx| w::InputState::new(window, cx).placeholder(ph));
        let projects_ph = if cfg!(windows) { "C:\\Projects" } else { "/home/you/projects" };
        LocalScreen {
            token: inp("123456:ABC-DEF...", window, cx),
            username: inp("your_username", window, cx),
            projects: inp(projects_ph, window, cx),
            tunnel_token: inp("eyJhIjoi...", window, cx),
            tunnel_url: inp("https://crundi.yourdomain.com", window, cx),
            step: 1, single: true, auto_update: true, auto_launch: true, focus_step: Some(1),
            _subs: vec![cx.observe(&local, |_: &mut Self, _, cx| cx.notify())],
            local,
        }
    }

    fn val(e: &Entity<w::InputState>, cx: &App) -> String { e.read(cx).value().trim().to_string() }

    /// showSetupStep.
    pub fn show_step(&mut self, n: usize, cx: &mut Context<Self>) {
        self.step = n.clamp(1, SETUP_TOTAL);
        self.focus_step = Some(self.step);
        cx.notify();
    }

    /// validateSetupStep: the alert's words, or None.
    fn invalid(&self, n: usize, cx: &App) -> Option<&'static str> {
        if n == 1 && Self::val(&self.token, cx).is_empty() { return Some("Paste your Telegram bot token to continue."); }
        if n == 2 && Self::val(&self.username, cx).is_empty() { return Some("Enter your Telegram username to continue."); }
        if n == 3 && self.single && Self::val(&self.projects, cx).is_empty() { return Some("Choose your projects directory (or switch to Multiple folders)."); }
        None
    }

    /// window.alert: closed, the keys go back to the step's box (the browser
    /// gives the focus back to what had it).
    fn alert(msg: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let me = cx.entity().downgrade();
        w::ask(window, cx, w::Ask { message: msg.into(), ok: "OK".into(), ..Default::default() }, move |_, _, cx| {
            let _ = me.update(cx, |s, cx| { s.focus_step = Some(s.step); cx.notify(); });
        });
    }

    /// setupNext.
    pub fn next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(m) = self.invalid(self.step, cx) { Self::alert(m, window, cx); return; }
        if self.step == SETUP_TOTAL { return self.save(false, window, cx); }
        self.show_step(self.step + 1, cx);
    }

    /// setupPrev.
    pub fn back(&mut self, cx: &mut Context<Self>) { if self.step > 1 { self.show_step(self.step - 1, cx); } }

    /// setupSkip: the tunnel dropped, on to the next step.
    pub fn skip(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tunnel_token.update(cx, |i, cx| i.set_value("", window, cx));
        self.tunnel_url.update(cx, |i, cx| i.set_value("", window, cx));
        self.show_step(self.step + 1, cx);
    }

    /// A click into the step's box.
    pub fn focus_box(&mut self, cx: &mut Context<Self>) { self.focus_step = Some(self.step); cx.notify(); }

    pub fn set_single(&mut self, single: bool, cx: &mut Context<Self>) { self.single = single; cx.notify(); }
    pub fn set_checks(&mut self, update: Option<bool>, launch: Option<bool>, cx: &mut Context<Self>) {
        if let Some(u) = update { self.auto_update = u; }
        if let Some(l) = launch { self.auto_launch = l; }
        cx.notify();
    }

    /// Browse (setup:browse).
    pub fn browse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(h) = local::hooks(cx) else { return };
        if let Some(p) = (h.browse)() { self.projects.update(cx, |i, cx| i.set_value(p, window, cx)); }
    }

    /// saveSetup (or the restore link's save).
    pub fn save(&mut self, restore: bool, window: &mut Window, cx: &mut Context<Self>) {
        let setup = if restore { Setup { restore: true, auto_update: true, auto_launch: true, ..Default::default() } } else {
            let s = Setup {
                bot_token: Self::val(&self.token, cx),
                username: Self::val(&self.username, cx).trim_start_matches('@').to_string(),
                projects_dir: if self.single { Self::val(&self.projects, cx) } else { String::new() },
                tunnel_token: Self::val(&self.tunnel_token, cx),
                tunnel_url: Self::val(&self.tunnel_url, cx),
                auto_update: self.auto_update, auto_launch: self.auto_launch, restore: false,
            };
            if s.bot_token.is_empty() || s.username.is_empty() { Self::alert("Bot token and username are required.", window, cx); return; }
            if self.single && s.projects_dir.is_empty() { Self::alert("Projects directory is required for single folder mode.", window, cx); return; }
            s
        };
        local::update(cx, |l| l.wizard = false);
        if let Some(h) = local::hooks(cx) { (h.save_setup)(setup); }
    }

    /// The driver's look at the wizard.
    pub fn props(&self, cx: &App) -> serde_json::Value {
        serde_json::json!({ "step": self.step, "single": self.single, "autoUpdate": self.auto_update, "autoLaunch": self.auto_launch,
            "token": Self::val(&self.token, cx), "username": Self::val(&self.username, cx), "projects": Self::val(&self.projects, cx),
            "tunnelToken": Self::val(&self.tunnel_token, cx), "tunnelUrl": Self::val(&self.tunnel_url, cx) })
    }

    fn first_input(&self, n: usize) -> Option<Entity<w::InputState>> {
        match n { 1 => Some(self.token.clone()), 2 => Some(self.username.clone()), 3 if self.single => Some(self.projects.clone()), 4 => Some(self.tunnel_token.clone()), _ => None }
    }
}

/// A numbered how-to (the wizard's .setup-howto).
fn howto(items: &[&str]) -> Div {
    div().flex().flex_col().gap(T::SP_6).mb(T::SP_12).children(items.iter().enumerate().map(|(i, t)| {
        div().flex().gap(T::SP_8).text_size(T::TEXT_13).text_color(T::text_secondary())
            .child(div().flex_none().text_color(T::text_muted()).child(format!("{}.", i + 1)))
            .child(div().flex_1().min_w_0().child(t.to_string()))
    }))
}
fn label(t: &str) -> Div { div().mt(T::SP_4).text_size(T::TEXT_11).text_color(T::text_muted()).child(t.to_uppercase()) }
fn hint(t: &str) -> Div { div().text_size(T::TEXT_12).text_color(T::text_muted()).child(t.to_string()) }
fn warn(t: &str) -> Div { div().my(T::SP_10).p(T::SP_10).rounded(T::R_CONTROL).border_1().border_color(T::yellow()).bg(T::yellow_dim()).text_size(T::TEXT_12).text_color(T::text()).child(t.to_string()) }

impl Render for LocalScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("local-screen");
        if let Some(n) = self.focus_step.take() {
            if let Some(i) = self.first_input(n) { i.update(cx, |i, cx| i.focus(window, cx)); }
        }
        let l = self.local.read(cx).clone();
        let page = div().flex().flex_col().flex_1().size_full().items_center().justify_center().bg(T::bg());
        if l.wizard { return page.child(self.wizard(cx)).into_any_element(); }

        // The status screen (#status-screen).
        let st = l.status.as_str();
        let color = match st { "starting" => T::yellow(), "running" => T::green(), "error" => T::red(), _ => T::text_muted() };
        let hooks = local::hooks(cx);
        let act = move |f: fn(local::Hooks) -> fn()| move |_: &ClickEvent, _: &mut Window, _: &mut App| if let Some(h) = hooks { f(h)() };
        let log = div().id("local-log").w(T::DIALOG_W_XL).h(T::SP_48 * 4.).p(T::SP_10).overflow_y_scroll()
            .rounded(T::R_CONTROL).border_1().border_color(T::border()).bg(T::bg_secondary())
            .font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_secondary())
            .children(l.logs.iter().rev().take(200).rev().map(|t| div().child(t.clone())));
        page.gap(T::SP_16).px(T::SP_24)
            .child(w::boot_mark())
            .child(div().id("local-status").flex().items_center().gap(T::SP_8).text_size(T::TEXT_14).text_color(T::text_secondary())
                .child(w::dot(color)).child(local::status_label(st)))
            .child(log)
            .child(div().flex().gap(T::SP_8)
                .child(w::disabled(w::button("local-start", "Start"), !(st == "stopped" || st == "error")).on_click(act(|h| h.start)))
                .child(w::disabled(w::button("local-stop", "Stop"), !(st == "running" || st == "starting")).on_click(act(|h| h.stop)))
                .child(w::disabled(w::button("local-restart", "Restart"), st == "stopped").on_click(act(|h| h.restart))))
            .into_any_element()
    }
}

impl LocalScreen {
    fn wizard(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let n = self.step;
        let dots = div().flex().gap(T::SP_6).mb(T::SP_16).children((1..=SETUP_TOTAL).map(|i| {
            div().h(T::SP_4).flex_1().rounded(T::R_PILL).bg(if i < n { T::accent() } else if i == n { T::accent_hover() } else { T::border() })
        }));
        let restore = div().mb(T::SP_16).flex().flex_wrap().gap(T::SP_4).text_size(T::TEXT_12).text_color(T::text_muted())
            .child("Moving from another Crundi?")
            .child(w::link("setup-restore", "Restore from a backup").on_click(cx.listener(|this, _, window, cx| this.save(true, window, cx))))
            .child("instead \u{2014} it brings back the settings, sign-in, projects and everything else.");
        let num = |t: &str| div().text_size(T::TEXT_11).text_color(T::text_muted()).child(t.to_string());
        let h2 = |t: &str| div().mb(T::SP_10).text_size(T::TEXT_20).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(t.to_string());
        let body = match n {
            1 => div().child(num("Step 1 of 7")).child(h2("Create your Telegram bot")).child(howto(&[
                    "Open Telegram and search for @BotFather (the account with the blue checkmark).",
                    "Open the chat, press Start, then send /newbot.",
                    "Choose a name, then a username that ends in bot.",
                    "BotFather replies with a line \"Use this token to access the HTTP API:\" \u{2014} copy that token and paste it below."]))
                .child(label("Telegram Bot Token")).child(w::input(&self.token)),
            2 => div().child(num("Step 2 of 7")).child(h2("Your Telegram username")).child(howto(&[
                    "In Telegram, open Settings (desktop: \u{2630} menu \u{2192} Settings; phone: Settings tab, bottom-right).",
                    "Tap Edit profile \u{2192} Username and set one if you don't have it yet.",
                    "Type it below without the leading @. Only this user may control the bot."]))
                .child(label("Your Telegram Username")).child(w::input(&self.username)),
            3 => {
                let single = self.single;
                let radio = |id: &'static str, t: &'static str, on: bool, v: bool| div().id(id).flex().items_center().gap(T::SP_6).cursor_pointer()
                    .child(w::choice_mark(false, on, false)).child(div().text_size(T::TEXT_13).text_color(T::text()).child(t))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_single(v, cx)));
                div().child(num("Step 3 of 7")).child(h2("Where are your projects?")).child(howto(&[
                        "Single folder: point Crundi at one parent folder \u{2014} every subfolder becomes a project.",
                        "Multiple folders: add projects individually (with full paths) later from the web UI.",
                        "For single folder, press Browse and pick the folder that holds your code."]))
                    .child(label("Project Mode"))
                    .child(div().flex().gap(T::SP_12).mb(T::SP_12).child(radio("setup-mode-single", "Single folder", single, true)).child(radio("setup-mode-multi", "Multiple folders", !single, false)))
                    .when(single, |d| d.child(label("Projects Directory")).child(div().flex().gap(T::SP_8)
                        .child(div().flex_1().child(w::input(&self.projects)))
                        .child(w::button("setup-browse", "Browse").on_click(cx.listener(|this, _, window, cx| this.browse(window, cx))))))
                    .when(!single, |d| d.child(hint("Projects will be added individually with their full paths from the web UI.")))
            }
            4 => div().child(num("Step 4 of 7 \u{b7} optional")).child(h2("Remote access (optional)")).child(howto(&[
                    "Skip this to use a free random quick tunnel \u{2014} Crundi generates a URL each start.",
                    "For a fixed custom domain: open Cloudflare Zero Trust \u{2192} Networks \u{2192} Tunnels \u{2192} Create a tunnel \u{2192} Cloudflared.",
                    "Name it. On the \"Install and run a connector\" screen, only copy the token from the install command (the long eyJ... string) and paste it below.",
                    "Add a Public Hostname (for example crundi.yourdomain.com) with Service type HTTP and URL 127.0.0.1:8888, and enter that full address below. Write 127.0.0.1, not localhost: on Windows localhost can mean the IPv6 address, which Crundi does not listen on, and the page shows \"Bad gateway\" (502)."]))
                .child(warn("Don't install Cloudflare's connector. Cloudflare's page tells you to download/run cloudflared (or a .msi/service) to \"install the connector\" \u{2014} skip that entirely. Crundi already bundles and runs cloudflared itself; installing it again creates a second connector that fights ours and breaks the tunnel. You only need the token and the public hostname URL from that screen."))
                .child(label("Cloudflare Tunnel Token")).child(w::input(&self.tunnel_token))
                .child(label("Tunnel Public URL")).child(w::input(&self.tunnel_url))
                .child(hint("Leave empty (or press Skip) to use a random quick tunnel.")),
            5 => div().child(num("Step 5 of 7")).child(h2("Enable Telegram login"))
                .child(div().mb(T::SP_10).child(hint("The web UI logs in with the \"Log in with Telegram\" button. Telegram only shows it on a domain you've registered for your bot.")))
                .child(howto(&[
                    "Press Save & Start below. When Crundi is running it shows your public URL (your custom domain, or the random quick-tunnel URL in the logs).",
                    "Open @BotFather \u{2192} send /setdomain (or /mybots \u{2192} your bot \u{2192} Bot Settings \u{2192} Domain).",
                    "Select your bot and send that full URL (e.g. https://crundi.yourdomain.com) as the domain."]))
                .child(warn("Use only the bare domain/URL \u{2014} nothing else. Don't add a path, port, or trailing text, and don't enter http://localhost; Telegram rejects those and the login button won't appear. If you use a random quick tunnel, the URL changes every launch, so you'd have to re-run /setdomain each time \u{2014} a fixed custom domain (previous step) is strongly recommended for login.")),
            6 => div().child(num("Step 6 of 7 \u{b7} optional")).child(h2("Locked-screen capture"))
                .child(hint(if cfg!(windows) {
                    "What this is: so Crundi can take screenshots and control the desktop even while the screen is locked, it uses a Windows scheduled task called DisconnectRDP (the tscon trick). It mainly matters when you connect to this PC over Remote Desktop. Creating the task needs administrator rights once."
                } else {
                    "Locked-screen capture uses a Windows-only trick (the DisconnectRDP scheduled task). This machine isn't Windows, so there's nothing to set up here."
                })),
            _ => {
                let (u, la) = (self.auto_update, self.auto_launch);
                let opt = |id: &'static str, on: bool, title: &'static str, sub: &'static str, which: bool| div().my(T::SP_6).p(T::SP_12).rounded(T::R_CARD).border_1().border_color(T::border()).bg(T::bg_tertiary())
                    .child(w::check(id, title, on, cx.listener(move |this, v: &bool, _, cx| if which { this.set_checks(Some(*v), None, cx) } else { this.set_checks(None, Some(*v), cx) })))
                    .child(div().pl(T::SP_24).child(hint(sub)));
                div().child(num("Step 7 of 7")).child(h2("Updates & startup"))
                    .child(hint("A couple of conveniences for the desktop app. You can change both any time in Settings."))
                    .child(opt("setup-auto-update", u, "Automatically check for updates", "Checks GitHub on launch and once a day. Code\u{2011}signed; nothing installs without your confirmation.", true))
                    .child(opt("setup-auto-launch", la, "Launch Crundi when this computer starts", "Starts in the tray so your terminals and services are always reachable.", false))
            }
        };
        let last = n == SETUP_TOTAL;
        let nav = div().flex().items_center().gap(T::SP_8).mt(T::SP_16)
            .child(w::disabled(w::ghost("setup-back", "Back"), n == 1).on_click(cx.listener(|this, _, _, cx| this.back(cx))))
            .child(div().flex_1())
            .when(n == SETUP_SKIPPABLE, |d| d.child(w::ghost("setup-skip", "Skip").on_click(cx.listener(|this, _, window, cx| this.skip(window, cx)))))
            .child(w::primary("setup-next", if last { "Save & Start" } else { "Next" }).on_click(cx.listener(|this, _, window, cx| this.next(window, cx))));
        div().id("setup-card").w(T::DIALOG_W_XL).max_h(relative(0.92)).overflow_y_scroll().p(T::SP_24).rounded(T::R_CARD).border_1().border_color(T::border()).bg(T::bg_secondary())
            .child(dots).child(restore).child(body).child(nav)
    }
}
