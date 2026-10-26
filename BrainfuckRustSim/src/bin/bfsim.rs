//! `bfsim` command-line interface.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use bfsim::core::{
    self, CellWidth, EofBehavior, FormatOptions, Indent, Machine, ParseOptions, Severity, Status, VmConfig,
};
use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "bfsim", version, about = "-[>+<]-  a Brainfuck interpreter, debugger & simulator")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run a program (breakpoints are ignored)
    Run {
        file: PathBuf,
        #[command(flatten)]
        vm: VmArgs,
        /// Print execution stats to stderr when done
        #[arg(long)]
        stats: bool,
        /// Dump the first N tape cells to stderr when done
        #[arg(long, value_name = "N")]
        dump: Option<usize>,
    },
    /// Run interactively, pausing at `# BREAKPOINT` markers
    Debug {
        file: PathBuf,
        #[command(flatten)]
        vm: VmArgs,
    },
    /// Auto-format a file (indents loop bodies)
    Fmt {
        file: PathBuf,
        /// Overwrite the file instead of printing
        #[arg(short, long)]
        write: bool,
        /// Exit with 1 if the file isn't formatted
        #[arg(long, conflicts_with = "write")]
        check: bool,
        /// Indent with tabs instead of spaces
        #[arg(long)]
        tabs: bool,
        /// Spaces per indent level
        #[arg(long, default_value_t = 4)]
        indent: usize,
    },
    /// Check a file for syntax errors and lint warnings
    Check {
        file: PathBuf,
        /// Treat `#` as a normal character (standard Brainfuck)
        #[arg(long)]
        strict: bool,
    },
}

#[derive(Args, Clone)]
struct VmArgs {
    /// Input string fed to `,` (otherwise stdin is used)
    #[arg(short, long, conflicts_with = "input_file")]
    input: Option<String>,
    /// Read program input from a file
    #[arg(long, value_name = "PATH")]
    input_file: Option<PathBuf>,
    /// Number of tape cells
    #[arg(long, default_value_t = 30_000)]
    tape: usize,
    /// Cell size in bits
    #[arg(long, value_enum, default_value_t = Bits::B8)]
    cell: Bits,
    /// What `,` stores at end of input
    #[arg(long, value_enum, default_value_t = Eof::Zero)]
    eof: Eof,
    /// Wrap the pointer around the tape ends instead of erroring
    #[arg(long)]
    wrap: bool,
    /// Treat `#` as a normal character (standard Brainfuck)
    #[arg(long)]
    strict: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Bits {
    #[value(name = "8")]
    B8,
    #[value(name = "16")]
    B16,
    #[value(name = "32")]
    B32,
}

#[derive(Clone, Copy, ValueEnum)]
enum Eof {
    Zero,
    Unchanged,
    #[value(name = "minus1")]
    MinusOne,
}

impl VmArgs {
    fn parse_opts(&self) -> ParseOptions {
        ParseOptions { hash_comments: !self.strict, ..Default::default() }
    }

    fn config(&self, track_usage: bool) -> VmConfig {
        VmConfig {
            tape_len: self.tape,
            cell: match self.cell {
                Bits::B8 => CellWidth::U8,
                Bits::B16 => CellWidth::U16,
                Bits::B32 => CellWidth::U32,
            },
            eof: match self.eof {
                Eof::Zero => EofBehavior::Zero,
                Eof::Unchanged => EofBehavior::Unchanged,
                Eof::MinusOne => EofBehavior::MinusOne,
            },
            wrap_pointer: self.wrap,
            track_usage,
        }
    }

    /// Fixed input, if one was given (otherwise stdin streams lazily).
    fn fixed_input(&self) -> io::Result<Option<Vec<u8>>> {
        if let Some(s) = &self.input {
            return Ok(Some(unescape(s)));
        }
        if let Some(p) = &self.input_file {
            return Ok(Some(std::fs::read(p)?));
        }
        Ok(None)
    }
}

/// Support `\n`, `\t`, `\0` and `\\` in `--input`.
fn unescape(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.extend(c.to_string().bytes());
            continue;
        }
        match chars.next() {
            Some('n') => out.push(b'\n'),
            Some('t') => out.push(b'\t'),
            Some('0') => out.push(0),
            Some('\\') => out.push(b'\\'),
            Some(o) => {
                out.push(b'\\');
                out.extend(o.to_string().bytes());
            }
            None => out.push(b'\\'),
        }
    }
    out
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Run { file, vm, stats, dump } => cmd_run(&file, &vm, stats, dump),
        Cmd::Debug { file, vm } => cmd_debug(&file, &vm),
        Cmd::Fmt { file, write, check, tabs, indent } => cmd_fmt(&file, write, check, tabs, indent),
        Cmd::Check { file, strict } => cmd_check(&file, strict),
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("\x1b[31merror:\x1b[0m {e}");
            ExitCode::from(2)
        }
    }
}

type CmdResult = Result<ExitCode, Box<dyn std::error::Error>>;

fn read_source(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()).into())
}

fn load(path: &Path, args: &VmArgs, track_usage: bool) -> Result<Machine, Box<dyn std::error::Error>> {
    let src = read_source(path)?;
    let program = core::parse_with(&src, args.parse_opts(), &[])
        .map_err(|e| format!("{}:{e}", path.display()))?;
    let mut vm = Machine::new(program, args.config(track_usage));
    if let Some(input) = args.fixed_input()? {
        vm.feed_input(&input);
        vm.close_input();
    }
    Ok(vm)
}

/// Pull one line from stdin into the VM, or close input on EOF.
fn refill_from_stdin(vm: &mut Machine) -> io::Result<()> {
    io::stdout().flush()?;
    let mut buf = Vec::new();
    if io::stdin().lock().read_until(b'\n', &mut buf)? == 0 {
        vm.close_input();
    } else {
        vm.feed_input(&buf);
    }
    Ok(())
}

fn flush_output(vm: &mut Machine, out: &mut impl Write) -> io::Result<()> {
    if !vm.output.is_empty() {
        out.write_all(&vm.take_output())?;
        out.flush()?;
    }
    Ok(())
}

fn cmd_run(path: &Path, args: &VmArgs, stats: bool, dump: Option<usize>) -> CmdResult {
    let mut vm = load(path, args, false)?;
    vm.set_ignore_breakpoints(true);
    let mut stdout = io::stdout().lock();
    let start = Instant::now();

    let status = loop {
        match vm.run(1 << 22) {
            Status::Running => flush_output(&mut vm, &mut stdout)?,
            Status::NeedInput => {
                flush_output(&mut vm, &mut stdout)?;
                refill_from_stdin(&mut vm)?;
            }
            s => break s,
        }
    };
    flush_output(&mut vm, &mut stdout)?;
    let elapsed = start.elapsed();

    if stats {
        let secs = elapsed.as_secs_f64().max(1e-9);
        eprintln!(
            "\n\x1b[33m-- stats --\x1b[0m ops: {}  ({} after folding)  time: {:.3?}  speed: {:.1} Mops/s  ptr: {}  max cell used: {}",
            vm.steps,
            vm.program.len(),
            elapsed,
            vm.steps as f64 / secs / 1e6,
            vm.ptr,
            vm.high_water
        );
    }
    if let Some(n) = dump {
        eprintln!("{}", tape_window(&vm, 0, n));
    }
    match status {
        Status::Error(e) => {
            eprintln!("\x1b[31mruntime error:\x1b[0m {e} at line {}", vm.current_line().map_or(0, |l| l + 1));
            Ok(ExitCode::from(1))
        }
        _ => Ok(ExitCode::SUCCESS),
    }
}

/// Render cells `[from, from+n)` as a boxed strip with the pointer marked.
fn tape_window(vm: &Machine, from: usize, n: usize) -> String {
    let to = (from + n).min(vm.tape.len());
    let mut idx = String::from("  idx ");
    let mut val = String::from("  val ");
    let mut chr = String::from("  chr ");
    let mut arrow = String::from("      ");
    for i in from..to {
        let v = vm.tape[i];
        let c = char::from_u32(v).filter(|c| c.is_ascii_graphic()).unwrap_or('·');
        idx.push_str(&format!("{i:>5}"));
        val.push_str(&format!("{v:>5}"));
        chr.push_str(&format!("{c:>5}"));
        arrow.push_str(if i == vm.ptr { "    ^" } else { "     " });
    }
    format!("{idx}\n{val}\n{chr}\n\x1b[33m{arrow}\x1b[0m")
}

fn cmd_debug(path: &Path, args: &VmArgs) -> CmdResult {
    let src = read_source(path)?;
    let lines: Vec<&str> = src.lines().collect();
    let mut vm = load(path, args, true)?;
    let mut stdout = io::stdout().lock();
    let fixed_input = args.input.is_some() || args.input_file.is_some();
    let mut stdin_buf = String::new();

    eprintln!(
        "\x1b[33mbfsim debug\x1b[0m — {} ops, {} breakpoints. Commands: [c]ontinue, [s]tep, [t]ape, [q]uit",
        vm.program.len(),
        vm.program.breakpoint_count()
    );

    let mut single_step = false;
    loop {
        let status = if single_step { vm.step() } else { vm.run(1 << 22) };
        flush_output(&mut vm, &mut stdout)?;
        match status {
            Status::Running if !single_step => continue,
            Status::NeedInput => {
                if fixed_input {
                    vm.close_input();
                } else {
                    refill_from_stdin(&mut vm)?;
                }
                continue;
            }
            Status::Halted => {
                eprintln!("\n\x1b[32m■ halted\x1b[0m after {} ops", vm.steps);
                return Ok(ExitCode::SUCCESS);
            }
            Status::Error(e) => {
                eprintln!("\n\x1b[31m✖ {e}\x1b[0m");
                eprintln!("{}", tape_window(&vm, vm.ptr.saturating_sub(5), 11));
                return Ok(ExitCode::from(1));
            }
            Status::Running | Status::Breakpoint => {}
        }

        // Paused: show where we are.
        let line = vm.current_line().unwrap_or(0);
        let label = if status == Status::Breakpoint { "\x1b[31m● breakpoint\x1b[0m" } else { "\x1b[36m→ step\x1b[0m" };
        eprintln!(
            "\n{label} line {}  pc {}  op {:?}  ptr {}  ops {}",
            line + 1,
            vm.pc,
            vm.program.ops.get(vm.pc),
            vm.ptr,
            vm.steps
        );
        eprintln!("  \x1b[2m{:>4} |\x1b[0m {}", line + 1, lines.get(line).unwrap_or(&"").trim_end());
        eprintln!("{}", tape_window(&vm, vm.ptr.saturating_sub(5), 11));

        loop {
            eprint!("\x1b[33m(bfsim)\x1b[0m ");
            io::stderr().flush()?;
            stdin_buf.clear();
            if io::stdin().read_line(&mut stdin_buf)? == 0 {
                return Ok(ExitCode::SUCCESS);
            }
            match stdin_buf.trim() {
                "" | "c" | "continue" => {
                    single_step = false;
                    break;
                }
                "s" | "step" => {
                    single_step = true;
                    break;
                }
                "t" | "tape" => eprintln!("{}", tape_window(&vm, 0, vm.high_water.max(vm.ptr) + 1)),
                "q" | "quit" => return Ok(ExitCode::SUCCESS),
                other => eprintln!("unknown command `{other}` (c, s, t, q)"),
            }
        }
    }
}

fn cmd_fmt(path: &Path, write: bool, check: bool, tabs: bool, indent: usize) -> CmdResult {
    let src = read_source(path)?;
    let opts = FormatOptions { indent: if tabs { Indent::Tab } else { Indent::Spaces(indent) }, ..Default::default() };
    let formatted = core::format(&src, opts);
    if check {
        return Ok(if formatted == src {
            ExitCode::SUCCESS
        } else {
            eprintln!("{} is not formatted", path.display());
            ExitCode::from(1)
        });
    }
    if write {
        std::fs::write(path, &formatted)?;
        eprintln!("formatted {}", path.display());
    } else {
        io::stdout().write_all(formatted.as_bytes())?;
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_check(path: &Path, strict: bool) -> CmdResult {
    let src = read_source(path)?;
    let diags = core::check(&src, ParseOptions { hash_comments: !strict, ..Default::default() });
    for d in &diags {
        let color = match d.severity {
            Severity::Error => "31",
            Severity::Warning => "33",
            Severity::Hint => "36",
        };
        eprintln!("{}:{}:{}: \x1b[{color}m{}\x1b[0m: {}", path.display(), d.line + 1, d.col + 1, d.severity.label(), d.message);
    }
    let errors = diags.iter().filter(|d| d.severity == Severity::Error).count();
    if errors > 0 {
        eprintln!("\x1b[31m✖ {errors} error(s)\x1b[0m");
        Ok(ExitCode::from(1))
    } else {
        eprintln!("\x1b[32m✔ {} looks good\x1b[0m ({} note(s))", path.display(), diags.len());
        Ok(ExitCode::SUCCESS)
    }
}
