//! The chat secrets dialog (2.12, specs/chat-secrets.md): how a grant's
//! time reads, what a PIN or passphrase field takes. Pure.

use chrono::{DateTime, Local};

/// A grant's "when" line (the web's describe): "until this chat ends", or
/// "until HH:MM (<left> left)".
pub fn when(expires_at: Option<&str>, now: DateTime<Local>) -> String {
    let Some(at) = expires_at.filter(|a| !a.is_empty()).and_then(|a| DateTime::parse_from_rfc3339(a).ok()) else {
        return "until this chat ends".into();
    };
    let at = at.with_timezone(&Local);
    let mins = ((at - now).num_milliseconds() as f64 / 60000.0).round() as i64;
    let left = if mins < 1 { "less than a minute".to_string() }
        else if mins < 60 { format!("{mins} min") }
        else if mins % 60 == 0 { format!("{} h", mins / 60) }
        else { format!("{} h {} min", mins / 60, mins % 60) };
    format!("until {} ({left} left)", at.format("%H:%M"))
}

/// What a lock field keeps as typed: a PIN takes digits only, at most 6.
pub fn typed(lock: &str, text: &str) -> String {
    if lock == "pin" { text.chars().filter(|c| c.is_ascii_digit()).take(6).collect() } else { text.to_string() }
}

/// A lock that can be sent: a PIN of exactly 6 digits, a passphrase of 1 to
/// 1024 characters.
pub fn lock_ok(lock: &str, text: &str) -> bool {
    if lock == "pin" { text.len() == 6 && text.chars().all(|c| c.is_ascii_digit()) } else { (1..=1024).contains(&text.chars().count()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    #[test]
    fn grants_read() {
        let now = Local.with_ymd_and_hms(2026, 11, 14, 12, 9, 0).unwrap();
        let iso = |m: i64| (now + Duration::minutes(m)).to_rfc3339();
        assert_eq!(when(None, now), "until this chat ends");
        assert_eq!(when(Some(""), now), "until this chat ends");
        assert_eq!(when(Some(&iso(15)), now), "until 12:24 (15 min left)");
        assert_eq!(when(Some(&iso(60)), now), "until 13:09 (1 h left)");
        assert_eq!(when(Some(&iso(135)), now), "until 14:24 (2 h 15 min left)");
        assert_eq!(when(Some(&(now + Duration::seconds(20)).to_rfc3339()), now), "until 12:09 (less than a minute left)");
    }

    #[test]
    fn locks() {
        assert_eq!(typed("pin", "12a34567"), "123456");
        assert_eq!(typed("passphrase", "a b"), "a b");
        assert!(lock_ok("pin", "123456") && !lock_ok("pin", "12345"));
        assert!(lock_ok("passphrase", "x") && !lock_ok("passphrase", ""));
    }
}
