use crate::error::ParseError;
use std::time::Duration;

// (unit name, nanoseconds per unit, rank from coarsest 0 to finest 6)
const UNITS: &[(&str, f64, u8)] = &[
    ("d", 86_400_000_000_000.0, 0),
    ("h", 3_600_000_000_000.0, 1),
    ("m", 60_000_000_000.0, 2),
    ("s", 1_000_000_000.0, 3),
    ("ms", 1_000_000.0, 4),
    ("us", 1_000.0, 5),
    ("ns", 1.0, 6),
];

/// Parses a duration literal such as "30s" or "1h30m" into a `Duration`.
///
/// Strict mode forbids whitespace, requires an explicit case-sensitive unit
/// on every component, and requires components in strictly descending order
/// (d, h, m, s, ms, us, ns) with each unit used at most once. Lenient mode
/// strips whitespace, matches units case-insensitively, allows components in
/// any order with repeats (values are summed), and treats a unit-less number
/// as a count of seconds.
pub fn parse_duration(input: &str, lenient: bool) -> Result<Duration, ParseError> {
    if !lenient && input.chars().any(|c| c.is_whitespace()) {
        return Err(ParseError::new("whitespace in a duration requires --lenient"));
    }
    let owned: String = if lenient {
        input.chars().filter(|c| !c.is_whitespace()).collect()
    } else {
        input.to_string()
    };
    let s = owned.as_str();
    if s.is_empty() {
        return Err(ParseError::new("empty input"));
    }

    if lenient {
        if let Ok(secs) = s.parse::<f64>() {
            return seconds_to_duration(secs);
        }
    }

    let mut rest = s;
    let mut total_ns: f64 = 0.0;
    let mut last_rank: i16 = -1;

    while !rest.is_empty() {
        let num_end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        if num_end == 0 {
            return Err(ParseError::new(format!("{s:?} is missing a number before a unit")));
        }
        let (num_str, after_num) = rest.split_at(num_end);
        let value: f64 = num_str
            .parse()
            .map_err(|_| ParseError::new(format!("{num_str:?} is not a valid number")))?;

        let unit_end = after_num
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(after_num.len());
        if unit_end == 0 {
            return Err(ParseError::new(format!(
                "{s:?} is missing a unit after {num_str:?}; expected one of ns, us, ms, s, m, h, d"
            )));
        }
        let (unit_str, remainder) = after_num.split_at(unit_end);

        let matched = if lenient {
            UNITS.iter().find(|u| u.0.eq_ignore_ascii_case(unit_str))
        } else {
            UNITS.iter().find(|u| u.0 == unit_str)
        };
        let unit_entry = matched.ok_or_else(|| {
            ParseError::new(format!(
                "{unit_str:?} is not a recognized duration unit; expected one of ns, us, ms, s, m, h, d"
            ))
        })?;
        let ns_per_unit = unit_entry.1;
        let rank = unit_entry.2;

        if !lenient {
            if i16::from(rank) <= last_rank {
                return Err(ParseError::new(format!(
                    "{s:?} repeats a unit or is out of order; strict mode requires descending order (d, h, m, s, ms, us, ns) with each unit at most once"
                )));
            }
            last_rank = i16::from(rank);
        }

        total_ns += value * ns_per_unit;
        rest = remainder;
    }

    if total_ns.fract() != 0.0 {
        if lenient {
            total_ns = total_ns.round();
        } else {
            return Err(ParseError::new(format!(
                "{s:?} does not resolve to a whole number of nanoseconds; use --lenient to round"
            )));
        }
    }
    Ok(Duration::from_nanos(total_ns as u64))
}

fn seconds_to_duration(secs: f64) -> Result<Duration, ParseError> {
    if secs < 0.0 {
        return Err(ParseError::new("duration cannot be negative"));
    }
    Ok(Duration::from_nanos((secs * 1_000_000_000.0).round() as u64))
}

/// Renders a `Duration` as a compact compound literal, e.g. 5400s -> "1h30m".
pub fn format_duration(d: Duration) -> String {
    let mut remaining = d.as_nanos();
    if remaining == 0 {
        return "0s".to_string();
    }
    let mut out = String::new();
    for entry in UNITS {
        let unit_ns = entry.1 as u128;
        let count = remaining / unit_ns;
        if count > 0 {
            out.push_str(&count.to_string());
            out.push_str(entry.0);
            remaining -= count * unit_ns;
        }
    }
    out
}
