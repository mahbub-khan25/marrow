//! Reading the times cloud services give (`2026-10-27T01:23:45.678Z`).

/// Milliseconds since 1970 for an RFC 3339 time; 0 if it can't be read.
pub(crate) fn ms(s: &str) -> u64 {
    parse(s).unwrap_or(0)
}

fn parse(s: &str) -> Option<u64> {
    let s = s.trim();
    let (date, rest) = s.split_once(['T', 't', ' '])?;
    let mut d = date.split('-');
    let (y, mo, da): (i64, i64, i64) = (d.next()?.parse().ok()?, d.next()?.parse().ok()?, d.next()?.parse().ok()?);
    // Split off the zone: Z, +hh:mm or -hh:mm.
    let (clock, offset_min) = if let Some(c) = rest.strip_suffix(['Z', 'z']) {
        (c, 0i64)
    } else if let Some(i) = rest.rfind(['+', '-']) {
        let (c, z) = rest.split_at(i);
        let sign = if z.starts_with('-') { -1 } else { 1 };
        let mut hm = z[1..].split(':');
        let h: i64 = hm.next()?.parse().ok()?;
        let m: i64 = hm.next().unwrap_or("0").parse().ok()?;
        (c, sign * (h * 60 + m))
    } else {
        (rest, 0)
    };
    let mut t = clock.split(':');
    let h: i64 = t.next()?.parse().ok()?;
    let mi: i64 = t.next()?.parse().ok()?;
    let sec = t.next().unwrap_or("0");
    let (whole, frac) = sec.split_once('.').unwrap_or((sec, ""));
    let se: i64 = whole.parse().ok()?;
    let millis: i64 = format!("{:0<3}", &frac[..frac.len().min(3)]).parse().unwrap_or(0);
    let days = days_from_civil(y, mo, da);
    let total = ((days * 24 + h) * 60 + mi - offset_min) * 60 + se;
    u64::try_from(total * 1000 + millis).ok()
}

/// Days since 1970-01-01 (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_times() {
        assert_eq!(ms("1970-01-01T00:00:00Z"), 0);
        assert_eq!(ms("2026-10-27T01:23:45.678Z"), 1_793_064_225_678);
        assert_eq!(ms("2026-10-27T01:23:45Z"), 1_793_064_225_000);
        assert_eq!(ms("2026-10-26T21:23:45-04:00"), 1_793_064_225_000);
        assert_eq!(ms("2026-10-27T01:23:45.6Z"), 1_793_064_225_600);
        assert_eq!(ms("nonsense"), 0);
    }
}
