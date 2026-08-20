use crate::error::ParseError;

const DECIMAL: &[(&str, u64)] = &[
    ("PB", 1_000_000_000_000_000),
    ("TB", 1_000_000_000_000),
    ("GB", 1_000_000_000),
    ("MB", 1_000_000),
    ("KB", 1_000),
    ("B", 1),
];

const BINARY: &[(&str, u64)] = &[
    ("PiB", 1u64 << 50),
    ("TiB", 1u64 << 40),
    ("GiB", 1u64 << 30),
    ("MiB", 1u64 << 20),
    ("KiB", 1u64 << 10),
];

/// Parses a byte size literal such as "10MB" or "1.5GiB" into an exact byte count.
///
/// Strict mode requires an explicit, case-sensitive unit suffix and forbids
/// whitespace anywhere in the input. Lenient mode trims whitespace, matches
/// units case-insensitively, accepts single-letter decimal shorthand (K, M,
/// G, T, P), and treats a unit-less number as a raw byte count.
pub fn parse_bytes(input: &str, lenient: bool) -> Result<u64, ParseError> {
    if !lenient && input.chars().any(|c| c.is_whitespace()) {
        return Err(ParseError::new("whitespace in a byte size requires --lenient"));
    }
    let s = if lenient { input.trim() } else { input };
    if s.is_empty() {
        return Err(ParseError::new("empty input"));
    }

    let split = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(s.len());
    let (num_part, unit_raw) = s.split_at(split);
    if num_part.is_empty() {
        return Err(ParseError::new(format!("{s:?} has no leading number")));
    }
    let number: f64 = num_part
        .parse()
        .map_err(|_| ParseError::new(format!("{num_part:?} is not a valid number")))?;

    let unit_part = if lenient { unit_raw.trim() } else { unit_raw };

    let multiplier = if unit_part.is_empty() {
        if lenient {
            1.0
        } else {
            return Err(ParseError::new(format!(
                "{s:?} is missing a unit suffix (e.g. B, KB, MiB); use --lenient to treat bare numbers as bytes"
            )));
        }
    } else if lenient {
        lenient_multiplier(unit_part)?
    } else {
        strict_multiplier(unit_part)?
    };

    let bytes = number * multiplier;
    if bytes.fract() != 0.0 {
        if lenient {
            Ok(bytes.round() as u64)
        } else {
            Err(ParseError::new(format!(
                "{s:?} does not resolve to a whole number of bytes; use --lenient to round"
            )))
        }
    } else {
        Ok(bytes as u64)
    }
}

fn strict_multiplier(unit: &str) -> Result<f64, ParseError> {
    DECIMAL
        .iter()
        .chain(BINARY.iter())
        .find(|u| u.0 == unit)
        .map(|u| u.1 as f64)
        .ok_or_else(|| {
            ParseError::new(format!(
                "{unit:?} is not a recognized unit; expected one of B, KB, MB, GB, TB, PB, KiB, MiB, GiB, TiB, PiB (case-sensitive)"
            ))
        })
}

fn lenient_multiplier(unit: &str) -> Result<f64, ParseError> {
    let upper = unit.to_ascii_uppercase();
    if let Some(u) = DECIMAL
        .iter()
        .chain(BINARY.iter())
        .find(|u| u.0.to_ascii_uppercase() == upper)
    {
        return Ok(u.1 as f64);
    }
    if upper.len() == 1 {
        let mult = match upper.as_str() {
            "B" => 1.0,
            "K" => 1_000.0,
            "M" => 1_000_000.0,
            "G" => 1_000_000_000.0,
            "T" => 1_000_000_000_000.0,
            "P" => 1_000_000_000_000_000.0,
            _ => return Err(unrecognized(unit)),
        };
        return Ok(mult);
    }
    Err(unrecognized(unit))
}

fn unrecognized(unit: &str) -> ParseError {
    ParseError::new(format!("{unit:?} is not a recognized byte unit even in lenient mode"))
}

/// Renders a byte count as a human-scale binary size, e.g. 1610612736 -> "1.50 GiB".
pub fn format_bytes(n: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut value = n as f64;
    let mut idx = 0;
    while value >= 1024.0 && idx < UNITS.len() - 1 {
        value /= 1024.0;
        idx += 1;
    }
    format!("{value:.2} {}", UNITS[idx])
}
