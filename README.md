# sizedur

A command line tool that parses byte size and duration literals ("10MB",
"1h30m", "500ms") and prints their exact canonical value.

## Why

Config files, env vars, and CLI flags are full of values like `10MB` or
`30s`. Every tool that reads them makes its own judgment call about what
counts as valid: is `10MB` decimal (10,000,000) or binary (10,485,760)? Is
`10mb` the same as `10MB`? Does `5` mean bytes, or is it an error? Those
judgment calls are usually undocumented and inconsistent between tools, which
is how a config value that "looked fine" ends up meaning something else than
whoever wrote it intended.

`sizedur` picks one strict interpretation and sticks to it. It exists so you
can validate or normalize these literals — in a script, a test, or while
writing a config file by hand — without guessing at some other tool's rules.

## Usage

```
$ sizedur 10MB
10MB => 10000000 bytes (9.54 MiB)

$ sizedur 1.5GiB
1.5GiB => 1610612736 bytes (1.50 GiB)

$ sizedur 1h30m
1h30m => 5400000000000ns (1h30m)

$ sizedur 500ms
500ms => 500000000ns (500ms)
```

Strict mode (the default) rejects anything sloppy:

```
$ sizedur 10mb
10mb: not a valid byte size ("mb" is not a recognized unit; expected one of
B, KB, MB, GB, TB, PB, KiB, MiB, GiB, TiB, PiB (case-sensitive)) or duration
("mb" is not a recognized duration unit; expected one of ns, us, ms, s, m, h, d)

$ sizedur 512
512: not a valid byte size ("512" is missing a unit suffix (e.g. B, KB, MiB);
use --lenient to treat bare numbers as bytes) or duration ("512" is missing a
unit after "512"; expected one of ns, us, ms, s, m, h, d)
```

Pass `--lenient` to accept the sloppy versions instead of rejecting them:

```
$ sizedur --lenient 10mb
10mb => 10000000 bytes (9.54 MiB)

$ sizedur --lenient 512
512 => 512 bytes (512 B)
```

With no arguments, it reads one literal per line from stdin, which is useful
for checking a whole config file at once:

```
$ grep -E '^(size|timeout)' app.conf | cut -d= -f2 | sizedur
```

## Strict rules

Byte sizes:

- a unit suffix is required: `B`, `KB`, `MB`, `GB`, `TB`, `PB` (decimal,
  powers of 1000) or `KiB`, `MiB`, `GiB`, `TiB`, `PiB` (binary, powers of
  1024)
- the unit must match case exactly — `Mb` and `mb` are rejected
- no whitespace anywhere in the literal
- the result must be a whole number of bytes

Durations:

- a unit is required on every component: `ns`, `us`, `ms`, `s`, `m`, `h`, `d`
- the unit must match case exactly
- compound literals like `1h30m` must list units in strictly descending
  order, each used at most once — `30m1h` and `1h1h` are both rejected
- no whitespace anywhere in the literal
- the result must be a whole number of nanoseconds

## `--lenient`

Relaxes all of the above: whitespace is trimmed, units are matched
case-insensitively, single-letter decimal shorthand (`K`, `M`, `G`, `T`, `P`)
is accepted for byte sizes, duration components can repeat and appear in any
order (values are summed), a bare number is read as bytes or seconds
depending on context, and fractional results are rounded instead of
rejected.

## Building

Standard library only, no third-party crates.

```
cargo build --release
```

## License

MIT, see LICENSE.
