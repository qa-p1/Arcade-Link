//! Sizes, counts, durations and relative times, worded the same in every app
//! (STANDARD.md §8.1). The TypeScript, QML and Dart copies in `design/` pass
//! the same cases (`design/format-cases.json`).

const UNITS: [&str; 5] = ["kB", "MB", "GB", "TB", "PB"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// Decimal units, one decimal place below 100: "940 kB", "12.4 MB", "3.1 GB".
pub fn format_size(bytes: u64) -> String {
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let bytes = u128::from(bytes);
    let mut divisor: u128 = 1;
    for (index, unit) in UNITS.iter().enumerate() {
        divisor *= 1000;
        let tenths = (bytes * 20 + divisor) / (2 * divisor);
        if tenths < 1000 {
            return format!("{}.{} {unit}", tenths / 10, tenths % 10);
        }
        let whole = (bytes * 2 + divisor) / (2 * divisor);
        if whole < 1000 || index == UNITS.len() - 1 {
            return format!("{whole} {unit}");
        }
    }
    unreachable!("the last unit always returns")
}

/// Thousands grouped with commas: "1,234,567".
pub fn format_count(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if n < 0 {
        out.push('-');
    }
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// How a duration reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationStyle {
    /// Media positions and lengths: "1:05:09", "0:05".
    Media,
    /// Jobs: "3 s", "2 min", "1 h 5 min".
    Job,
}

/// A duration in seconds (fractions are dropped).
pub fn format_duration(seconds: f64, style: DurationStyle) -> String {
    let total = if seconds.is_finite() && seconds > 0.0 { seconds.floor() as u64 } else { 0 };
    let (h, m, s) = (total / 3600, total / 60 % 60, total % 60);
    match style {
        DurationStyle::Media if h > 0 => format!("{h}:{m:02}:{s:02}"),
        DurationStyle::Media => format!("{m}:{s:02}"),
        DurationStyle::Job if total < 60 => format!("{total} s"),
        DurationStyle::Job if total < 3600 => format!("{} min", total / 60),
        DurationStyle::Job if m == 0 => format!("{h} h"),
        DurationStyle::Job => format!("{h} h {m} min"),
    }
}

/// "Just now", "5 min ago", "2 h ago", "Yesterday 14:05", "12 Oct",
/// "12 Oct 2025". Times are Unix milliseconds; `offset_minutes` is the local
/// UTC offset, so the calendar day is the user's.
pub fn format_relative(time_ms: i64, now_ms: i64, offset_minutes: i32) -> String {
    let delta = now_ms - time_ms;
    let offset = i64::from(offset_minutes) * 60_000;
    let (local, local_now) = (time_ms + offset, now_ms + offset);
    let (day, today) = (local.div_euclid(86_400_000), local_now.div_euclid(86_400_000));
    if delta < 60_000 && delta > -60_000 {
        return "Just now".into();
    }
    if delta > 0 && delta < 3_600_000 {
        return format!("{} min ago", delta / 60_000);
    }
    if delta > 0 && day == today {
        return format!("{} h ago", delta / 3_600_000);
    }
    let minutes = local.rem_euclid(86_400_000) / 60_000;
    if day == today - 1 {
        return format!("Yesterday {:02}:{:02}", minutes / 60, minutes % 60);
    }
    let (year, month, d) = civil_from_days(day);
    let (this_year, _, _) = civil_from_days(today);
    let name = MONTHS[(month - 1) as usize];
    if year == this_year {
        format!("{d} {name}")
    } else {
        format!("{d} {name} {year}")
    }
}

/// Days since 1970-01-01 → (year, month 1-12, day 1-31); Howard Hinnant's algorithm.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn cases() -> Value {
        serde_json::from_str(include_str!("../../../../design/format-cases.json")).unwrap()
    }

    #[test]
    fn shared_cases() {
        let cases = cases();
        for case in cases["size"].as_array().unwrap() {
            assert_eq!(format_size(case[0].as_u64().unwrap()), case[1], "size {case}");
        }
        for case in cases["count"].as_array().unwrap() {
            assert_eq!(format_count(case[0].as_i64().unwrap()), case[1], "count {case}");
        }
        for case in cases["duration"].as_array().unwrap() {
            let style = if case[1] == "media" { DurationStyle::Media } else { DurationStyle::Job };
            assert_eq!(format_duration(case[0].as_f64().unwrap(), style), case[2], "duration {case}");
        }
        for case in cases["relative"].as_array().unwrap() {
            let got = format_relative(case[0].as_i64().unwrap(), case[1].as_i64().unwrap(), case[2].as_i64().unwrap() as i32);
            assert_eq!(got, case[3], "relative {case}");
        }
    }

    #[test]
    fn extremes() {
        assert_eq!(format_size(u64::MAX), "18447 PB");
        assert_eq!(format_count(i64::MIN), "-9,223,372,036,854,775,808");
        assert_eq!(format_duration(f64::NAN, DurationStyle::Media), "0:00");
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }
}
