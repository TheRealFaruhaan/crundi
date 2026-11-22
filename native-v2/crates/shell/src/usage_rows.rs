//! The top bar's usage bars (the web's renderUsage / tickResets /
//! paintOverPace; the Slint client's show_usage): for the weekly and the
//! 5-hour window, how much is used, how much of the window has passed, the
//! time to its reset, and how far usage runs ahead of the clock.

use serde_json::Value;

/// One window's row.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// "wk" or "5h".
    pub tag: &'static str,
    /// Used, 0-100.
    pub used: f32,
    /// The window passed, 0-100 (None: no reset time).
    pub time: Option<f32>,
    /// The time to the reset ("2d 4h", "3h 12m", "45m"; "—" none).
    pub left: String,
    /// Usage ahead of the clock, in points (0: on or behind pace).
    pub over: f32,
}

pub const WEEK: f64 = 7.0 * 86400.0;
pub const FIVE_HOURS: f64 = 5.0 * 3600.0;

/// "2d 4h", "3h 12m", "45m" (the web's fmtRemaining).
pub fn remaining(left: f64) -> String {
    let left = left.max(0.0);
    let (d, h, m) = ((left / 86400.0) as u64, ((left % 86400.0) / 3600.0) as u64, ((left % 3600.0) / 60.0) as u64);
    if d > 0 { format!("{d}d {h}h") } else if h > 0 { format!("{h}h {m}m") } else { format!("{m}m") }
}

/// The two rows (weekly first, as the web's), from the server's usage
/// answer; None when it is not a good one (the bars keep their last values).
pub fn rows(u: &Value, now: f64) -> Option<[Row; 2]> {
    if !u.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) { return None; }
    let row = |k: &str, tag: &'static str, span: f64| {
        let used = u.pointer(&format!("/{k}/utilization")).and_then(|v| v.as_f64()).unwrap_or(0.0).clamp(0.0, 100.0) as f32;
        let resets = u.pointer(&format!("/{k}/resetsAt")).and_then(|v| v.as_str()).and_then(crundi_core::time::parse_iso);
        let (time, left) = match resets {
            Some(end) => { let l = end - now; (Some((100.0 * (1.0 - l / span)).clamp(0.0, 100.0) as f32), remaining(l)) }
            None => (None, "—".into()),
        };
        let over = time.map(|t| (used - t).max(0.0)).unwrap_or(0.0);
        Row { tag, used, time, left, over }
    };
    Some([row("week", "wk", WEEK), row("fiveHour", "5h", FIVE_HOURS)])
}

/// The over-pace slice's strength, 0-1 (fully red at 25 points ahead: the web's UB_OVER_FULL).
pub fn over_strength(over: f32) -> f32 { (over / 25.0).clamp(0.0, 1.0) }

/// "just now", "5m ago", "3h ago", "2d ago" (the badge).
pub fn ago(secs: f64) -> String {
    let d = secs.max(0.0);
    if d < 60.0 { "just now".into() } else if d < 3600.0 { format!("{}m ago", (d / 60.0) as u64) } else if d < 86400.0 { format!("{}h ago", (d / 3600.0) as u64) } else { format!("{}d ago", (d / 86400.0) as u64) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rows_and_pace() {
        let now = crundi_core::time::parse_iso("2026-11-17T12:00:00Z").unwrap();
        let u = json!({ "ok": true,
            "week": { "utilization": 60, "resetsAt": "2026-11-20T12:00:00Z" },
            "fiveHour": { "utilization": 10, "resetsAt": "2026-11-17T14:30:00Z" } });
        let [wk, fh] = rows(&u, now).unwrap();
        assert_eq!((wk.tag, wk.used, wk.left.as_str()), ("wk", 60.0, "3d 0h"));
        // 4 of 7 days passed: 57.1%; 60% used: ~2.9 points ahead.
        assert!((wk.time.unwrap() - 57.14).abs() < 0.1 && (wk.over - 2.86).abs() < 0.1);
        assert_eq!((fh.left.as_str(), fh.over), ("2h 30m", 0.0));
        assert!(rows(&json!({ "ok": false }), now).is_none());
        assert_eq!(over_strength(50.0), 1.0);
        assert_eq!(ago(30.0), "just now");
        assert_eq!(ago(600.0), "10m ago");
    }
}
