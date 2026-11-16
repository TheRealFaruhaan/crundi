//! Dates and times as the server sends them (ISO 8601), with no time crate.

/// An ISO date ("2026-11-15T08:00:00.000Z", an offset or none) as seconds since 1970.
pub fn parse_iso(s: &str) -> Option<f64> {
    let (date, rest) = s.split_once('T')?;
    let mut d = date.split('-').map(|x| x.parse::<i64>());
    let (y, mo, da) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let (time, zone) = match rest.find(['Z', '+', '-']) { Some(i) => (&rest[..i], &rest[i..]), None => (rest, "Z") };
    let mut t = time.split(':');
    let (hh, mm) = (t.next()?.parse::<i64>().ok()?, t.next()?.parse::<i64>().ok()?);
    let ss: f64 = t.next().unwrap_or("0").parse().ok()?;
    let offset = if zone == "Z" || zone.is_empty() { 0 } else {
        let sign = if zone.starts_with('-') { -1 } else { 1 };
        let z = &zone[1..];
        let (zh, zm) = z.split_once(':').unwrap_or((z.get(..2).unwrap_or("0"), z.get(2..).unwrap_or("0")));
        sign * (zh.parse::<i64>().unwrap_or(0) * 3600 + zm.parse::<i64>().unwrap_or(0) * 60)
    };
    // Days from civil (Howard Hinnant).
    let (y2, m2) = if mo <= 2 { (y - 1, mo + 9) } else { (y, mo - 3) };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * m2 + 2) / 5 + da - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some((days * 86400 + hh * 3600 + mm * 60 - offset) as f64 + ss)
}

/// Seconds since 1970, now.
pub fn now_secs() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    #[test]
    fn iso_dates() {
        assert_eq!(super::parse_iso("1970-01-01T00:00:00Z"), Some(0.0));
        assert_eq!(super::parse_iso("1970-01-02T01:00:00.500+01:00"), Some(86400.5));
        assert!(super::parse_iso("nope").is_none());
    }
}
