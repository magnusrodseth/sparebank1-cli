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

/// Placeholder for a masked free-text value (name, account number, counterparty)
/// when `--mask` is active.
pub const MASKED: &str = "*****";

/// Masked amount, preserving the sign and `kr` shape so masked tables still look
/// like real output (`-kr *****` / `kr *****`).
pub fn mask_kr(amount: f64) -> String {
    if amount < 0.0 {
        "-kr *****".to_string()
    } else {
        "kr *****".to_string()
    }
}

/// Format an amount, masked when `mask` is set, otherwise the normal kr format.
pub fn kr(amount: f64, mask: bool) -> String {
    if mask {
        mask_kr(amount)
    } else {
        format_kr(amount)
    }
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

    #[test]
    fn groups_thousands_at_boundaries() {
        assert_eq!(format_kr(1000.0), "kr 1 000,00");
        assert_eq!(format_kr(10000.0), "kr 10 000,00");
    }

    #[test]
    fn rounds_to_two_decimals() {
        assert_eq!(format_kr(1.999), "kr 2,00");
        assert_eq!(format_kr(0.994), "kr 0,99");
    }

    #[test]
    fn negative_zero_has_no_minus_sign() {
        assert_eq!(format_kr(-0.0), "kr 0,00");
    }

    #[test]
    fn mask_kr_keeps_sign_and_currency_shape() {
        assert_eq!(mask_kr(0.0), "kr *****");
        assert_eq!(mask_kr(1234.56), "kr *****");
        assert_eq!(mask_kr(-1234.56), "-kr *****");
    }

    #[test]
    fn mask_kr_treats_negative_zero_as_positive() {
        // Matches format_kr: -0.0 is not < 0.0, so no leading minus.
        assert_eq!(mask_kr(-0.0), "kr *****");
    }

    #[test]
    fn kr_masks_only_when_flag_set() {
        assert_eq!(kr(1234.5, false), format_kr(1234.5));
        assert_eq!(kr(1234.5, true), mask_kr(1234.5));
        assert_eq!(kr(-99.0, true), "-kr *****");
    }

    #[test]
    fn ms_epoch_converts_to_local_date() {
        // 2021-06-15 12:00:00 UTC. Noon UTC keeps the same calendar date across
        // every plausible local timezone, so this is deterministic in CI.
        assert_eq!(ms_epoch_to_date(1_623_758_400_000), "2021-06-15");
    }

    #[test]
    fn ms_epoch_is_empty_for_unrepresentable() {
        assert_eq!(ms_epoch_to_date(i64::MAX), "");
    }

    #[test]
    fn days_ago_zero_is_today() {
        assert_eq!(days_ago(0), today());
    }
}
