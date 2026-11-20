//! What a pane kind adds to its panes' heads beyond `HeadInfo` (F80, F83):
//! an icon after the grip (a panel's), extra head buttons before pin (the
//! Mind map's "?", a panel's Refresh), and per pane the tags (a chat's plan
//! pill, collaborator, factory and "chat" tags) and the "Secrets for this
//! chat" lock. Registered once per key prefix with `register_head`, beside
//! `register_kind` (which is unchanged, so every kind keeps compiling):
//!
//! ```ignore
//! crundi_workbench::register_head(cx, "panel:mindmap", KindHead::new()
//!     .icon(IconName::Network)
//!     .button(HeadButton::text("help", "?", "Mindmap shortcuts"))
//!     .button(HeadButton::icon("refresh", IconName::RefreshCw, "Refresh"))
//!     .on_action(|key, action, window, cx| …));
//! crundi_workbench::register_head(cx, "live:", KindHead::new()
//!     .extra(|key, cx| { let mut e = HeadExtra::chat_from(listed, plan_status); e.secrets = Some(n); e })
//!     .on_action(|key, action, window, cx| match action { "chat-plan" => …, "chat-secrets" => …, _ => {} }));
//! ```
//!
//! The head reads `extra` when it draws and when the kind's `watch` calls
//! `PaneHead::refresh`: it redraws only when the info or the extra changed.
//! A chat kind with no `extra` registered gets `HeadExtra::chat_from` of its
//! entry in the server's list (Projects.terminals): its plan pill (from the
//! list's planStatus / permissionMode), collaborator, factory and "chat" tags.

use crundi_theme::IconName;
use crundi_widgets::HeadTagLook;
use gpui::*;
use serde_json::Value;
use std::rc::Rc;

/// A tag in a head's tools (before the lock and A- ↺ A+).
#[derive(Clone, PartialEq, Debug)]
pub struct HeadTag {
    pub text: SharedString,
    pub look: HeadTagLook,
    pub tip: SharedString,
    /// Clicking it runs this through the kind's `on_action` (the plan pill's
    /// "chat-plan" when there is a plan to read); None: not clickable.
    pub action: Option<&'static str>,
}

impl HeadTag {
    /// The "chat" kind tag (every chat's).
    pub fn chat() -> Self { HeadTag { text: "chat".into(), look: HeadTagLook::Kind, tip: "".into(), action: None } }

    /// The plan pill (the web's planTagFor): a plan proposed, executing or
    /// sent back ("plan", clickable: "chat-plan"), or plan mode with nothing
    /// to read yet ("planning"); None otherwise.
    pub fn plan(plan_status: &str, permission_mode: &str) -> Option<Self> {
        let (look, label, read, tip) = match plan_status {
            "proposed" => (HeadTagLook::Proposed, "plan", true, "Plan awaiting your approval \u{2014} click to read it"),
            "executing" => (HeadTagLook::Executing, "plan", true, "Approved plan \u{2014} click to read it"),
            "rejected" => (HeadTagLook::Rejected, "plan", true, "Plan sent back for revision \u{2014} click to read it"),
            _ if permission_mode == "plan" => (HeadTagLook::Planning, "planning", false, "Plan mode \u{2014} Claude is working one out"),
            _ => return None,
        };
        Some(HeadTag { text: label.into(), look, tip: tip.into(), action: read.then_some("chat-plan") })
    }

    /// A collaborator's chat on the owner's screen: whose it is.
    pub fn collaborator(name: &str) -> Self {
        let who = if name.is_empty() { "Collaborator" } else { name };
        HeadTag { text: who.to_string().into(), look: HeadTagLook::Collab,
            tip: format!("{who}\u{2019}s chat (outside collaborator). You can type here; it runs in their sandbox and counts toward their tokens.").into(), action: None }
    }

    /// A factory run's Manager.
    pub fn factory() -> Self {
        HeadTag { text: "Factory".into(), look: HeadTagLook::Factory,
            tip: "The Manager of a factory run. It stays open while the run is going: stop the factory to close it.".into(), action: None }
    }
}

/// An extra head button (before pin): an icon or a short text, its tooltip,
/// and the action it runs through the kind's `on_action`.
#[derive(Clone, PartialEq, Debug)]
pub struct HeadButton {
    pub action: &'static str,
    pub icon: Option<IconName>,
    pub text: SharedString,
    pub tip: SharedString,
}

impl HeadButton {
    pub fn icon(action: &'static str, icon: IconName, tip: impl Into<SharedString>) -> Self { HeadButton { action, icon: Some(icon), text: "".into(), tip: tip.into() } }
    pub fn text(action: &'static str, text: impl Into<SharedString>, tip: impl Into<SharedString>) -> Self { HeadButton { action, icon: None, text: text.into(), tip: tip.into() } }
}

/// What one pane's head shows beyond its info (read by the head).
#[derive(Clone, Default, PartialEq, Debug)]
pub struct HeadExtra {
    /// In this order, before the lock (the web: plan, collaborator, factory, chat).
    pub tags: Vec<HeadTag>,
    /// Some(n): the "Secrets for this chat" lock, n secrets granted (its
    /// click runs "chat-secrets"). None: no lock (no capability, a
    /// collaborator's chat, not a chat).
    pub secrets: Option<usize>,
    /// Buttons for this pane only, after the kind's own.
    pub buttons: Vec<HeadButton>,
}

impl HeadExtra {
    /// A chat's tags from its entry in the server's list (the web's
    /// headHtmlLive / updateCellHead): plan pill, collaborator, factory
    /// Manager, "chat". `plan_status`: the chat's own when it knows a newer
    /// one (the web's applyChatMeta), else "" (the list's is used).
    pub fn chat_from(listed: Option<&Value>, plan_status: &str) -> Self {
        let s = |k: &str| listed.and_then(|t| t.get(k)).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let mut tags = vec![];
        let ps = if plan_status.is_empty() { s("planStatus") } else { plan_status.to_string() };
        if let Some(p) = HeadTag::plan(&ps, &s("permissionMode")) { tags.push(p); }
        if let Some(c) = listed.and_then(|t| t.get("collaborator")).filter(|c| !c.is_null()) {
            tags.push(HeadTag::collaborator(c.get("name").and_then(|n| n.as_str()).unwrap_or("")));
        }
        if listed.is_some_and(crundi_core::head::factory_manager) { tags.push(HeadTag::factory()); }
        tags.push(HeadTag::chat());
        HeadExtra { tags, secrets: None, buttons: vec![] }
    }
}

pub type ExtraFn = Rc<dyn Fn(&str, &App) -> HeadExtra>;
pub type HeadAction = Rc<dyn Fn(&str, &str, &mut Window, &mut App)>;

/// A kind's head: its icon, its buttons, each pane's extra, and what a
/// tag's, a button's or the lock's click does (`on_action(key, action)`).
#[derive(Clone, Default)]
pub struct KindHead {
    pub icon: Option<IconName>,
    pub buttons: Vec<HeadButton>,
    pub extra: Option<ExtraFn>,
    pub on_action: Option<HeadAction>,
}

impl KindHead {
    pub fn new() -> Self { Self::default() }
    pub fn icon(mut self, i: IconName) -> Self { self.icon = Some(i); self }
    pub fn button(mut self, b: HeadButton) -> Self { self.buttons.push(b); self }
    pub fn extra(mut self, f: impl Fn(&str, &App) -> HeadExtra + 'static) -> Self { self.extra = Some(Rc::new(f)); self }
    pub fn on_action(mut self, f: impl Fn(&str, &str, &mut Window, &mut App) + 'static) -> Self { self.on_action = Some(Rc::new(f)); self }
}

#[derive(Default)]
struct Heads(Vec<(&'static str, KindHead)>);
impl Global for Heads {}

/// Register a kind's head (at start, beside `register_kind`; a second for
/// the same prefix replaces the first).
pub fn register_head(cx: &mut App, prefix: &'static str, h: KindHead) {
    let heads = cx.default_global::<Heads>();
    heads.0.retain(|(p, _)| *p != prefix);
    heads.0.push((prefix, h));
}

/// The head registered for a key (the longest matching prefix).
pub(crate) fn head_of(cx: &App, key: &str) -> Option<KindHead> {
    cx.try_global::<Heads>().and_then(|h| h.0.iter().filter(|(p, _)| key.starts_with(p)).max_by_key(|(p, _)| p.len()).map(|(_, k)| k.clone()))
}

#[cfg(test)]
mod tests {
    use super::{HeadExtra, HeadTag, HeadTagLook};
    use serde_json::json;

    #[test]
    fn plan_pill_as_the_web() {
        assert_eq!(HeadTag::plan("proposed", "").unwrap().action, Some("chat-plan"));
        assert_eq!(HeadTag::plan("", "plan").unwrap().text.as_ref(), "planning");
        assert!(HeadTag::plan("", "plan").unwrap().action.is_none());
        assert!(HeadTag::plan("", "default").is_none());
    }

    #[test]
    fn chat_tags_in_order() {
        let t = json!({ "planStatus": "executing", "collaborator": { "name": "Ana" }, "factory": { "role": "manager" } });
        let e = HeadExtra::chat_from(Some(&t), "");
        let looks: Vec<_> = e.tags.iter().map(|t| t.look).collect();
        assert_eq!(looks, vec![HeadTagLook::Executing, HeadTagLook::Collab, HeadTagLook::Factory, HeadTagLook::Kind]);
        assert_eq!(HeadExtra::chat_from(None, "").tags, vec![HeadTag::chat()]);
    }
}
