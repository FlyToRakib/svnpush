//! Wall-clock timestamps in UTC, without a date library.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since the Unix epoch.
pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Days since 1970-01-01 → (year, month, day), proleptic Gregorian (Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, u32::try_from(month).unwrap_or(1), u32::try_from(day).unwrap_or(1))
}

struct Parts {
    year: i64,
    month: u32,
    day: u32,
    hour: u64,
    minute: u64,
    second: u64,
}

fn parts(unix: u64) -> Parts {
    let days = i64::try_from(unix / 86_400).unwrap_or(0);
    let (year, month, day) = civil_from_days(days);
    let rest = unix % 86_400;
    Parts { year, month, day, hour: rest / 3_600, minute: rest % 3_600 / 60, second: rest % 60 }
}

/// `2026-09-17T10:15:30Z`
pub fn iso8601(unix: u64) -> String {
    let p = parts(unix);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        p.year, p.month, p.day, p.hour, p.minute, p.second
    )
}

/// `20260917-101530`, sortable and safe in file names.
pub fn file_stamp(unix: u64) -> String {
    let p = parts(unix);
    format!("{:04}{:02}{:02}-{:02}{:02}{:02}", p.year, p.month, p.day, p.hour, p.minute, p.second)
}

/// `2026-09`, the key for monthly request counts.
pub fn month(unix: u64) -> String {
    let p = parts(unix);
    format!("{:04}-{:02}", p.year, p.month)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_known_instants() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso8601(1_789_640_130), "2026-09-17T10:15:30Z");
        assert_eq!(file_stamp(1_789_640_130), "20260917-101530");
        assert_eq!(month(1_789_640_130), "2026-09");
    }
}
