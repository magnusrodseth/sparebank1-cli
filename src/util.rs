//! Small shared helpers: time and Norwegian-locale formatting.

use chrono::{DateTime, Local, TimeZone};

/// Current Unix time in seconds.
pub fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Format a SpareBank 1 millisecond-epoch timestamp as an ISO date (local time).
/// The API expresses transaction dates as ms since the Unix epoch.
pub fn ms_epoch_to_date(ms: i64) -> String {
    match Local.timestamp_millis_opt(ms).single() {
        Some(dt) => dt.format("%Y-%m-%d").to_string(),
        None => String::new(),
    }
}

/// Format a ms-epoch timestamp as a local date-time (used in detail views).
#[allow(dead_code)]
pub fn ms_epoch_to_datetime(ms: i64) -> String {
    match Local.timestamp_millis_opt(ms).single() {
        Some(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        None => String::new(),
    }
}

/// Format an amount with Norwegian conventions: space as thousands separator,
/// comma as the decimal mark, two decimals, prefixed with `kr`.
///
/// `-1234.5` -> `-kr 1 234,50`
pub fn format_kr(amount: f64) -> String {
    let sign = if amount < 0.0 { "-" } else { "" };
    let abs = amount.abs();
    let cents = (abs * 100.0).round() as i64;
    let kroner = cents / 100;
    let rest = cents % 100;

    // Group the integer part into thousands with a normal space.
    let digits = kroner.to_string();
    let mut grouped = String::new();
    let bytes = digits.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            grouped.push(' ');
        }
        grouped.push(*b as char);
    }
    format!("{sign}kr {grouped},{rest:02}")
}

/// A `DateTime<Local>` for `n` days ago, as `YYYY-MM-DD`.
pub fn days_ago(n: i64) -> String {
    (Local::now() - chrono::Duration::days(n))
        .format("%Y-%m-%d")
        .to_string()
}

/// Today as `YYYY-MM-DD` (local).
pub fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

/// Convert a `DateTime` to a display string (helper for tests/clarity).
#[allow(dead_code)]
pub fn fmt_dt(dt: DateTime<Local>) -> String {
    dt.format("%Y-%m-%d %H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_norwegian_amounts() {
        assert_eq!(format_kr(0.0), "kr 0,00");
        assert_eq!(format_kr(5.5), "kr 5,50");
        assert_eq!(format_kr(1234.5), "kr 1 234,50");
        assert_eq!(format_kr(1234567.89), "kr 1 234 567,89");
        assert_eq!(format_kr(-1234.56), "-kr 1 234,56");
        assert_eq!(format_kr(100.0), "kr 100,00");
    }
}
