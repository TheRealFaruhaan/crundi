//! Spike: does a component compiled in another crate (spike-panel) work
//! when embedded in this window through ComponentContainer? Prints one
//! line per check and exits non-zero if any fails. Run under Xvfb with
//! SLINT_BACKEND=winit-software.

slint::include_modules!();

use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, Model, VecModel};
use spike_panel::{PanelState, SpikePanel};
use std::{cell::RefCell, rc::Rc};

thread_local! { static PANEL: RefCell<Option<SpikePanel>> = const { RefCell::new(None) }; }

fn click(w: &slint::Window, x: f32, y: f32) {
    let p = LogicalPosition::new(x, y);
    w.dispatch_event(WindowEvent::PointerMoved { position: p });
    w.dispatch_event(WindowEvent::PointerPressed { position: p, button: PointerEventButton::Left });
    w.dispatch_event(WindowEvent::PointerReleased { position: p, button: PointerEventButton::Left });
}

fn key(w: &slint::Window, t: &str) {
    w.dispatch_event(WindowEvent::KeyPressed { text: t.into() });
    w.dispatch_event(WindowEvent::KeyReleased { text: t.into() });
}

fn px(w: &slint::Window, x: u32, y: u32) -> (u8, u8, u8) {
    let s = w.take_snapshot().expect("snapshot");
    let i = (y * s.width() + x) as usize;
    let p = s.as_slice()[i];
    (p.r, p.g, p.b)
}

/// A panel instance on the host's window adapter: the host's root, focus
/// and open popups are put back afterwards.
fn embed_new(w: &slint::Window) -> SpikePanel {
    use slint::private_unstable_api::re_exports::WindowInner;
    let wi = WindowInner::from_pub(w);
    let host_rc = wi.component();
    let focus = wi.focus_item.borrow().clone();
    let p = SpikePanel::new_with_existing_window(w).unwrap();
    wi.set_component(&host_rc);
    *wi.focus_item.borrow_mut() = focus;
    p
}

fn main() {
    let host = Host::new().unwrap();
    let made = Rc::new(RefCell::new(0));
    let m2 = made.clone();
    // SPIKE_SHARED=1: the panel is made on the host's own window adapter
    // (new_with_existing_window), then the host is made the window's root
    // again; the factory hands that panel over.
    let shared = std::env::var_os("SPIKE_SHARED").is_some();
    if shared {
        use slint::private_unstable_api::re_exports::WindowInner;
        let p = embed_new(host.window());
        PANEL.with(|x| *x.borrow_mut() = Some(p));
    }
    host.set_panel(slint::ComponentFactory::new(move |_ctx| {
        *m2.borrow_mut() += 1;
        if shared { return PANEL.with(|x| x.borrow().as_ref().map(|p| p.clone_strong())); }
        let p = SpikePanel::new().ok()?;
        PANEL.with(|x| *x.borrow_mut() = Some(p.clone_strong()));
        Some(p)
    }));
    let h = host.as_weak();
    let made2 = made.clone();
    slint::Timer::single_shot(std::time::Duration::from_millis(500), move || {
        let host = h.upgrade().unwrap();
        let w = host.window();
        let mut fails = 0;
        let st0 = PANEL.with(|x| x.borrow().as_ref().map(|p| p.clone_strong())).unwrap();
        st0.global::<PanelState>().set_origin_x(host.get_c1x());
        st0.global::<PanelState>().set_origin_y(host.get_c1y());
        let mut check = |name: &str, ok: bool, extra: String| { println!("{} {name} {extra}", if ok { "ok  " } else { "FAIL" }); if !ok { fails += 1; } };
        let panel = PANEL.with(|x| x.borrow().as_ref().map(|p| p.clone_strong()));
        check("the factory made the panel", panel.is_some(), format!("(made {} times)", made2.borrow()));
        let Some(panel) = panel else { std::process::exit(1) };
        let st = panel.global::<PanelState>();
        st.on_hook(|s| format!("hooked:{s}").into());
        check("test hook (a global callback from Rust)", st.invoke_hook("x".into()) == "hooked:x", String::new());
        println!("     host scale {} / panel scale {}; panel window size {:?}; host size {:?}", w.scale_factor(), panel.window().scale_factor(), panel.window().size(), w.size());

        println!("     {}", host.get_geo());
        let bg = px(w, 580, 280);
        check("draws (the panel's background inside the container)", bg == (0x20, 0x20, 0x40), format!("{bg:?}"));

        click(w, 400.0, 25.0);
        check("pointer: a click reaches the panel's TouchArea", st.get_clicks() == 1, format!("clicks={}", st.get_clicks()));
        check("absolute position with the origin added is the window's", st.get_abs_x() == 210.0, format!("{}", st.get_abs_x()));

        click(w, 400.0, 58.0);
        key(w, "a");
        check("focus + keys: click the panel's TextInput, type", st.get_typed() == "a", format!("typed={:?}", st.get_typed()));

        host.invoke_focus_host();
        key(w, "h");
        check("the host's own field still takes keys", host.get_host_typed() == "h", format!("host={:?}", host.get_host_typed()));

        click(w, 400.0, 88.0);
        key(w, "x");
        check("FocusScope: click to focus, key-pressed", st.get_last_key() == "x", format!("last-key={:?}", st.get_last_key()));

        panel.invoke_focus_text();
        key(w, "b");
        check("focus from Rust (a public function calling .focus())", st.get_typed().contains('b'), format!("typed={:?}", st.get_typed()));

        click(w, 400.0, 120.0);
        check("popup: PopupWindow.show() from inside the panel", st.get_popup_shown(), String::new());
        let g1 = px(w, 300, 90);
        check("popup: drawn where the panel put it (origin offset)", g1 == (0, 0xcc, 0), format!("{g1:?}"));
        click(w, 300.0, 90.0);
        check("popup: takes a click", st.get_popup_clicks() == 1, format!("popup-clicks={}", st.get_popup_clicks()));
        key(w, "Escape");

        let rows = Rc::new(VecModel::from((0..10).map(|i| slint::SharedString::from(format!("row {i}"))).collect::<Vec<_>>()));
        st.set_rows(rows.clone().into());
        let p = LogicalPosition::new(400.0, 180.0);
        w.dispatch_event(WindowEvent::PointerMoved { position: p });
        w.dispatch_event(WindowEvent::PointerScrolled { position: p, delta_x: 0.0, delta_y: -60.0 });
        let _ = w.take_snapshot();
        check("scrolling: the wheel scrolls the panel's Flickable", st.get_scroll() > 0.0, format!("scroll={}", st.get_scroll()));
        rows.push("row new".into());
        check("live model: a pushed row is in the panel's model", st.get_rows().row_count() == 11, String::new());

        // A second instance made while the host's field has the focus: the
        // focus stays, and each instance has its own globals.
        host.invoke_focus_host();
        let p2 = embed_new(w);
        p2.global::<PanelState>().set_clicks(100);
        let p2c = p2.clone_strong();
        host.set_panel2(slint::ComponentFactory::new(move |_| Some(p2c.clone_strong())));
        let _ = w.take_snapshot();
        key(w, "k");
        check("a second instance made at run time keeps the host's focus", host.get_host_typed() == "hk", format!("host={:?}", host.get_host_typed()));
        check("each instance has its own globals", st.get_clicks() == 1 && p2.global::<PanelState>().get_clicks() == 100, String::new());
        println!("     {}", host.get_geo());
        let bg2 = px(w, 580, 395);
        check("the second instance draws", bg2 == (0x20, 0x20, 0x40), format!("{bg2:?}"));
        click(w, 400.0, 300.0 + 25.0);
        check("pointer reaches the second instance", p2.global::<PanelState>().get_clicks() == 101, format!("{}", p2.global::<PanelState>().get_clicks()));
        click(w, 400.0, 25.0);
        check("and still the first", st.get_clicks() == 2, format!("{}", st.get_clicks()));

        println!("\n{} failed", fails);
        std::process::exit(if fails > 0 { 1 } else { 0 });
    });
    host.run().unwrap();
}
