// End-to-end tests that exercise the actual compiled binary: argument
// parsing, stdin reading, and exit codes are only real once a process
// boundary is involved, which the unit tests in src/ don't cross.

use std::io::Write;
use std::process::{Command, Stdio};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sizedur"))
}

fn run_with_stdin(args: &[&str], stdin: &str) -> (bool, String, String) {
    let mut child = bin()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn sizedur");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    (
        output.status.success(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

fn run(args: &[&str]) -> (bool, String, String) {
    run_with_stdin(args, "")
}

#[test]
fn prints_canonical_byte_size() {
    let (ok, stdout, _) = run(&["10MB"]);
    assert!(ok);
    assert_eq!(stdout, "10MB => 10000000 bytes (9.54 MiB)\n");
}

#[test]
fn prints_canonical_duration() {
    let (ok, stdout, _) = run(&["1h30m"]);
    assert!(ok);
    assert_eq!(stdout, "1h30m => 5400000000000ns (1h30m)\n");
}

#[test]
fn strict_mode_rejects_lowercase_unit() {
    let (ok, stdout, stderr) = run(&["10mb"]);
    assert!(!ok);
    assert!(stdout.is_empty());
    assert!(stderr.contains("not a valid byte size"), "{stderr}");
}

#[test]
fn lenient_flag_accepts_lowercase_unit() {
    let (ok, stdout, _) = run(&["--lenient", "10mb"]);
    assert!(ok);
    assert_eq!(stdout, "10mb => 10000000 bytes (9.54 MiB)\n");
}

#[test]
fn reads_multiple_literals_from_stdin() {
    let (ok, stdout, _) = run_with_stdin(&[], "10MB\n1h30m\n");
    assert!(ok);
    let mut lines = stdout.lines();
    assert_eq!(lines.next(), Some("10MB => 10000000 bytes (9.54 MiB)"));
    assert_eq!(lines.next(), Some("1h30m => 5400000000000ns (1h30m)"));
    assert_eq!(lines.next(), None);
}

#[test]
fn blank_stdin_lines_are_skipped() {
    let (ok, stdout, _) = run_with_stdin(&[], "10MB\n\n\n500ms\n");
    assert!(ok);
    assert_eq!(stdout, "10MB => 10000000 bytes (9.54 MiB)\n500ms => 500000000ns (500ms)\n");
}

#[test]
fn empty_stdin_and_no_args_prints_usage_and_fails() {
    let (ok, stdout, stderr) = run_with_stdin(&[], "");
    assert!(!ok);
    assert!(stdout.is_empty());
    assert!(stderr.contains("usage: sizedur"), "{stderr}");
}

#[test]
fn json_output_mode() {
    let (ok, stdout, _) = run(&["--json", "10MB"]);
    assert!(ok);
    assert_eq!(
        stdout,
        "{\"input\":\"10MB\",\"domain\":\"bytes\",\"bytes\":10000000,\"formatted\":\"9.54 MiB\"}\n"
    );
}

#[test]
fn json_output_reports_parse_errors_on_stderr() {
    let (ok, stdout, stderr) = run(&["--json", "10mb"]);
    assert!(!ok);
    assert!(stdout.is_empty());
    assert!(stderr.contains("\"input\":\"10mb\""), "{stderr}");
    assert!(stderr.contains("\"error\":"), "{stderr}");
}

#[test]
fn to_unit_conversion() {
    let (ok, stdout, _) = run(&["--to", "GB", "10MB"]);
    assert!(ok);
    assert_eq!(stdout, "10MB => 0.01 GB\n");
}

#[test]
fn min_max_bounds_reject_out_of_range_values() {
    let (ok, stdout, stderr) = run(&["--max", "1GB", "2GB"]);
    assert!(!ok);
    assert!(stdout.is_empty());
    assert!(stderr.contains("above --max"), "{stderr}");
}

#[test]
fn min_max_bounds_accept_in_range_values() {
    let (ok, stdout, _) = run(&["--min", "30s", "--max", "5m", "90s"]);
    assert!(ok);
    assert_eq!(stdout, "90s => 90000000000ns (1m30s)\n");
}

#[test]
fn help_flag_prints_usage_and_succeeds() {
    let (ok, stdout, stderr) = run(&["--help"]);
    assert!(ok);
    assert!(stdout.is_empty());
    assert!(stderr.contains("usage: sizedur"), "{stderr}");
}

#[test]
fn unrecognized_flag_is_rejected() {
    let (ok, stdout, stderr) = run(&["--bogus", "10MB"]);
    assert!(!ok);
    assert!(stdout.is_empty());
    assert!(stderr.contains("unrecognized flag: --bogus"), "{stderr}");
}

#[test]
fn multiple_values_continue_past_a_failure() {
    let (ok, stdout, stderr) = run(&["10MB", "10mb", "1h30m"]);
    assert!(!ok);
    assert_eq!(
        stdout,
        "10MB => 10000000 bytes (9.54 MiB)\n1h30m => 5400000000000ns (1h30m)\n"
    );
    assert!(stderr.contains("10mb: not a valid byte size"), "{stderr}");
}
