//! A transcript's header: the web's chat page header (#cp-head,
//! src/chat-page.js:37-57, 119-125; app/vendor/crundi-chat-page.js) and the
//! secret approval cards under it (#cp-ask, :88-99; crundi-chat-page.js:
//! 281-348).
//!
//! The head: back, the chat's mark (the project's icon, or the chat's
//! initials on the project's colour → colorDeep gradient), the title (a box
//! to type in while renaming), the state's dot (pulsing while it works) with
//! the project and the state's word, the secrets lock with its count (when
//! the sheet is 600 px or wider), and ⋯ (Rename chat, Secrets for this chat
//! when narrow, Stop Claude, Open in Crundi).
//!
//! Views: ChatHead (redrawn on the chat's meta, its project's icon, the
//! secrets' count, the sheet crossing 600 px), its dot (a leaf: its pulse
//! redraws only while the chat works and the head is on show), and AskCards
//! (redrawn when the waiting requests change).

use super::act as fact;
use crundi_core::sreq::{self, Req};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::{cell::Cell, rc::Rc, time::Duration};

/// The sheet's width from which the lock is in the head (narrower, it is a
/// line of the menu): the web's @media (min-width:600px).
const WIDE: f32 = 600.;
/// #cp-mark and .cp-btn: 38 px.
fn btn_size() -> Pixels { T::SP_32 + T::SP_6 }

// ─── The waiting requests (one list for every transcript) ───

/// The secret requests waiting on the owner (the `secret-requests` event,
/// or GET /api/secrets once when a transcript first opens).
#[derive(Default)]
pub struct Asks { pub list: Vec<Req>, loaded: bool }
struct AsksHandle(Entity<Asks>);
impl Global for AsksHandle {}

fn asks(cx: &mut App) -> Entity<Asks> {
    if let Some(h) = cx.try_global::<AsksHandle>() { return h.0.clone(); }
    let e = cx.new(|_| Asks::default());
    cx.set_global(AsksHandle(e.clone()));
    e
}

fn set_asks(v: &Value, cx: &mut App) {
    let list = sreq::parse(v.get("requests").unwrap_or(&Value::Null));
    asks(cx).update(cx, |a, cx| { a.loaded = true; if a.list != list { a.list = list; cx.notify(); } });
}

/// The `secret-requests` event (route_chat).
pub(crate) fn on_requests(v: &Value, cx: &mut App) { set_asks(v, cx); }

fn load_asks(cx: &mut App) {
    crundi_shell::call(cx, |api| api.get("/api/secrets"), |r, cx| if let Ok(v) = r { set_asks(&v, cx); });
}

// ─── The head ───

pub struct ChatHead {
    meta: Entity<crundi_chat::store::ChatMeta>,
    store: Entity<crundi_chat::store::ChatStore>,
    id: String,
    pub(super) back: &'static str,
    pub(super) project: String,
    dot: Entity<CpDot>,
    pub asks: Entity<AskCards>,
    /// The sheet's width as last drawn (the lock shows from WIDE).
    width: Rc<Cell<f32>>,
    wide: bool,
    rename: Option<Entity<w::InputState>>,
    icon: Option<std::sync::Arc<Image>>,
    /// The chat's name as the server lists it (loadMeta: /api/ui-sessions'
    /// longTitle, else title; the event stream's list when it has the chat).
    listed: String,
    /// The transcript's log (⋯'s "Show the plan" toggles its plan panel).
    pub plan_log: Option<WeakEntity<crundi_chat::log::LogView>>,
    _subs: Vec<Subscription>,
}

/// The chat's name in the event stream's list (longTitle, else title).
fn listed_title(p: &crundi_shell::Projects, id: &str) -> Option<String> {
    let t = p.terminals.iter().find(|t| t.get("id").and_then(|i| i.as_str()) == Some(id))?;
    let f = |k: &str| t.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let l = f("longTitle");
    Some(if l.is_empty() { f("title") } else { l })
}

impl ChatHead {
    pub fn new(store: Entity<crundi_chat::store::ChatStore>, id: &str, back: &'static str, project: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let meta = store.read(cx).meta.clone();
        let projects = crundi_shell::stores(cx).projects.clone();
        let secrets = crundi_chat::composer::secrets_entity(cx);
        let st = meta.read(cx).0.state.clone();
        let dot = cx.new(|_| CpDot { state: st });
        let asks_v = cx.new(|cx| AskCards::new(id, project, window, cx));
        let subs = vec![
            cx.observe(&meta, |this, m, cx| {
                let st = m.read(cx).0.state.clone();
                this.dot.update(cx, |d, cx| if d.state != st { d.state = st; cx.notify(); });
                cx.notify();
            }),
            // Only the project's icon redraws the head (not every state event).
            // and a rename (the list's title).
            cx.observe(&projects, |this, p, cx| {
                let i = p.read(cx).icon_of(&this.project);
                if i.as_ref().map(|x| x.id) != this.icon.as_ref().map(|x| x.id) { this.icon = i; cx.notify(); }
                if let Some(t) = listed_title(p.read(cx), &this.id).filter(|t| !t.is_empty() && *t != this.listed) { this.listed = t; cx.notify(); }
            }),
            cx.observe(&secrets, |_, _, cx| cx.notify()),
        ];
        let icon = projects.read(cx).icon_of(project);
        let listed = listed_title(projects.read(cx), id).unwrap_or_default();
        if !asks(cx).read(cx).loaded { load_asks(cx); }
        let me = cx.entity().downgrade();
        if listed.is_empty() { load_title(id, me, cx); }
        ChatHead { meta, store, id: id.to_string(), back, project: project.to_string(), dot, asks: asks_v, width: Rc::new(Cell::new(0.)), wide: false, rename: None, icon, listed, plan_log: None, _subs: subs }
    }

    /// The name shown: the list's, else the chat's own, else "Chat".
    fn title(&self, cx: &App) -> String {
        if !self.listed.is_empty() { return self.listed.clone(); }
        let t = &self.meta.read(cx).0.title;
        if t.is_empty() { "Chat".into() } else { t.clone() }
    }

    /// The project changed under the head (another run's transcript).
    pub fn set_place(&mut self, back: &'static str, project: &str, cx: &mut Context<Self>) {
        if self.back == back && self.project == project { return; }
        self.back = back;
        self.project = project.to_string();
        self.icon = crundi_shell::stores(cx).projects.read(cx).icon_of(project);
        let p = project.to_string();
        self.asks.update(cx, |a, cx| { a.project = p; cx.notify(); });
        cx.notify();
    }

    /// Rename chat: the title becomes a box to type in (renameHere).
    fn start_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.rename.is_some() { return; }
        let title = self.title(cx);
        let input = cx.new(|cx| w::InputState::new(window, cx));
        input.update(cx, |s, cx| {
            s.set_value(title.clone(), window, cx);
            let n = title.len();
            s.set_selected_range(0..n, cx);
            s.focus(window, cx);
        });
        self._subs.push(cx.subscribe_in(&input, window, |this, _, ev: &w::InputEvent, _, cx| match ev {
            w::InputEvent::PressEnter { .. } | w::InputEvent::Blur => this.finish_rename(true, cx),
            _ => {}
        }));
        self.rename = Some(input);
        cx.notify();
    }

    fn finish_rename(&mut self, save: bool, cx: &mut Context<Self>) {
        let Some(input) = self.rename.take() else { return };
        cx.notify();
        let next: String = input.read(cx).value().trim().chars().take(80).collect();
        let cur = self.title(cx);
        if !save || next.is_empty() || next == cur { return; }
        let path = format!("/api/ui-sessions/{}/rename", self.id);
        let (id, me) = (self.id.clone(), cx.entity().downgrade());
        crundi_shell::call(cx, move |api| api.post(&path, &json!({ "title": next })), move |r, cx| {
            let err = match r { Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => Some(v.get("error").and_then(|e| e.as_str()).unwrap_or("Could not rename it").to_string()), Ok(_) => None, Err(_) => Some("Could not rename it".to_string()) };
            match err { Some(e) => crundi_shell::toast(cx, e, w::Toast::Error), None => load_title(&id, me, cx) }
        });
    }

    /// ⋯: the page's menu (#cp-menu), under the button at the head's right.
    fn open_menu(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let working = self.meta.read(cx).0.state == "working";
        let n = crundi_chat::composer::secrets_count(&self.id, cx);
        let item = |id: &str, label: String, disabled: bool| w::MenuEntry { id: id.into(), label: label.into(), icon: None, danger: false, sep: false, disabled };
        let mut items = vec![];
        // "Show the plan", while the chat has one (the page's hasPlan).
        if self.meta.read(cx).0.plan.is_some() && self.plan_log.is_some() { items.push(item("plan", "Show the plan".into(), false)); }
        items.push(item("rename", "Rename chat".into(), false));
        // The lock's line, when the head has no room for the lock.
        if let Some(n) = n.filter(|_| !self.wide) { items.push(item("secrets", if n > 0 { format!("Secrets for this chat ({n})") } else { "Secrets for this chat".into() }, false)); }
        items.push(item("stop", "Stop Claude".into(), !working));
        items.push(item("open", "Open in Crundi".into(), false));
        let me = cx.entity().downgrade();
        w::menu_at(cx, at, items, move |id, window, cx| { let _ = me.update(cx, |this, cx| this.menu_pick(&id, window, cx)); });
    }

    fn menu_pick(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        match id {
            "rename" => self.start_rename(window, cx),
            // view.togglePlan(): the transcript's plan panel opens or closes.
            "plan" => if let Some(l) = self.plan_log.as_ref().and_then(|l| l.upgrade()) { l.update(cx, |v, cx| crundi_chat::log::goal::toggle(v, cx)); },
            "secrets" => crundi_chat::composer::open_secrets(&self.id, window, cx),
            "stop" => self.store.update(cx, |s, cx| s.stop(cx)),
            // openMain('/chat/<id>'): the chat in Crundi itself.
            "open" => { let id = self.id.clone(); cx.defer(move |cx| super::view::open_in_crundi(&id, cx)); }
            _ => {}
        }
    }

    /// For the driver (factories-transcript-head <what> <arg>): "menu" opens
    /// ⋯ (then menu-pick), "rename" types <arg> as the new name and presses
    /// Enter, "rename-escape" starts a rename and lets it go.
    pub(super) fn drive(&mut self, what: &str, arg: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match what {
            "menu" => { self.open_menu(point(px(400.), px(60.)), cx); true }
            "rename" | "rename-escape" => {
                self.start_rename(window, cx);
                if let Some(i) = self.rename.clone() { let v = arg.to_string(); i.update(cx, |s, cx| s.set_value(v, window, cx)); }
                self.finish_rename(what == "rename", cx);
                true
            }
            // The secret requests' cards: the list as the server would send
            // it (ask-set <requests JSON>), the box typed, a card's Reject.
            "ask-set" => { let v: Value = serde_json::from_str(arg).unwrap_or(Value::Null); set_asks(&json!({ "requests": v }), cx); true }
            "ask-type" => { let v = arg.to_string(); self.asks.update(cx, |a, cx| a.pin.update(cx, |s, cx| s.set_value(v, window, cx))); true }
            "ask-reject" => { let id = arg.to_string(); self.asks.update(cx, |a, cx| a.reject(&id, window, cx)); true }
            _ => false,
        }
    }

    /// For the driver: what the head shows.
    pub fn props(&self, cx: &App) -> Value {
        let m = &self.meta.read(cx).0;
        json!({ "title": self.title(cx), "state": m.state, "wide": self.wide, "renaming": self.rename.is_some(),
            "lock": crundi_chat::composer::secrets_count(&self.id, cx).filter(|_| self.wide), "icon": self.icon.is_some(),
            "mark": crundi_core::head::chat_mark(&self.project, &m.title).0, "asks": self.asks.read(cx).props(cx) })
    }
}

impl Render for ChatHead {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("fx-cp-head");
        let title = self.title(cx);
        let m = &self.meta.read(cx).0;
        let word = match m.state.as_str() { "working" => "Working", "needs-input" => "Needs you", "waiting" => "Waiting", "idle" => "Ready", _ => "" };
        let sub = format!("{}{}{}", self.project, if !word.is_empty() && !self.project.is_empty() { " · " } else { "" }, word);
        let back = self.back;
        let (ini, c0, c1) = crundi_core::head::chat_mark(&self.project, &m.title);
        let wide = self.width.get() >= WIDE;
        self.wide = wide;
        let lock = crundi_chat::composer::secrets_count(&self.id, cx).filter(|_| wide);
        let s = btn_size();
        let mark = div().flex_none().size(s).rounded(T::R_BUBBLE).overflow_hidden().flex().items_center().justify_center()
            .text_size(T::TEXT_15).font_weight(T::W_BOLD).text_color(T::white())
            .map(|d| match self.icon.clone() {
                Some(i) => d.bg(T::bg()).child(img(i).size_full().rounded(T::R_BUBBLE).object_fit(ObjectFit::Cover)),
                None => d.bg(linear_gradient(135., linear_color_stop(Hsla::from(rgb(c0)), 0.), linear_color_stop(Hsla::from(rgb(c1)), 1.))).child(ini),
            });
        let title_el = match &self.rename {
            Some(input) => div().on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| if e.keystroke.key == "escape" { this.finish_rename(false, cx); }))
                .child(w::input(input).h(T::SP_24).text_size(T::TEXT_15).font_weight(T::W_SEMIBOLD)).into_any_element(),
            None => div().truncate().text_size(T::TEXT_15).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(title).into_any_element(),
        };
        // The sheet's width, read as it is laid out: crossing WIDE draws the head again.
        let (width, me) = (self.width.clone(), cx.entity_id());
        let probe = canvas(move |b, _, cx| {
            let w0 = f32::from(b.size.width);
            let was = width.replace(w0);
            if (was >= WIDE) != (w0 >= WIDE) { cx.notify(me); }
        }, |_, _, _, _| {}).absolute().size_full();
        div().relative().w_full().flex().flex_none().items_center().gap(T::SP_10).px(T::SP_10).py(T::SP_8).bg(T::bg_secondary()).border_b_1().border_color(T::border_subtle())
            .child(probe)
            .child(w::icon_button("fx-cp-back", IconName::ArrowLeft, "Back").on_click(move |_, _, cx| fact(back, "", "", cx)))
            .child(mark)
            .child(div().flex().flex_col().flex_1().min_w_0()
                .child(title_el)
                .child(div().flex().items_center().gap(T::SP_6).min_w_0().text_size(T::TEXT_12).text_color(T::text_secondary())
                    .child(self.dot.clone())
                    .child(div().truncate().child(sub))))
            .when_some(lock, |d, n| {
                let tip = crundi_chat::composer::secrets_tip(n);
                d.child(div().relative().flex_none()
                    .child(w::icon_button("fx-cp-lock", IconName::Lock, tip).when(n > 0, |b| b.text_color(T::yellow()))
                        .on_click(cx.listener(|this, _, window, cx| crundi_chat::composer::open_secrets(&this.id, window, cx))))
                    .when(n > 0, |d| d.child(div().absolute().top(px(-4.)).right(px(-4.)).min_w(px(14.)).h(px(14.)).px(px(3.)).rounded(px(7.))
                        .bg(T::yellow()).text_color(T::bg()).text_size(T::TEXT_10).font_weight(T::W_BOLD).flex().items_center().justify_center()
                        .child(n.to_string()))))
            })
            .child(w::icon_button("fx-cp-more", IconName::EllipsisVertical, "More").on_click(cx.listener(|this, e: &ClickEvent, _, cx| {
                // #cp-menu: right 8 px, under the head; the pointer's row when the click had none.
                let p = e.position();
                this.open_menu(point(p.x - px(190.), p.y + T::SP_16), cx);
            })))
    }
}

/// loadMeta: the chat's name from /api/ui-sessions (longTitle, else title).
fn load_title(id: &str, me: WeakEntity<ChatHead>, cx: &mut App) {
    let id = id.to_string();
    crundi_shell::call(cx, |api| api.get("/api/ui-sessions"), move |r, cx| {
        let Ok(v) = r else { return };
        let Some(s) = v.get("sessions").and_then(|a| a.as_array()).and_then(|a| a.iter().find(|x| x.get("id").and_then(|i| i.as_str()) == Some(id.as_str()))) else { return };
        let f = |k: &str| s.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let t = if f("longTitle").is_empty() { f("title") } else { f("longTitle") };
        let _ = me.update(cx, |h, cx| if !t.is_empty() && t != h.listed { h.listed = t; cx.notify(); });
    });
}

// ─── The state's dot (#cp-dot) ───

/// The dot: its colour from the state; while working it pulses (cp-pulse:
/// 1.2 s, 50%: 0.35), at most T::ANIM_FPS and only while it is drawn.
pub struct CpDot { state: String }
impl Render for CpDot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let c = match self.state.as_str() { "working" => T::amber(), "needs-input" => T::accent_hover(), "idle" => T::green(), _ => T::text_muted() };
        let d = div().flex_none().size(px(7.)).rounded_full().bg(c);
        // (with_animation keeps still under the system's reduced motion.)
        if self.state == "working" {
            d.with_animation("cp-pulse", Animation::new(Duration::from_millis(1200)).repeat().with_max_fps(T::ANIM_FPS),
                |d, t| { let k = (1.0 - (std::f32::consts::TAU * t).cos()) / 2.0; d.opacity(1.0 - 0.65 * k) }).into_any_element()
        } else { d.into_any_element() }
    }
}

// ─── The approval cards (#cp-ask) ───

pub struct AskCards {
    id: String,
    project: String,
    /// The card with the PIN box (the first, until Approve picks another).
    armed: String,
    /// Armed by itself (the first request): the box doesn't take the keys.
    auto: bool,
    err: String,
    pin: Entity<w::InputState>,
    /// The box's word ('PIN' or 'Passphrase', the armed request's lock).
    ph: &'static str,
    /// The requests shown, as last drawn (a change of them redraws).
    shown: Vec<Req>,
    /// "Allow for this chat" on the open card (the web's askAllow): ticked,
    /// and for how long (a key of sreq::ALLOW_FOR); a new card starts unticked.
    allow: (bool, &'static str),
    /// The server pre-authorises ('chat-secret-preauth').
    preauth: bool,
    _subs: Vec<Subscription>,
}

impl AskCards {
    fn new(id: &str, project: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let pin = cx.new(|cx| w::InputState::new(window, cx).masked(true).placeholder("PIN"));
        let a = asks(cx);
        let subs = vec![
            cx.observe_in(&a, window, |this, _, window, cx| this.sync(window, cx)),
            cx.subscribe_in(&pin, window, |this, _, ev: &w::InputEvent, window, cx| if let w::InputEvent::PressEnter { .. } = ev { this.approve(window, cx); }),
        ];
        let mut me = AskCards { id: id.to_string(), project: project.to_string(), armed: String::new(), auto: false, err: String::new(), pin, ph: "PIN", shown: vec![], allow: (false, "1h"), preauth: false, _subs: subs };
        me.sync(window, cx);
        // (After this update: the Factories tab, which keeps the server's
        // capabilities, may be the one making this card.)
        let w8 = cx.entity().downgrade();
        cx.defer(move |cx| super::caps(cx, move |caps, cx| {
            let yes = caps.iter().any(|c| c == "chat-secret-preauth");
            let _ = w8.update(cx, |this, cx| if this.preauth != yes { this.preauth = yes; cx.notify(); });
        }));
        me
    }

    /// "Allow for this chat" is offered (CS.isSupported, a run this chat
    /// asked for, not a collaborator's chat).
    fn can_allow(&self, r: &Req, cx: &App) -> bool {
        let collab = crundi_shell::stores(cx).projects.read(cx).terminals.iter()
            .find(|t| t.get("id").and_then(|i| i.as_str()) == Some(self.id.as_str()))
            .and_then(|c| c.get("collaborator").cloned()).is_some_and(|v| v.as_bool().unwrap_or(!v.is_null()));
        sreq::can_allow(r, self.preauth, collab) && r.session == self.id
    }

    /// Mine: asked by this chat, or by no chat in this project (loadAsks).
    fn mine(&self, cx: &App) -> Vec<Req> {
        let Some(h) = cx.try_global::<AsksHandle>() else { return vec![] };
        h.0.read(cx).list.iter().filter(|r| if !r.session.is_empty() { r.session == self.id } else { r.project.is_empty() || r.project == self.project }).cloned().collect()
    }

    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let list = self.mine(cx);
        match sreq::armed(&list, (!self.armed.is_empty()).then_some(self.armed.as_str())) {
            // A request armed by itself starts with an empty box (the web
            // draws every card's box anew), its word from its lock (F210).
            Some((id, kept)) => if !kept {
                let lock = list.iter().find(|r| r.id == id).map(|r| r.lock.clone()).unwrap_or_default();
                self.armed = id; self.auto = true; self.err.clear();
                self.fresh(&lock, false, window, cx);
            },
            None => { self.armed.clear(); self.err.clear(); }
        }
        if list != self.shown { self.shown = list; cx.notify(); }
    }

    /// The box as a new card has it: empty, saying 'PIN' or 'Passphrase'
    /// for the request's lock (SL.field), focused when chosen by hand.
    fn fresh(&mut self, lock: &str, focus: bool, window: &mut Window, cx: &mut Context<Self>) {
        let ph = if lock == "passphrase" { "Passphrase" } else { "PIN" };
        self.ph = ph;
        self.pin.update(cx, |s, cx| { s.set_value("", window, cx); s.set_placeholder(ph, window, cx); if focus { s.focus(window, cx); } });
    }

    fn arm(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.armed = id.to_string();
        self.auto = false;
        self.err.clear();
        self.allow.0 = false;
        let lock = self.shown.iter().find(|r| r.id == id).map(|r| r.lock.clone()).unwrap_or_default();
        self.fresh(&lock, true, window, cx);
        cx.notify();
    }

    fn approve(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.shown.iter().find(|r| r.id == self.armed).cloned() else { return };
        let v = self.pin.read(cx).value().to_string();
        if let Err(e) = sreq::check(&r.lock, &v) { self.err = e.into(); cx.notify(); return; }
        let allow = (self.allow.0 && self.can_allow(&r, cx)).then_some(self.allow.1);
        let body = sreq::approve_body(&r, &v, allow);
        let me = cx.entity().downgrade();
        let wh = window.window_handle();
        let pin = if r.lock == "pin" { v.trim().to_string() } else { v.clone() };
        crundi_shell::call(cx, move |api| api.post("/api/secrets", &body), move |res, cx| {
            let upgrade = res.as_ref().ok().and_then(|d| d.get("needsUpgrade")).and_then(|x| x.as_bool()).unwrap_or(false) || r.upgrade;
            match sreq::answer(res.map(|v| (200, v)), allow.is_some()) {
                Ok(t) => {
                    let _ = me.update(cx, |this, cx| { this.armed.clear(); this.err.clear(); this.allow.0 = false; cx.notify(); });
                    // Released: the PIN doesn't stay for the next request (F210).
                    let _ = wh.update(cx, |_, window, cx| { let _ = me.update(cx, |this, cx| this.pin.update(cx, |s, cx| s.set_value("", window, cx))); });
                    crundi_shell::toast(cx, t, w::Toast::Success);
                    load_asks(cx);
                    // An old lock: offer to set a passphrase now, with the one just typed.
                    if upgrade { super::dlg::upgrade(vec![(r.secret.clone(), r.name.clone(), r.lock.clone(), pin)], cx); }
                }
                // A wrong PIN leaves the request waiting: the card says why.
                Err(e) => { let _ = me.update(cx, |this, cx| { this.err = e; cx.notify(); }); }
            }
        });
    }

    fn reject(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.armed == id { self.armed.clear(); self.err.clear(); }
        self.pin.update(cx, |s, cx| s.set_value("", window, cx));
        let body = json!({ "action": "deny", "reqId": id });
        crundi_shell::call(cx, move |api| api.post("/api/secrets", &body), |_, cx| load_asks(cx));
        cx.notify();
    }

    fn props(&self, cx: &App) -> Value {
        json!({ "cards": self.shown.iter().map(|r| json!({ "id": r.id, "name": r.name, "run": r.run, "armed": r.id == self.armed })).collect::<Vec<_>>(), "error": self.err,
            "pin": self.pin.read(cx).value().to_string(), "placeholder": self.ph,
            "allow": { "on": self.allow.0, "for": self.allow.1 } })
    }
}

impl Render for AskCards {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("fx-cp-ask");
        if self.shown.is_empty() { return div().into_any_element(); }
        let line = |cls_why: bool, t: String| div().when(cls_why, |d| d.mt(T::SP_4).text_size(T::TEXT_12).text_color(T::text_secondary()).line_height(relative(1.4))).child(t);
        let mut host = div().id("fx-cp-ask").flex().flex_col().gap(T::SP_8).p(T::SP_10).bg(T::bg_secondary()).border_b_1().border_color(T::border())
            .max_h(relative(0.6)).overflow_y_scroll();
        for (i, r) in self.shown.iter().enumerate() {
            let armed = r.id == self.armed;
            let head = if r.run { format!("Run a command with {}", r.name) } else { format!("Release {}", r.name) };
            let mut card = div().border_1().border_color(T::yellow()).bg(T::yellow_dim()).rounded(T::R_CARD).p(T::SP_12)
                .child(div().text_size(T::TEXT_14).font_weight(T::W_SEMIBOLD).child(head))
                .when(!r.reason.is_empty(), |d| d.child(line(true, r.reason.clone())));
            if r.run {
                card = card.child(div().mt(T::SP_8).p(T::SP_8).font_family(T::MONO).text_size(T::TEXT_12).bg(T::bg()).border_1().border_color(T::border()).rounded(T::R_CONTROL).child(r.command.clone()))
                    .child(line(true, format!("The value is bound to ${} and never shown to Claude.", r.env)));
            } else {
                card = card.child(line(true, "Claude will be able to read this value.".into()));
            }
            let id = r.id.clone();
            let id2 = r.id.clone();
            let mut row = div().flex().flex_wrap().gap(T::SP_8).mt(T::SP_10);
            if armed {
                row = row.child(div().flex_1().min_w(px(110.)).child(w::input(&self.pin)))
                    .child(w::primary(("fx-cp-approve", i), "Approve").on_click(cx.listener(|this, _, window, cx| this.approve(window, cx))));
            } else {
                row = row.child(w::primary(("fx-cp-approve", i), "Approve").on_click(cx.listener(move |this, _, window, cx| this.arm(&id, window, cx))));
            }
            row = row.child(w::button(("fx-cp-reject", i), "Reject").on_click(cx.listener(move |this, _, window, cx| this.reject(&id2, window, cx))));
            card = card.child(row);
            // "Allow for this chat" under the open card's PIN (CS.allowRow).
            if armed && self.can_allow(r, cx) {
                let (on, dur) = self.allow;
                let at = sreq::ALLOW_FOR.iter().position(|(k, _)| *k == dur);
                let names: Vec<SharedString> = sreq::ALLOW_FOR.iter().map(|(_, l)| SharedString::from(*l)).collect();
                card = card.child(div().flex().flex_wrap().items_center().gap(T::SP_8).mt(T::SP_6).text_size(T::TEXT_13)
                    .child(w::check(("fx-cp-allow", i), "Allow for this chat", on, cx.listener(|this, v: &bool, _, cx| { this.allow.0 = *v; cx.notify(); })))
                    .child(w::select(("fx-cp-allow-for", i), names, at, "1 hour", !on, { let me = cx.entity().downgrade(); move |k: usize, _: &mut Window, cx: &mut App| { if let Some((key, _)) = sreq::ALLOW_FOR.get(k) { let _ = me.update(cx, |this, cx| { this.allow.1 = key; cx.notify(); }); } } }))
                    .when(on, |d| d.child(div().w_full().text_size(T::TEXT_12).text_color(T::text_secondary()).line_height(relative(1.45))
                        .child("This chat may then run commands with this secret without asking, until then. Its value is never shown."))));
            }
            card = card.when(armed && !self.err.is_empty(), |d| d.child(div().mt(T::SP_6).text_size(T::TEXT_12).text_color(T::red()).child(self.err.clone())));
            host = host.child(card);
        }
        host.into_any_element()
    }
}
