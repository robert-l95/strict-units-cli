mod bytesize;
mod duration;
mod error;

use std::env;
use std::io::{self, BufRead};
use std::process::ExitCode;

fn print_usage() {
    eprintln!("usage: sizedur [--lenient] [VALUE ...]");
    eprintln!();
    eprintln!("Parses byte size or duration literals and prints their canonical value.");
    eprintln!("With no VALUE arguments, reads one literal per line from stdin.");
    eprintln!();
    eprintln!("  --lenient   accept sloppy input (bare numbers, mixed-case units, extra whitespace)");
    eprintln!("  --help      show this message");
}

fn main() -> ExitCode {
    let mut lenient = false;
    let mut values: Vec<String> = Vec::new();

    for arg in env::args().skip(1) {
        match arg.as_str() {
            "--lenient" => lenient = true,
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
        match process_one(value, lenient) {
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
fn process_one(value: &str, lenient: bool) -> Result<String, String> {
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
