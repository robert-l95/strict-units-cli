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

fn find_unit(unit: &str, lenient: bool) -> Option<&'static (&'static str, f64, u8)> {
    if lenient {
        UNITS.iter().find(|u| u.0.eq_ignore_ascii_case(unit))
    } else {
        UNITS.iter().find(|u| u.0 == unit)
    }
}

/// Reports whether `unit` is a recognized duration unit (ns, us, ms, s, m, h, d).
pub fn is_duration_unit(unit: &str, lenient: bool) -> bool {
    find_unit(unit, lenient).is_some()
}

/// Converts a `Duration` to a count of the given unit, e.g. 90s as "m" -> 1.5.
pub fn unit_value(d: Duration, unit: &str, lenient: bool) -> Result<f64, ParseError> {
    let entry = find_unit(unit, lenient).ok_or_else(|| {
        ParseError::new(format!(
            "{unit:?} is not a recognized duration unit; expected one of ns, us, ms, s, m, h, d"
        ))
    })?;
    Ok(d.as_nanos() as f64 / entry.1)
}

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

        let unit_entry = find_unit(unit_str, lenient).ok_or_else(|| {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_compound_literal() {
        let d = parse_duration("1h30m", false).unwrap();
        assert_eq!(d.as_nanos(), 5_400_000_000_000);
    }

    #[test]
    fn strict_single_component() {
        assert_eq!(parse_duration("500ms", false).unwrap().as_nanos(), 500_000_000);
        assert_eq!(parse_duration("30s", false).unwrap().as_secs(), 30);
        assert_eq!(parse_duration("1d", false).unwrap().as_secs(), 86_400);
    }

    #[test]
    fn strict_exact_fractional_component() {
        assert_eq!(parse_duration("1.5s", false).unwrap().as_nanos(), 1_500_000_000);
    }

    #[test]
    fn strict_rejects_out_of_order() {
        let err = parse_duration("30m1h", false).unwrap_err().to_string();
        assert!(err.contains("descending order"), "{err}");
    }

    #[test]
    fn strict_rejects_repeated_unit() {
        assert!(parse_duration("1h1h", false).is_err());
    }

    #[test]
    fn strict_rejects_wrong_case() {
        assert!(parse_duration("10S", false).is_err());
    }

    #[test]
    fn strict_rejects_missing_unit() {
        let err = parse_duration("10", false).unwrap_err().to_string();
        assert!(err.contains("missing a unit"), "{err}");
    }

    #[test]
    fn strict_rejects_whitespace() {
        assert!(parse_duration("1h 30m", false).is_err());
    }

    #[test]
    fn strict_rejects_unknown_unit() {
        assert!(parse_duration("10z", false).is_err());
    }

    #[test]
    fn strict_rejects_non_integer_nanoseconds() {
        let err = parse_duration("1.5ns", false).unwrap_err().to_string();
        assert!(err.contains("whole number"), "{err}");
    }

    #[test]
    fn strict_rejects_empty_input() {
        assert!(parse_duration("", false).is_err());
    }

    #[test]
    fn lenient_strips_whitespace() {
        let d = parse_duration(" 1h 30m ", true).unwrap();
        assert_eq!(d.as_nanos(), 5_400_000_000_000);
    }

    #[test]
    fn lenient_case_insensitive_units() {
        let d = parse_duration("1H30M", true).unwrap();
        assert_eq!(d.as_nanos(), 5_400_000_000_000);
    }

    #[test]
    fn lenient_allows_any_order() {
        let d = parse_duration("30m1h", true).unwrap();
        assert_eq!(d.as_nanos(), 5_400_000_000_000);
    }

    #[test]
    fn lenient_sums_repeated_units() {
        let d = parse_duration("1h1h", true).unwrap();
        assert_eq!(d.as_nanos(), 7_200_000_000_000);
    }

    #[test]
    fn lenient_bare_number_is_seconds() {
        let d = parse_duration("90", true).unwrap();
        assert_eq!(d.as_nanos(), 90_000_000_000);
    }

    #[test]
    fn lenient_rejects_negative() {
        let err = parse_duration("-5", true).unwrap_err().to_string();
        assert!(err.contains("negative"), "{err}");
    }

    #[test]
    fn lenient_rounds_fractional_nanoseconds() {
        assert_eq!(parse_duration("1.5ns", true).unwrap().as_nanos(), 2);
    }

    #[test]
    fn unit_value_converts_strict() {
        let d = Duration::from_nanos(5_400_000_000_000);
        assert_eq!(unit_value(d, "m", false).unwrap(), 90.0);
        assert_eq!(unit_value(d, "h", false).unwrap(), 1.5);
    }

    #[test]
    fn unit_value_converts_lenient_case_insensitive() {
        let d = Duration::from_secs(90);
        assert_eq!(unit_value(d, "M", true).unwrap(), 1.5);
    }

    #[test]
    fn unit_value_rejects_unknown_unit() {
        assert!(unit_value(Duration::from_secs(1), "z", false).is_err());
    }

    #[test]
    fn is_duration_unit_examples() {
        assert!(is_duration_unit("h", false));
        assert!(!is_duration_unit("H", false));
        assert!(is_duration_unit("H", true));
        assert!(!is_duration_unit("MB", true));
    }

    #[test]
    fn format_duration_examples() {
        assert_eq!(format_duration(Duration::ZERO), "0s");
        assert_eq!(format_duration(Duration::from_nanos(5_400_000_000_000)), "1h30m");
        assert_eq!(format_duration(Duration::from_millis(500)), "500ms");
    }
}
