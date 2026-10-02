//! UTC instants as RFC 3339 text (`YYYY-MM-DDTHH:MM:SS[.fff]Z`) <-> Unix milliseconds.
//! Only the UTC `Z` form the app writes is accepted (contract timestamps are UTC).

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn digits(text: &str) -> Option<i64> {
    (!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

fn days_in_month(year: i64, month: i64) -> i64 {
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Parses a UTC RFC 3339 timestamp to Unix milliseconds.
pub fn rfc3339_to_ms(text: &str) -> Option<i64> {
    let body = text.strip_suffix('Z')?;
    let (date, clock) = body.split_once('T')?;
    let mut date_parts = date.split('-');
    let (year, month, day) = (date_parts.next()?, date_parts.next()?, date_parts.next()?);
    if date_parts.next().is_some() || year.len() != 4 || month.len() != 2 || day.len() != 2 {
        return None;
    }
    let (clock, fraction) = match clock.split_once('.') {
        Some((c, f)) if (1..=9).contains(&f.len()) && f.bytes().all(|b| b.is_ascii_digit()) => {
            (c, Some(f))
        }
        Some(_) => return None,
        None => (clock, None),
    };
    let mut clock_parts = clock.split(':');
    let (hour, minute, second) = (
        clock_parts.next()?,
        clock_parts.next()?,
        clock_parts.next()?,
    );
    if clock_parts.next().is_some() || [hour, minute, second].iter().any(|p| p.len() != 2) {
        return None;
    }
    let (year, month, day) = (digits(year)?, digits(month)?, digits(day)?);
    let (hour, minute, second) = (digits(hour)?, digits(minute)?, digits(second)?);
    if !(1..=12).contains(&month)
        || day < 1
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let millis = match fraction {
        Some(f) => {
            let padded = format!("{f:0<3}");
            digits(&padded[..3])?
        }
        None => 0,
    };
    let days = days_from_civil(year, month, day);
    Some(((days * 24 + hour) * 60 + minute) * 60_000 + second * 1000 + millis)
}

/// Formats Unix milliseconds as `YYYY-MM-DDTHH:MM:SS.mmmZ` (years 0000-9999).
pub fn ms_to_rfc3339(ms: i64) -> Option<String> {
    let days = ms.div_euclid(86_400_000);
    let rest = ms.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    if !(0..=9999).contains(&year) {
        return None;
    }
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3_600_000,
        rest / 60_000 % 60,
        rest / 1000 % 60,
        rest % 1000
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_instants_round_trip() {
        assert_eq!(rfc3339_to_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            rfc3339_to_ms("2026-10-02T12:34:56.789Z"),
            Some(1_790_944_496_789)
        );
        assert_eq!(rfc3339_to_ms("2000-02-29T00:00:00Z"), Some(951_782_400_000));
        for ms in [0, 951_782_400_000, 1_790_944_496_789, 4_102_444_799_999] {
            let text = ms_to_rfc3339(ms);
            assert_eq!(
                text.as_deref().and_then(rfc3339_to_ms),
                Some(ms),
                "{text:?}"
            );
        }
    }

    #[test]
    fn non_utc_or_malformed_text_is_rejected() {
        for bad in [
            "2026-10-02T12:34:56+07:00",
            "2026-10-02 12:34:56Z",
            "2026-13-02T00:00:00Z",
            "2026-02-29T00:00:00Z",
            "2026-10-02T24:00:00Z",
            "2026-10-02T12:34:56.Z",
            "2026-10-2T12:34:56Z",
            "+026-10-02T12:34:56Z",
            "2026-10-02T12:34:56.1€Z",
            "2026-10-02T12:34:56.123abcZ",
        ] {
            assert_eq!(rfc3339_to_ms(bad), None, "{bad}");
        }
    }
}
