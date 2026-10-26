//! End-to-end checks: example programs, formatter round-trips and the CLI.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use bfsim::core::{self, FormatOptions, Indent, Machine, ParseOptions, Status, VmConfig};

fn example(name: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join(name)).unwrap()
}

fn run(name: &str, input: &[u8]) -> Vec<u8> {
    core::run_source(&example(name), input).unwrap()
}

#[test]
fn hello_world() {
    assert_eq!(run("hello.bf", b""), b"Hello World!\n");
}

#[test]
fn digits() {
    assert_eq!(run("digits.bf", b""), b"0123456789\n");
}

#[test]
fn cat_and_reverse() {
    assert_eq!(run("cat.bf", b"meow\n"), b"meow\n");
    assert_eq!(run("reverse.bf", b"stressed"), b"desserts");
}

#[test]
fn squares() {
    let out = String::from_utf8(run("squares.bf", b"")).unwrap();
    let want: String = (0..=100).map(|i| format!("{}\n", i * i)).collect();
    assert_eq!(out, want);
}

#[test]
fn fibonacci_on_tape() {
    let mut vm = Machine::new(core::parse(&example("fibonacci.bf")).unwrap(), VmConfig::default());
    assert_eq!(vm.run(u64::MAX), Status::Breakpoint);
    assert_eq!(vm.run(u64::MAX), Status::Halted);
    assert_eq!(&vm.tape[..13], &[1, 1, 2, 3, 5, 8, 13, 21, 34, 55, 89, 144, 233]);
}

/// Exact fib(n) as a decimal string (schoolbook big-number addition).
fn fib_decimal(n: usize) -> String {
    // Digits least significant first.
    let (mut a, mut b) = (vec![0u8], vec![1u8]);
    for _ in 0..n {
        let mut sum = Vec::with_capacity(b.len() + 1);
        let mut carry = 0;
        for i in 0..b.len().max(a.len()) {
            let s = a.get(i).copied().unwrap_or(0) + b.get(i).copied().unwrap_or(0) + carry;
            sum.push(s % 10);
            carry = s / 10;
        }
        if carry > 0 {
            sum.push(carry);
        }
        a = std::mem::replace(&mut b, sum);
    }
    a.iter().rev().map(|d| char::from(b'0' + d)).collect()
}

fn run_fib_n(input: &str, config: VmConfig) -> String {
    let mut vm = Machine::new(core::parse(&example("fib_n.bf")).unwrap(), config);
    vm.feed_input(input.as_bytes());
    assert_eq!(vm.run_to_end(), Status::Halted);
    String::from_utf8(vm.output).unwrap()
}

#[test]
fn fib_n_known_values() {
    let cfg = VmConfig::default(); // plain 8-bit cells
    for (n, want) in [(0, "0"), (1, "1"), (2, "1"), (13, "233"), (14, "377"), (18, "2584"), (20, "6765")] {
        assert_eq!(run_fib_n(&format!("{n}\n"), cfg), format!("{want}\n"), "F_{n}");
    }
    assert_eq!(run_fib_n("100\n", cfg), "354224848179261915075\n");
}

#[test]
fn fib_n_matches_reference() {
    let cfg = VmConfig::default();
    // Every n up to 150, alternating newline and EOF endings.
    for n in 0..=150 {
        let input = if n % 2 == 0 { format!("{n}\n") } else { n.to_string() };
        assert_eq!(run_fib_n(&input, cfg), fib_decimal(n) + "\n", "F_{n}");
    }
    // A couple of big ones (F_1000 has 209 digits).
    for n in [500, 1000] {
        assert_eq!(run_fib_n(&n.to_string(), cfg), fib_decimal(n) + "\n", "F_{n}");
    }
}

#[test]
fn fib_n_input_quirks() {
    let cfg = VmConfig::default();
    assert_eq!(run_fib_n("0014\n", cfg), "377\n", "leading zeros");
    assert_eq!(run_fib_n("20\n30\n", cfg), "6765\n", "stops at the first newline");
    // Works the same with wider cells.
    let wide = VmConfig { cell: core::CellWidth::U32, ..Default::default() };
    assert_eq!(run_fib_n("90\n", wide), fib_decimal(90) + "\n");
}

#[test]
fn fib_n_breakpoint_holds_digits() {
    let mut vm = Machine::new(core::parse(&example("fib_n.bf")).unwrap(), VmConfig::default());
    vm.feed_input(b"12\n");
    assert_eq!(vm.run(u64::MAX), Status::Breakpoint);
    // a = 144, least significant digit first, one 8-cell group per digit.
    assert_eq!([vm.tape[25], vm.tape[33], vm.tape[41]], [4, 4, 1]);
    vm.close_input();
    assert_eq!(vm.run(u64::MAX), Status::Halted);
    assert_eq!(vm.output, b"144\n");
}

#[test]
fn hello_breakpoint_pauses_before_printing() {
    let mut vm = Machine::new(core::parse(&example("hello.bf")).unwrap(), VmConfig::default());
    assert_eq!(vm.run(u64::MAX), Status::Breakpoint);
    assert!(vm.output.is_empty());
    assert_eq!(vm.current_line(), Some(14));
    assert_eq!(vm.run(u64::MAX), Status::Halted);
    assert_eq!(vm.output, b"Hello World!\n");
}

#[test]
fn unoptimized_matches_optimized() {
    let opts = ParseOptions { optimize: false, ..Default::default() };
    for name in ["hello.bf", "squares.bf", "fibonacci.bf"] {
        let src = example(name);
        let mut a = Machine::new(core::parse(&src).unwrap(), VmConfig::default());
        let mut b = Machine::new(core::parse_with(&src, opts, &[]).unwrap(), VmConfig::default());
        a.run_to_end();
        b.run_to_end();
        assert_eq!(a.output, b.output, "{name}");
        assert_eq!(a.tape, b.tape, "{name}");
        assert!(b.steps > a.steps, "folding should save ops in {name}");
    }
}

#[test]
fn formatting_preserves_behaviour() {
    for name in ["hello.bf", "digits.bf", "squares.bf", "fibonacci.bf", "reverse.bf"] {
        let src = example(name);
        for indent in [Indent::Tab, Indent::Spaces(2), Indent::Spaces(4)] {
            let formatted = core::format(&src, FormatOptions { indent, ..Default::default() });
            assert_eq!(core::format(&formatted, FormatOptions { indent, ..Default::default() }), formatted, "{name} idempotent");
            assert_eq!(
                core::run_source(&formatted, b"abc").unwrap(),
                core::run_source(&src, b"abc").unwrap(),
                "{name} behaviour"
            );
            let p = core::parse(&formatted).unwrap();
            assert_eq!(p.breakpoint_count(), core::parse(&src).unwrap().breakpoint_count(), "{name} breakpoints");
        }
    }
}

#[test]
fn broken_example_has_diagnostics() {
    let diags = core::check(&example("broken.bf"), ParseOptions::default());
    let errors: Vec<_> = diags.iter().filter(|d| d.severity == core::Severity::Error).collect();
    assert_eq!(errors.len(), 2, "{diags:#?}");
    assert!(core::parse(&example("broken.bf")).is_err());
}

// ---- CLI ----

fn bfsim(args: &[&str], stdin: &[u8]) -> (i32, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_bfsim"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn cli_run() {
    let (code, out, _) = bfsim(&["run", "examples/hello.bf"], b"");
    assert_eq!((code, out.as_str()), (0, "Hello World!\n"));
}

#[test]
fn cli_run_with_stdin_and_input_flag() {
    let (_, out, _) = bfsim(&["run", "examples/cat.bf"], b"piped\n");
    assert_eq!(out, "piped\n");
    let (_, out, _) = bfsim(&["run", "examples/reverse.bf", "--input", "abc"], b"ignored");
    assert_eq!(out, "cba");
}

#[test]
fn cli_runtime_error_exit_code() {
    let dir = std::env::temp_dir().join(format!("bfsim-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("underflow.bf");
    std::fs::write(&f, "+<").unwrap();
    let (code, _, err) = bfsim(&["run", f.to_str().unwrap()], b"");
    assert_eq!(code, 1);
    assert!(err.contains("left of cell 0"), "{err}");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn cli_check() {
    let (code, _, err) = bfsim(&["check", "examples/broken.bf"], b"");
    assert_eq!(code, 1);
    assert!(err.contains("unmatched `]`") && err.contains("unclosed `[`"), "{err}");
    let (code, _, _) = bfsim(&["check", "examples/hello.bf"], b"");
    assert_eq!(code, 0);
}

#[test]
fn cli_fmt() {
    let (code, out, _) = bfsim(&["fmt", "examples/digits.bf", "--tabs"], b"");
    assert_eq!(code, 0);
    assert!(out.contains("[\n\t<.+>-"), "{out}");
    let (code, _, _) = bfsim(&["fmt", "examples/cat.bf", "--check"], b"");
    assert_eq!(code, 0, "cat.bf is already formatted");
}

#[test]
fn cli_debug_stops_at_breakpoints() {
    // runs to the breakpoint, then: step once, continue to the end
    let (code, out, err) = bfsim(&["debug", "examples/hello.bf"], b"s\nc\n");
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, "Hello World!\n");
    assert!(err.contains("● breakpoint") && err.contains("line 15"), "{err}");
    assert!(err.contains("→ step"), "{err}");
    assert!(err.contains("halted"), "{err}");
}
