//! Sign-in while signed out (the web's #login-screen and #boot, the
//! Electron app's server screen), one cached view:
//! - the start-up screen while a saved login resumes (Mini, the name, "Starting
//!   your workbench" after 1.5 s): the form never flashes for a saved login;
//! - the server step (Electron's #server-screen in the client-only build:
//!   "Server", its note, the address, Save; Cancel when there is one to go back to);
//! - the login step: Mini and the name, "Claude Code terminal in your
//!   browser", the box: its prompt (initLogin's words), the providers
//!   (Telegram, Google, GitHub), OR, password and the 6-digit code, the
//!   collaborator's link and form; first run: set a password (or restore
//!   from a backup, the Operations area's layer: `slots::register_login_restore`),
//!   then the authenticator step ("One more step"); the browser's code step.
//! Enter submits every field's form.

use crate::{local, local_ui::LocalScreen, session::Step, slots, views::BarPress, Stores};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};

pub struct SignIn {
    stores: Stores,
    server_input: Entity<w::InputState>,
    password_input: Entity<w::InputState>,
    code_input: Entity<w::InputState>,
    collab_name: Entity<w::InputState>,
    collab_pass: Entity<w::InputState>,
    setup_pw: Entity<w::InputState>,
    /// "I have added it to my authenticator app" is ticked.
    enrol_saved: bool,
    /// The restore (Operations'): its opener and its form, read once; the
    /// link shows only with one, and the form takes the setup form's place in
    /// the box while open (the web's showLoginRestore, .login-box.restore-mode).
    restore: Option<(std::rc::Rc<dyn Fn(&mut App, &str)>, slots::Draw)>,
    /// The failed sign-ins seen (a new one clears the code, or the passcode).
    failed_seen: u64,
    press: BarPress,
    /// Local mode's status screen and setup wizard (local_ui.rs).
    pub(crate) local_screen: Entity<LocalScreen>,
    /// The server screen's choice in the all-in-one: connect to a server
    /// (else run one on this machine; Electron's #srv-remote).
    srv_remote: bool,
    _subs: Vec<Subscription>,
}

/// The fields' Enter: which form it submits.
#[derive(Clone, Copy)]
enum Submit { Server, Password, Collab, Setup }

impl SignIn {
    pub fn new(stores: Stores, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let server_input = cx.new(|cx| w::InputState::new(window, cx).placeholder("https://crundi.example.com"));
        let password_input = cx.new(|cx| w::InputState::new(window, cx).placeholder("Password").masked(true));
        let code_input = cx.new(|cx| w::digits_state(w::InputState::new(window, cx).placeholder("6-digit code")));
        let collab_name = cx.new(|cx| w::InputState::new(window, cx).placeholder("Your name"));
        let collab_pass = cx.new(|cx| w::InputState::new(window, cx).placeholder("Passcode").masked(true));
        let setup_pw = cx.new(|cx| w::InputState::new(window, cx).placeholder("Choose a password (12+ characters)").masked(true));
        let mut subs = vec![cx.observe_in(&stores.session, window, |this: &mut Self, s, window, cx| {
            // A failed password sign-in: the code is cleared and takes the
            // focus (codes are single-use in practice); a collaborator's: the passcode.
            let (failed, collab) = { let s = s.read(cx); (s.failed_logins, s.collab) };
            if failed != this.failed_seen {
                this.failed_seen = failed;
                let f = if collab { this.collab_pass.clone() } else { this.code_input.clone() };
                f.update(cx, |i, cx| { i.set_value("", window, cx); i.focus(window, cx); });
            }
            cx.notify();
        })];
        let enter = [(&server_input, Submit::Server), (&password_input, Submit::Password), (&code_input, Submit::Password),
            (&collab_name, Submit::Collab), (&collab_pass, Submit::Collab), (&setup_pw, Submit::Setup)];
        for (i, what) in enter {
            subs.push(cx.subscribe_in(i, window, move |this: &mut Self, _, ev: &w::InputEvent, _, cx| {
                if let w::InputEvent::PressEnter { .. } = ev { this.submit(what, cx); }
            }));
        }
        // Mini and the name: decoding now; drawn again once (a cached view
        // first shown before they are ready would not hear that they are).
        w::warm_marks(window, cx);
        let t = cx.background_executor().timer(std::time::Duration::from_millis(500));
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| { t.await; let _ = this.update(cx, |_, cx| cx.notify()); }).detach();
        // The restore's form is drawn in the box: redrawn when its state changes.
        let restore = slots::restore(cx).map(|(watch, open, draw)| {
            let me = cx.entity().downgrade();
            subs.push(watch(cx, Box::new(move |cx| { let _ = me.update(cx, |_, cx| cx.notify()); })));
            (open, draw)
        });
        let local_screen = cx.new(|cx| LocalScreen::new(window, cx));
        let lo = local::local(cx);
        subs.push(cx.observe(&lo, |_, _, cx| cx.notify()));
        let srv_remote = local::client_config(lo.read(cx).bundled).0 == "remote";
        SignIn { local_screen, srv_remote, stores, server_input, password_input, code_input, collab_name, collab_pass, setup_pw, enrol_saved: false, restore, failed_seen: 0, press: BarPress::default(), _subs: subs }
    }

    fn submit(&mut self, what: Submit, cx: &mut Context<Self>) {
        if self.stores.session.read(cx).signin_busy { return; }
        let val = |e: &Entity<w::InputState>, cx: &App| e.read(cx).value().to_string();
        match what {
            Submit::Server => {
                let v = val(&self.server_input, cx);
                // The all-in-one's choice (Electron's client:setConfig): run a
                // server here, or connect to one; either way for the next start.
                if local::local(cx).read(cx).bundled {
                    let t = v.trim().to_string();
                    let ok = !self.srv_remote || { let l = t.to_ascii_lowercase(); l.starts_with("http://") || l.starts_with("https://") };
                    if !ok {
                        self.stores.session.update(cx, |s, cx| { s.server_note = "Enter the full server address, including http:// or https://".into(); cx.notify(); });
                        return;
                    }
                    let saved = local::write_client_config(if self.srv_remote { "remote" } else { "local" }, &t);
                    if !self.srv_remote || self.stores.session.read(cx).server_over {
                        if self.srv_remote { crundi_core::store::update(|st| { let b = crundi_core::api::normalize_base(&t); if st.server != b { st.refresh_token.clear(); } st.server = b; }); }
                        let note = if saved { "Saved. Restart Crundi for it to take effect." } else { "Could not save the setting" };
                        self.stores.session.update(cx, |s, cx| { s.server_note = note.into(); cx.notify(); });
                        return;
                    }
                }
                // Over the signed-in app: kept for the next start (F131).
                if self.stores.session.read(cx).server_over { self.stores.session.update(cx, |s, cx| s.save_server_over(&v, cx)); }
                else { self.stores.session.update(cx, |s, cx| s.check_server(&v, cx)); }
            }
            Submit::Password => {
                let (p, c) = (val(&self.password_input, cx), val(&self.code_input, cx));
                self.stores.session.update(cx, |s, cx| { let srv = s.signin_server.clone(); s.sign_in_password(&srv, &p, &c, cx) });
            }
            Submit::Collab => {
                let (n, p) = (val(&self.collab_name, cx), val(&self.collab_pass, cx));
                self.stores.session.update(cx, |s, cx| s.sign_in_collab(&n, &p, cx));
            }
            Submit::Setup => {
                let p = val(&self.setup_pw, cx);
                self.stores.session.update(cx, |s, cx| s.set_up_password(&p, cx));
            }
        }
    }
}

/// The login box's prompt (the web's initLogin; in this app Telegram is a
/// button like Google and GitHub).
pub fn prompt(setup: bool, telegram: bool, google: bool, github: bool, password: bool) -> &'static str {
    if setup { "Set up a way to sign in" }
    else if telegram || google || github { "Sign in to continue" }
    else if password { "Sign in with your password and authenticator code" }
    else { "Sign in with your Telegram account to continue" }
}

/// A column of a form in the box (the web's 260 wide forms).
fn form(w_: Pixels) -> Div { div().flex().flex_col().gap(T::SP_10).w(w_) }

impl SignIn {
    /// The server box with the saved address (the server step over the app).
    pub(crate) fn fill_server(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let srv = crundi_core::store::load().server;
        self.server_input.update(cx, |st, cx| st.set_value(srv, window, cx));
        self.srv_remote = local::client_config(local::local(cx).read(cx).bundled).0 == "remote";
    }

    pub(crate) fn srv_remote_now(&self) -> bool { self.srv_remote }

    /// The server screen's choice (the driver's "server-mode").
    pub(crate) fn set_srv_remote(&mut self, remote: bool, cx: &mut Context<Self>) { self.srv_remote = remote; cx.notify(); }
}

impl Render for SignIn {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("sign-in");
        let s = self.stores.session.read(cx);
        let (step, err, busy, srv) = (s.step.clone(), s.signin_error.clone(), s.signin_busy, s.signin_server.clone());
        let (code, google, github, telegram, password) = (s.oauth_code.clone(), s.offer_google, s.offer_github, s.offer_telegram, s.offer_password);
        let (setup, enrol, collab, boot_slow, base) = (s.setup_required, s.enrol.clone(), s.collab, s.boot_slow, s.signin_base());
        let (over, note) = (s.server_over, s.server_note.clone());
        let had_server = !srv.is_empty();
        let session = self.stores.session.clone();
        let error = div().min_h(T::SP_12 + T::SP_2).text_size(T::TEXT_12).text_color(T::red()).child(err.clone());

        // The window's own bar: drag to move; minimise, maximise, close.
        let bar = div().id("signin-bar").flex().flex_none().items_center().h(T::TOPBAR_H).px(T::SP_4).justify_end()
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, _, _| this.press.0 = Some(e.position)))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, _| this.press.drag(e, window)))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.press.0 = None))
            .child(w::icon_button("win-min", IconName::Minus, "Minimize").on_click(|_, window, _| window.minimize_window()))
            .child(w::icon_button("win-max", IconName::Square, "Maximize").on_click(|_, window, _| window.zoom_window()))
            .child(w::icon_button("win-close", IconName::X, "Close").on_click(|_, window, cx| crate::close_window(window, cx)));
        let page = div().flex().flex_col().size_full().bg(T::bg()).child(bar);

        // Local mode: the status screen or the setup wizard (index.html), until
        // its server is up and signed in to.
        let lo = local::local(cx).read(cx).clone();
        if lo.active && !over {
            return page.child(self.local_screen.clone().cached(StyleRefinement::default().flex_1().w_full().min_h_0())).into_any_element();
        }

        // The start-up screen (#boot).
        if step == Step::Boot {
            return page.child(div().flex().flex_col().flex_1().items_center().justify_center().gap(T::SP_20 + T::SP_2)
                .child(w::boot_mark())
                .child(div().min_h(T::SP_16).font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_secondary())
                    .child(if boot_slow { "Starting your workbench" } else { "" })))
                .into_any_element();
        }

        // The server step (Electron's #server-screen, the client-only build).
        if step == Step::Server || over {
            let noted = !note.is_empty();
            let (err, had_server) = if over { (note, true) } else if noted { (note, had_server) } else { (err, had_server) };
            let (bundled, remote) = (lo.bundled, self.srv_remote);
            // Where the card and its parts are drawn (the driver's "signin.rects":
            // nothing may reach past the card's padding, at any width).
            crate::spots::begin(cx, "server-screen");
            let mut spot = |name: &str| crate::spots::spot(cx, "server-screen", format!("srv:{name}"));
            let (sp_card, sp_local, sp_remote, sp_input) = (spot("card"), spot("srv-local"), spot("srv-remote"), spot("input"));
            // The option's words take the room beside the mark and wrap there
            // (index.html's label: flex, the text a block that wraps).
            let radio = |id: &'static str, title: &'static str, sub: &'static str, on: bool, v: bool, sp: Canvas<()>| div().id(id).flex().items_start().gap(T::SP_8).mb(T::SP_8).cursor_pointer()
                .child(w::choice_mark(false, on, false))
                .child(div().relative().flex().flex_col().flex_1().min_w_0().child(div().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(title))
                    .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child(sub)).child(sp))
                .on_click(cx.listener(move |this, _, _, cx| this.set_srv_remote(v, cx)));
            let card = w::card().relative().w(T::DIALOG_W_LG).max_w(window.viewport_size().width * 0.9).p(T::SP_24).gap(T::SP_6).bg(T::bg_secondary()).child(sp_card)
                .child(div().text_size(T::TEXT_20).font_weight(T::W_SEMIBOLD).text_color(T::text()).child("Server"))
                .child(div().mb(T::SP_10).text_size(T::TEXT_13).text_color(T::text_muted())
                    .child("Crundi can run its own server on this machine, or connect to one running elsewhere — a container on your network, for instance. Either way the browser panel opens here, in front of you."))
                .when(bundled, |d| d
                    .child(radio("srv-local", "Run a server on this machine", "The standard setup. Everything local.", !remote, false, sp_local))
                    .child(radio("srv-remote", "Connect to a server", "Your projects and Claude live there; this app is the window onto them.", remote, true, sp_remote)))
                .child(div().relative().when(bundled && !remote, |d| d.opacity(0.45)).child(w::input(&self.server_input)).child(sp_input))
                .child(div().min_h(T::SP_16).mb(T::SP_6).text_size(T::TEXT_12).text_color(if err.is_empty() || over || noted { T::text_muted() } else { T::red() }).child(err.clone()))
                .child(div().flex().gap(T::SP_8)
                    .child(w::disabled(w::primary("srv-save", "Save"), busy).on_click(cx.listener(|this, _, _, cx| this.submit(Submit::Server, cx))))
                    .when(had_server, |d| d.child(w::button("srv-cancel", "Cancel").on_click({ let session = session.clone(); move |_, _, cx| {
                        // Over the app: back to it (Electron's hideServerScreen).
                        if session.read(cx).server_over { return session.update(cx, |s, cx| s.show_server_over(false, cx)); }
                        let srv = crundi_core::store::load().server;
                        session.update(cx, |s, cx| s.check_server(&srv, cx));
                    } }))));
            return page.child(div().flex().flex_1().items_center().justify_center().child(card)).into_any_element();
        }

        let mut boxed = div().flex().flex_col().items_center().gap(T::SP_24).px(T::SP_48).py(T::SP_40)
            .rounded(T::R_CARD).border_1().border_color(T::border()).bg(T::bg_secondary()).shadow_lg();
        let p = |t: &str| div().text_size(T::TEXT_14).text_color(T::text_secondary()).child(t.to_string());

        if !code.is_empty() {
            // Signing in in the browser: the code to compare.
            boxed = boxed
                .child(p("Finish signing in in your browser. Check that it shows this code:"))
                .child(div().flex().justify_center().font_family(T::MONO).text_size(T::TEXT_26).font_weight(T::W_BOLD).text_color(T::accent_hover()).child(code))
                .child(w::button("cancel-sign-in", "Cancel").on_click({ let session = session.clone(); move |_, _, cx| session.update(cx, |s, cx| s.cancel_browser(cx)) }));
        } else if let Some((secret, uri)) = enrol {
            // Right after the password is set (#setup-enrol).
            let saved = self.enrol_saved;
            let u = uri.clone();
            boxed = boxed.child(p("One more step")).child(form(T::DIALOG_W - T::SP_48 - T::SP_12)
                .child(div().font_weight(T::W_SEMIBOLD).text_color(T::text()).child("Add this to your authenticator app"))
                .child(div().text_size(T::TEXT_13).text_color(T::text_secondary())
                    .child("Signing in needs a 6-digit code as well as the password. Save this now — it is shown once, and without it you will not be able to sign in again."))
                .child(div().id("setup-secret").p(T::SP_10).rounded(T::R_CONTROL).border_1().border_color(T::border()).bg(T::bg())
                    .font_family(T::MONO).text_size(T::TEXT_15).text_color(T::text()).flex().justify_center().child(secret))
                .child(w::link("setup-uri", uri).on_click(move |_, _, _| { let _ = crate::session::open_url(&u); }))
                .child(w::check("setup-saved", "I have added it to my authenticator app", saved, cx.listener(|this, v: &bool, _, cx| { this.enrol_saved = *v; cx.notify(); })))
                .child(w::disabled(w::primary("setup-done", "Continue"), !saved).on_click({ let session = session.clone(); move |_, _, cx| session.update(cx, |s, cx| s.finish_enrol(cx)) })));
        } else if let Some(lr) = self.restore.clone().filter(|_| setup).and_then(|(_, draw)| draw(window, cx)) {
            // First run, the other way (#login-restore): the box holds the
            // restore form, titled for it (showLoginRestore(true)).
            boxed = boxed.child(p("Restore from a backup")).child(lr);
        } else if setup {
            // First run (#login-setup): nothing else works until a way in is set.
            let restore = self.restore.clone().map(|r| r.0);
            boxed = boxed.child(p(prompt(true, false, false, false, false))).child(form(T::DIALOG_W - T::SP_48 - T::SP_32)
                .child(div().text_size(T::TEXT_13).text_color(T::yellow())
                    .child("No sign-in method is set up yet. Choose one now — nothing else on this server works until you do."))
                .child(w::input(&self.setup_pw))
                .child(w::disabled(w::primary("setup-submit", "Set password"), busy).on_click(cx.listener(|this, _, _, cx| this.submit(Submit::Setup, cx))))
                .child(error))
                .when_some(restore, |d, open| d.child(w::link("setup-restore-link", "Restore from a backup instead").on_click(move |_, _, cx| open(cx, &base))));
        } else if collab {
            // The collaborator's way in, instead of the owner's (#collab-login).
            boxed = boxed.child(form(T::DIALOG_W - T::SP_48 - T::SP_48 - T::SP_24)
                .child(div().flex().justify_center().text_size(T::TEXT_15).font_weight(T::W_SEMIBOLD).text_color(T::text()).child("Invited to a project"))
                .child(w::input(&self.collab_name))
                .child(w::input(&self.collab_pass))
                .child(w::disabled(w::primary("collab-login-submit", "Sign in"), busy).on_click(cx.listener(|this, _, _, cx| this.submit(Submit::Collab, cx))))
                .child(error)
                .child(div().flex().justify_center().child(w::quiet_link("collab-login-back", "Back to the normal sign-in")
                    .on_click({ let session = session.clone(); move |_, _, cx| session.update(cx, |s, cx| s.show_collab(false, cx)) }))));
        } else {
            let oauth = |id: &'static str, label: &'static str, provider: &'static str| {
                let session = session.clone();
                w::disabled(w::button(id, label), busy).w_full().on_click(move |_, _, cx| session.update(cx, |s, cx| s.sign_in_oauth(provider, cx)))
            };
            let any_button = telegram || google || github;
            boxed = boxed.child(p(prompt(false, telegram, google, github, password)))
                .when(any_button, |d| d.child(form(T::DIALOG_W - T::SP_48 - T::SP_48 - T::SP_24)
                    .when(telegram, |d| d.child(oauth("oauth-btn-telegram", "Sign in with Telegram", "telegram")))
                    .when(google, |d| d.child(oauth("oauth-btn-google", "Sign in with Google", "google")))
                    .when(github, |d| d.child(oauth("oauth-btn-github", "Sign in with GitHub", "github")))))
                .when(any_button && password, |d| d.child(div().id("login-or").text_size(T::TEXT_12).text_color(T::text_muted()).child("OR")))
                .when(password, |d| d.child(form(T::DIALOG_W - T::SP_48 - T::SP_48 - T::SP_24)
                    .child(w::input(&self.password_input))
                    .child(w::code_input(&self.code_input))
                    .child(w::disabled(w::primary("login-submit", "Sign in"), busy).on_click(cx.listener(|this, _, _, cx| this.submit(Submit::Password, cx))))))
                .child(error)
                .child(w::quiet_link("collab-login-link", "I was invited to a project")
                    .on_click({ let session = session.clone(); move |_, _, cx| session.update(cx, |s, cx| s.show_collab(true, cx)) }));
        }
        // Which server, and the way back to the server step (this app's own: it
        // signs in to any server).
        let which = div().flex().items_center().gap(T::SP_6).text_size(T::TEXT_12).text_color(T::text_muted())
            .child(srv).child(w::quiet_link("change-server", "Change").on_click({ let session = session.clone(); move |_, _, cx| session.update(cx, |s, cx| s.change_server(cx)) }));
        page.child(div().id("login-screen").flex().flex_col().flex_1().items_center().justify_center().gap(T::SP_32).overflow_y_scroll().py(T::SP_20)
            .child(w::sign_in_mark())
            .child(div().mt(-T::SP_20).text_size(T::TEXT_15).text_color(T::text_secondary()).child("Claude Code terminal in your browser"))
            .child(boxed)
            .child(which))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::prompt;
    #[test]
    fn the_prompt_follows_what_the_server_offers() {
        assert_eq!(prompt(true, true, true, true, true), "Set up a way to sign in");
        assert_eq!(prompt(false, false, true, false, true), "Sign in to continue");
        assert_eq!(prompt(false, false, false, false, true), "Sign in with your password and authenticator code");
        assert_eq!(prompt(false, false, false, false, false), "Sign in with your Telegram account to continue");
    }
}
