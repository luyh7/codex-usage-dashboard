//! ISO timestamp ingestion matching the frozen Python 3.14 behavior, with
//! microsecond precision. Naive timestamps are UTC, independent of host zone.
use chrono::{DateTime, Datelike, Duration, NaiveDate, SecondsFormat, Utc, Weekday};

fn digits(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}
fn date_prefix(text: &str) -> Option<(NaiveDate, usize)> {
    let year = digits(text.get(..4)?)? as i32;
    if !(1..=9999).contains(&year) {
        return None;
    }
    let dashed = text.as_bytes().get(4) == Some(&b'-');
    let index = if dashed { 5 } else { 4 };
    if text.as_bytes().get(index) == Some(&b'W') {
        let week = digits(text.get(index + 1..index + 3)?)?;
        let mut end = index + 3;
        let weekday = if dashed && text.as_bytes().get(end) == Some(&b'-') {
            let day = digits(text.get(end + 1..end + 2)?)?;
            end += 2;
            day
        } else if !dashed && text.as_bytes().get(end).is_some_and(u8::is_ascii_digit) {
            let day = digits(text.get(end..end + 1)?)?;
            end += 1;
            day
        } else {
            1
        };
        let weekdays = [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
            Weekday::Sat,
            Weekday::Sun,
        ];
        let weekday = *weekdays.get(weekday.checked_sub(1)? as usize)?;
        return Some((NaiveDate::from_isoywd_opt(year, week, weekday)?, end));
    }
    let month = digits(text.get(index..index + 2)?)?;
    let day_start = if dashed {
        if text.as_bytes().get(index + 2) != Some(&b'-') {
            return None;
        }
        index + 3
    } else {
        index + 2
    };
    Some((
        NaiveDate::from_ymd_opt(year, month, digits(text.get(day_start..day_start + 2)?)?)?,
        day_start + 2,
    ))
}
fn hms(text: &str) -> Option<(u32, u32, u32, u32)> {
    let (clock, fraction) = if let Some(i) = text.find(['.', ',']) {
        (&text[..i], Some(&text[i + 1..]))
    } else {
        (text, None)
    };
    let parts: Vec<_> = if clock.contains(':') {
        clock.split(':').collect()
    } else {
        if ![2, 4, 6].contains(&clock.len()) || !clock.is_ascii() {
            return None;
        }
        (0..clock.len())
            .step_by(2)
            .map(|i| &clock[i..i + 2])
            .collect()
    };
    if parts.is_empty() || parts.len() > 3 || parts.iter().any(|p| p.len() != 2) {
        return None;
    }
    let hour = digits(parts[0])?;
    let minute = if parts.len() > 1 {
        digits(parts[1])?
    } else {
        0
    };
    let second = if parts.len() > 2 {
        digits(parts[2])?
    } else {
        0
    };
    let micros = if let Some(fraction) = fraction {
        if parts.len() != 3 || fraction.is_empty() || !fraction.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let len = fraction.len().min(6);
        digits(&fraction[..len])? * 10u32.pow((6 - len) as u32)
    } else {
        0
    };
    Some((hour, minute, second, micros))
}

pub fn parse_date(text: &str) -> Option<NaiveDate> {
    let (date, length) = date_prefix(text)?;
    if length == text.len() {
        Some(date)
    } else {
        None
    }
}

pub fn parse(text: &str) -> Option<DateTime<Utc>> {
    let text = text.trim();
    let (date, date_len) = date_prefix(text)?;
    let rest = text.get(date_len..)?;
    if rest.is_empty() {
        return Some(date.and_hms_opt(0, 0, 0)?.and_utc());
    }
    let separator = rest.chars().next()?;
    let time = &rest[separator.len_utf8()..];
    let (clock, zone) = if let Some(i) = time.find(['+', '-', 'Z']) {
        (&time[..i], &time[i..])
    } else {
        (time, "")
    };
    let (hour, minute, second, micros) = hms(clock)?;
    if hour > 24
        || minute > 59
        || second > 59
        || hour == 24 && (minute != 0 || second != 0 || micros != 0)
    {
        return None;
    }
    let midnight = date.and_hms_opt(0, 0, 0)?;
    let mut instant = midnight.checked_add_signed(Duration::microseconds(
        (hour as i64 * 3600 + minute as i64 * 60 + second as i64) * 1_000_000 + micros as i64,
    ))?;
    if !zone.is_empty() && zone != "Z" {
        let sign = match zone.as_bytes()[0] {
            b'+' => 1,
            b'-' => -1,
            _ => return None,
        };
        let (h, m, s, f) = hms(&zone[1..])?;
        let seconds = h as i64 * 3600 + m as i64 * 60 + s as i64;
        if seconds >= 86400 {
            return None;
        }
        // CPython treats an all-zero HMS offset as UTC, ignoring its fraction.
        let offset = if seconds == 0 {
            0
        } else {
            seconds * 1_000_000 + f as i64
        };
        instant = instant.checked_sub_signed(Duration::microseconds(sign * offset))?;
    }
    if !(1..=9999).contains(&instant.year()) {
        return None;
    }
    Some(instant.and_utc())
}

pub fn format(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(
        if t.timestamp_subsec_micros() == 0 {
            SecondsFormat::Secs
        } else {
            SecondsFormat::Micros
        },
        true,
    )
}
