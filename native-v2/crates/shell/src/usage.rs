//! The top bar's data (the shell's store): the account's usage (the bars,
//! live from the `usage` event, else /api/usage/latest) and the Telegram
//! bot's status (GET /api/status every 30 s, as the web's updateTgBadge).
//! The bars' countdowns and "updated … ago" read the clock: the store
//! notifies once a minute while it has usage (the resolution they show),
//! never more (ARCHITECTURE.md rule 6).

pub use crate::usage_rows::{ago, over_strength, rows, Row};
use gpui::*;
use serde_json::Value;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Usage {
    /// The server's answer and when it came.
    pub usage: Option<(Value, Instant)>,
    /// The Telegram bot: configured, connected, its @name.
    pub tg: Option<(bool, bool, String)>,
    ticking: bool,
}

impl Usage {
    pub fn set_usage(&mut self, u: Value, cx: &mut Context<Self>) {
        self.usage = Some((u, Instant::now()));
        cx.notify();
        if !self.ticking {
            self.ticking = true;
            cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| loop {
                cx.background_executor().timer(Duration::from_secs(60)).await;
                let alive = this.update(cx, |u, cx| { if u.usage.is_some() { cx.notify(); true } else { u.ticking = false; false } });
                if !matches!(alive, Ok(true)) { break; }
            }).detach();
        }
    }

    pub fn set_tg(&mut self, tg: (bool, bool, String), cx: &mut Context<Self>) {
        if self.tg.as_ref() != Some(&tg) { self.tg = Some(tg); cx.notify(); }
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.usage = None;
        self.tg = None;
        cx.notify();
    }
}
