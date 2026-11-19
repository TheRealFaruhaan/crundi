//! A button that asks by arming (the Slint client's SmallBtn with
//! confirm-text; the web's two-click .dk-arm): the first click turns it red
//! and shows its question ("Stop?", "Remove?"), a second click within three
//! seconds does it; left alone it disarms. With no question it acts at once.
//! Looks: "" (quiet), "primary", "danger". (Made by Operations: Docker's
//! Stop, Kill, Down, Remove, Clean…, AREAS.md "A new control".)

use crundi_theme::IconName;
use gpui::*;
use gpui_component::{button::{Button, ButtonVariants as _}, Disableable as _};
use std::rc::Rc;

/// How long an armed button waits for the second click.
const ARMED_FOR: std::time::Duration = std::time::Duration::from_secs(3);

#[derive(IntoElement)]
pub struct ArmButton {
    id: ElementId,
    label: SharedString,
    icon: Option<IconName>,
    ask: SharedString,
    look: &'static str,
    disabled: bool,
    fire: Rc<dyn Fn(&mut Window, &mut App)>,
}

/// `label` ("" for an icon-only one), its `icon`, the question shown while
/// armed ("" acts on the first click), and what it does.
pub fn arm_button(id: impl Into<ElementId>, label: impl Into<SharedString>, icon: Option<IconName>, ask: impl Into<SharedString>, fire: impl Fn(&mut Window, &mut App) + 'static) -> ArmButton {
    ArmButton { id: id.into(), label: label.into(), icon, ask: ask.into(), look: "", disabled: false, fire: Rc::new(fire) }
}

impl ArmButton {
    /// "", "primary" or "danger".
    pub fn look(mut self, look: &'static str) -> Self { self.look = look; self }
    pub fn disabled(mut self, off: bool) -> Self { self.disabled = off; self }
}

impl RenderOnce for ArmButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| false);
        let armed = *state.read(cx) && !self.ask.is_empty();
        let label = if armed { self.ask.clone() } else { self.label.clone() };
        let mut b = Button::new(self.id);
        b = if armed || self.look == "danger" { b.danger() } else if self.look == "primary" { b.primary() } else { b };
        b = crate::control::sized(b, (!label.is_empty()).then(|| label.clone()), self.icon.filter(|_| !armed), false, crate::control::Ctl::Normal);
        let (fire, asks) = (self.fire.clone(), !self.ask.is_empty());
        b.disabled(self.disabled).on_click(move |_, window, cx| {
            if !asks || armed {
                state.update(cx, |s, cx| { *s = false; cx.notify(); });
                fire(window, cx);
                return;
            }
            state.update(cx, |s, cx| { *s = true; cx.notify(); });
            let st = state.clone();
            cx.spawn(async move |cx| {
                cx.background_executor().timer(ARMED_FOR).await;
                let _ = st.update(cx, |s, cx| { *s = false; cx.notify(); });
            }).detach();
        })
    }
}
