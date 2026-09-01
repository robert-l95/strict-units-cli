mod bytesize;
mod duration;
mod error;

use std::env;
use std::io::{self, BufRead};
use std::process::ExitCode;
use std::time::Duration;

fn print_usage() {
    eprintln!("usage: sizedur [--lenient] [--to UNIT] [--min VALUE] [--max VALUE] [--json] [VALUE ...]");
    eprintln!();
    eprintln!("Parses byte size or duration literals and prints their canonical value.");
    eprintln!("With no VALUE arguments, reads one literal per line from stdin.");
    eprintln!();
    eprintln!("  --lenient   accept sloppy input (bare numbers, mixed-case units, extra whitespace)");
    eprintln!("  --to UNIT   convert to an explicit unit instead of printing the canonical value");
    eprintln!("              (bytes: B, KB, MB, GB, TB, PB, KiB, MiB, GiB, TiB, PiB;");
    eprintln!("               duration: ns, us, ms, s, m, h, d)");
    eprintln!("  --min VALUE reject input below VALUE (parsed in the same domain as the input)");
    eprintln!("  --max VALUE reject input above VALUE (parsed in the same domain as the input)");
    eprintln!("  --json      print one JSON object per line instead of plain text");
    eprintln!("  --help      show this message");
}

fn main() -> ExitCode {
    let mut lenient = false;
    let mut json = false;
    let mut to_unit: Option<String> = None;
    let mut min_value: Option<String> = None;
    let mut max_value: Option<String> = None;
    let mut values: Vec<String> = Vec::new();

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--lenient" => lenient = true,
            "--json" => json = true,
            "--to" => match args.next() {
                Some(unit) => to_unit = Some(unit),
                None => {
                    eprintln!("--to requires a unit argument (e.g. --to GB, --to h)");
                    return ExitCode::FAILURE;
                }
            },
            "--min" => match args.next() {
                Some(value) => min_value = Some(value),
                None => {
                    eprintln!("--min requires a value argument (e.g. --min 1MB, --min 5s)");
                    return ExitCode::FAILURE;
                }
            },
            "--max" => match args.next() {
                Some(value) => max_value = Some(value),
                None => {
                    eprintln!("--max requires a value argument (e.g. --max 1GB, --max 1h)");
                    return ExitCode::FAILURE;
                }
            },
            "--help" | "-h" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            other => values.push(other.to_string()),
        }
    }

    if values.is_empty() {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(line) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        values.push(trimmed.to_string());
                    }
                }
                Err(err) => {
                    eprintln!("error reading stdin: {err}");
                    return ExitCode::FAILURE;
                }
            }
        }
    }

    if values.is_empty() {
        print_usage();
        return ExitCode::FAILURE;
    }

    let bounds = Bounds {
        min: min_value.as_deref(),
        max: max_value.as_deref(),
    };

    let mut had_error = false;
    for value in &values {
        if json {
            match process_one_json(value, lenient, to_unit.as_deref(), bounds) {
                Ok(line) => println!("{line}"),
                Err(err) => {
                    eprintln!(
                        "{{\"input\":{},\"error\":{}}}",
                        json_string(value),
                        json_string(&err)
                    );
                    had_error = true;
                }
            }
        } else {
            match process_one(value, lenient, to_unit.as_deref(), bounds) {
                Ok(line) => println!("{line}"),
                Err(err) => {
                    eprintln!("{value}: {err}");
                    had_error = true;
                }
            }
        }
    }

    if had_error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

// Bounds are held as unparsed literals rather than pre-resolved numbers
// because a bound only makes sense once we know which domain (bytes or
// duration) the value it's checked against belongs to, and a single run can
// mix both.
#[derive(Clone, Copy)]
struct Bounds<'a> {
    min: Option<&'a str>,
    max: Option<&'a str>,
}

fn check_bytes_bounds(bytes: u64, bounds: Bounds, lenient: bool) -> Result<(), String> {
    if let Some(min) = bounds.min {
        let min_bytes = bytesize::parse_bytes(min, lenient)
            .map_err(|e| format!("--min {min:?} is not a valid byte size: {e}"))?;
        if bytes < min_bytes {
            return Err(format!("{bytes} bytes is below --min {min} ({min_bytes} bytes)"));
        }
    }
    if let Some(max) = bounds.max {
        let max_bytes = bytesize::parse_bytes(max, lenient)
            .map_err(|e| format!("--max {max:?} is not a valid byte size: {e}"))?;
        if bytes > max_bytes {
            return Err(format!("{bytes} bytes is above --max {max} ({max_bytes} bytes)"));
        }
    }
    Ok(())
}

fn check_duration_bounds(d: Duration, bounds: Bounds, lenient: bool) -> Result<(), String> {
    if let Some(min) = bounds.min {
        let min_d = duration::parse_duration(min, lenient)
            .map_err(|e| format!("--min {min:?} is not a valid duration: {e}"))?;
        if d < min_d {
            return Err(format!(
                "{}ns is below --min {min} ({}ns)",
                d.as_nanos(),
                min_d.as_nanos()
            ));
        }
    }
    if let Some(max) = bounds.max {
        let max_d = duration::parse_duration(max, lenient)
            .map_err(|e| format!("--max {max:?} is not a valid duration: {e}"))?;
        if d > max_d {
            return Err(format!(
                "{}ns is above --max {max} ({}ns)",
                d.as_nanos(),
                max_d.as_nanos()
            ));
        }
    }
    Ok(())
}

// A literal like "10s" is ambiguous between the two domains only in strict
// mode where both would be malformed anyway, so trying bytes first and
// falling back to duration never silently picks the wrong interpretation.
//
// When --to is given, the target unit itself tells us which domain to parse
// as, so there's no bytes-first guessing to do.
fn process_one(value: &str, lenient: bool, to_unit: Option<&str>, bounds: Bounds) -> Result<String, String> {
    if let Some(unit) = to_unit {
        if bytesize::is_byte_unit(unit, lenient) {
            let bytes = bytesize::parse_bytes(value, lenient).map_err(|e| e.to_string())?;
            check_bytes_bounds(bytes, bounds, lenient)?;
            let converted = bytesize::unit_value(bytes, unit, lenient).map_err(|e| e.to_string())?;
            return Ok(format!("{value} => {converted} {unit}"));
        }
        if duration::is_duration_unit(unit, lenient) {
            let d = duration::parse_duration(value, lenient).map_err(|e| e.to_string())?;
            check_duration_bounds(d, bounds, lenient)?;
            let converted = duration::unit_value(d, unit, lenient).map_err(|e| e.to_string())?;
            return Ok(format!("{value} => {converted} {unit}"));
        }
        return Err(format!(
            "{unit:?} is not a recognized byte size or duration unit"
        ));
    }

    match bytesize::parse_bytes(value, lenient) {
        Ok(bytes) => {
            check_bytes_bounds(bytes, bounds, lenient)?;
            Ok(format!(
                "{value} => {bytes} bytes ({})",
                bytesize::format_bytes(bytes)
            ))
        }
        Err(byte_err) => match duration::parse_duration(value, lenient) {
            Ok(d) => {
                check_duration_bounds(d, bounds, lenient)?;
                Ok(format!(
                    "{value} => {}ns ({})",
                    d.as_nanos(),
                    duration::format_duration(d)
                ))
            }
            Err(dur_err) => Err(format!(
                "not a valid byte size ({byte_err}) or duration ({dur_err})"
            )),
        },
    }
}

// Same domain-detection logic as process_one, but emits a single-line JSON
// object per value instead of the human-readable line. Kept as a separate
// function rather than a formatting branch inside process_one because the
// two output shapes carry different fields (e.g. plain text folds the
// canonical value into a sentence; JSON needs it as its own typed field).
fn process_one_json(
    value: &str,
    lenient: bool,
    to_unit: Option<&str>,
    bounds: Bounds,
) -> Result<String, String> {
    if let Some(unit) = to_unit {
        if bytesize::is_byte_unit(unit, lenient) {
            let bytes = bytesize::parse_bytes(value, lenient).map_err(|e| e.to_string())?;
            check_bytes_bounds(bytes, bounds, lenient)?;
            let converted = bytesize::unit_value(bytes, unit, lenient).map_err(|e| e.to_string())?;
            return Ok(format!(
                "{{\"input\":{},\"domain\":\"bytes\",\"bytes\":{bytes},\"value\":{converted},\"unit\":{}}}",
                json_string(value),
                json_string(unit),
            ));
        }
        if duration::is_duration_unit(unit, lenient) {
            let d = duration::parse_duration(value, lenient).map_err(|e| e.to_string())?;
            check_duration_bounds(d, bounds, lenient)?;
            let converted = duration::unit_value(d, unit, lenient).map_err(|e| e.to_string())?;
            return Ok(format!(
                "{{\"input\":{},\"domain\":\"duration\",\"nanoseconds\":{},\"value\":{converted},\"unit\":{}}}",
                json_string(value),
                d.as_nanos(),
                json_string(unit),
            ));
        }
        return Err(format!(
            "{unit:?} is not a recognized byte size or duration unit"
        ));
    }

    match bytesize::parse_bytes(value, lenient) {
        Ok(bytes) => {
            check_bytes_bounds(bytes, bounds, lenient)?;
            Ok(format!(
                "{{\"input\":{},\"domain\":\"bytes\",\"bytes\":{bytes},\"formatted\":{}}}",
                json_string(value),
                json_string(&bytesize::format_bytes(bytes)),
            ))
        }
        Err(byte_err) => match duration::parse_duration(value, lenient) {
            Ok(d) => {
                check_duration_bounds(d, bounds, lenient)?;
                Ok(format!(
                    "{{\"input\":{},\"domain\":\"duration\",\"nanoseconds\":{},\"formatted\":{}}}",
                    json_string(value),
                    d.as_nanos(),
                    json_string(&duration::format_duration(d)),
                ))
            }
            Err(dur_err) => Err(format!(
                "not a valid byte size ({byte_err}) or duration ({dur_err})"
            )),
        },
    }
}

/// Renders `s` as a double-quoted JSON string literal.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_BOUNDS: Bounds<'static> = Bounds { min: None, max: None };

    #[test]
    fn json_string_escapes_control_characters() {
        assert_eq!(json_string("10MB"), "\"10MB\"");
        assert_eq!(json_string("a\"b"), "\"a\\\"b\"");
        assert_eq!(json_string("a\\b"), "\"a\\\\b\"");
        assert_eq!(json_string("a\nb"), "\"a\\nb\"");
        assert_eq!(json_string("a\tb"), "\"a\\tb\"");
    }

    #[test]
    fn process_one_json_canonical_bytes() {
        let line = process_one_json("10MB", false, None, NO_BOUNDS).unwrap();
        assert_eq!(
            line,
            "{\"input\":\"10MB\",\"domain\":\"bytes\",\"bytes\":10000000,\"formatted\":\"9.54 MiB\"}"
        );
    }

    #[test]
    fn process_one_json_canonical_duration() {
        let line = process_one_json("1h30m", false, None, NO_BOUNDS).unwrap();
        assert_eq!(
            line,
            "{\"input\":\"1h30m\",\"domain\":\"duration\",\"nanoseconds\":5400000000000,\"formatted\":\"1h30m\"}"
        );
    }

    #[test]
    fn process_one_json_to_unit() {
        let line = process_one_json("10MB", false, Some("GB"), NO_BOUNDS).unwrap();
        assert_eq!(
            line,
            "{\"input\":\"10MB\",\"domain\":\"bytes\",\"bytes\":10000000,\"value\":0.01,\"unit\":\"GB\"}"
        );
    }

    #[test]
    fn process_one_json_reports_error() {
        let err = process_one_json("10mb", false, None, NO_BOUNDS).unwrap_err();
        assert!(err.contains("not a valid byte size"), "{err}");
    }

    #[test]
    fn process_one_rejects_below_min() {
        let bounds = Bounds { min: Some("1MB"), max: None };
        let err = process_one("1KB", false, None, bounds).unwrap_err();
        assert!(err.contains("below --min"), "{err}");
    }

    #[test]
    fn process_one_rejects_above_max() {
        let bounds = Bounds { min: None, max: Some("1h") };
        let err = process_one("2h", false, None, bounds).unwrap_err();
        assert!(err.contains("above --max"), "{err}");
    }

    #[test]
    fn process_one_accepts_value_within_bounds() {
        let bounds = Bounds { min: Some("1MB"), max: Some("1GB") };
        let line = process_one("10MB", false, None, bounds).unwrap();
        assert_eq!(line, "10MB => 10000000 bytes (9.54 MiB)");
    }

    #[test]
    fn process_one_bounds_respect_lenient() {
        let bounds = Bounds { min: Some("1mb"), max: None };
        assert!(process_one("5MB", false, None, bounds)
            .unwrap_err()
            .contains("not a valid byte size"));
        let line = process_one("5mb", true, None, bounds).unwrap();
        assert_eq!(line, "5mb => 5000000 bytes (4.77 MiB)");
    }

    #[test]
    fn process_one_bounds_apply_before_to_unit_conversion() {
        let bounds = Bounds { min: None, max: Some("1MB") };
        let err = process_one("10MB", false, Some("GB"), bounds).unwrap_err();
        assert!(err.contains("above --max"), "{err}");
    }
}
