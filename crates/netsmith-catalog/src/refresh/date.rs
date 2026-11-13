//! Days, without a calendar crate: the `date` column is `2026-10-31`, UTC.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since 1970 now.
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The UTC day of a Unix time, `2026-10-31`.
pub fn date_of(secs: u64) -> String {
    let (y, m, d) = civil_from_days(i64::try_from(secs / 86_400).unwrap_or(0));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Days since 1970-01-01 of a `YYYY-MM-DD` date, or `None` if it is not one.
pub fn days_of(date: &str) -> Option<i64> {
    let mut parts = date.trim().split('-');
    let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || y.len() != 4 || m.len() != 2 || d.len() != 2 {
        return None;
    }
    let (y, m, d): (i64, i64, i64) = (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    ((1..=12).contains(&m) && (1..=31).contains(&d)).then(|| days_from_civil(y, m, d))
}

/// How many whole days `date` is before `today` (negative for a date in the future).
pub fn age_days(date: &str, today: &str) -> Option<i64> {
    Some(days_of(today)? - days_of(date)?)
}

// Howard Hinnant's civil-date algorithms.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_days_agree() {
        assert_eq!(date_of(0), "1970-01-01");
        assert_eq!(date_of(1_793_649_041), "2026-11-02");
        assert_eq!(date_of(951_782_400), "2000-02-29");
        assert_eq!(days_of("1970-01-01"), Some(0));
        assert_eq!(age_days("2026-10-31", "2026-11-02"), Some(2));
        assert_eq!(age_days("2026-08-04", "2026-11-02"), Some(90));
        assert_eq!(age_days("2025-12-31", "2026-01-01"), Some(1));
    }

    #[test]
    fn a_text_that_is_no_date_is_none() {
        for bad in [
            "",
            "2026-1-5",
            "2026/11/02",
            "2026-13-01",
            "2026-11-02-1",
            "soon",
        ] {
            assert_eq!(days_of(bad), None, "{bad}");
        }
    }
}
