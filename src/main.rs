mod bytesize;
mod duration;
mod error;

use std::env;
use std::io::{self, BufRead};
use std::process::ExitCode;

fn print_usage() {
    eprintln!("usage: sizedur [--lenient] [--to UNIT] [VALUE ...]");
    eprintln!();
    eprintln!("Parses byte size or duration literals and prints their canonical value.");
    eprintln!("With no VALUE arguments, reads one literal per line from stdin.");
    eprintln!();
    eprintln!("  --lenient   accept sloppy input (bare numbers, mixed-case units, extra whitespace)");
    eprintln!("  --to UNIT   convert to an explicit unit instead of printing the canonical value");
    eprintln!("              (bytes: B, KB, MB, GB, TB, PB, KiB, MiB, GiB, TiB, PiB;");
    eprintln!("               duration: ns, us, ms, s, m, h, d)");
    eprintln!("  --help      show this message");
}

fn main() -> ExitCode {
    let mut lenient = false;
    let mut to_unit: Option<String> = None;
    let mut values: Vec<String> = Vec::new();

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--lenient" => lenient = true,
            "--to" => match args.next() {
                Some(unit) => to_unit = Some(unit),
                None => {
                    eprintln!("--to requires a unit argument (e.g. --to GB, --to h)");
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

    let mut had_error = false;
    for value in &values {
        match process_one(value, lenient, to_unit.as_deref()) {
            Ok(line) => println!("{line}"),
            Err(err) => {
                eprintln!("{value}: {err}");
                had_error = true;
            }
        }
    }

    if had_error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

// A literal like "10s" is ambiguous between the two domains only in strict
// mode where both would be malformed anyway, so trying bytes first and
// falling back to duration never silently picks the wrong interpretation.
//
// When --to is given, the target unit itself tells us which domain to parse
// as, so there's no bytes-first guessing to do.
fn process_one(value: &str, lenient: bool, to_unit: Option<&str>) -> Result<String, String> {
    if let Some(unit) = to_unit {
        if bytesize::is_byte_unit(unit, lenient) {
            let bytes = bytesize::parse_bytes(value, lenient).map_err(|e| e.to_string())?;
            let converted = bytesize::unit_value(bytes, unit, lenient).map_err(|e| e.to_string())?;
            return Ok(format!("{value} => {converted} {unit}"));
        }
        if duration::is_duration_unit(unit, lenient) {
            let d = duration::parse_duration(value, lenient).map_err(|e| e.to_string())?;
            let converted = duration::unit_value(d, unit, lenient).map_err(|e| e.to_string())?;
            return Ok(format!("{value} => {converted} {unit}"));
        }
        return Err(format!(
            "{unit:?} is not a recognized byte size or duration unit"
        ));
    }

    match bytesize::parse_bytes(value, lenient) {
        Ok(bytes) => Ok(format!(
            "{value} => {bytes} bytes ({})",
            bytesize::format_bytes(bytes)
        )),
        Err(byte_err) => match duration::parse_duration(value, lenient) {
            Ok(d) => Ok(format!(
                "{value} => {}ns ({})",
                d.as_nanos(),
                duration::format_duration(d)
            )),
            Err(dur_err) => Err(format!(
                "not a valid byte size ({byte_err}) or duration ({dur_err})"
            )),
        },
    }
}
