//! Source text → optimized op IR.
//!
//! The parser folds runs of `+-` and `<>` into single ops, recognises a few
//! idioms (`[-]`, `[>]`, `[<]`), precomputes jump targets, and records where
//! breakpoints sit. Every op keeps the byte span it came from so the GUI can
//! highlight the instruction being executed.

use std::ops::Range;

/// Marker that turns the rest of its line into a breakpoint.
pub const BREAKPOINT_MARKER: &str = "BREAKPOINT";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// Add a (possibly negative) amount to the current cell.
    Add(i32),
    /// Move the pointer by a (possibly negative) amount.
    Move(isize),
    Out,
    In,
    /// `[` — jump to the op *after* the matching `]` when the cell is zero.
    JumpIfZero(usize),
    /// `]` — jump to the op *after* the matching `[` when the cell is non-zero.
    JumpIfNonZero(usize),
    /// `[-]` / `[+]`
    Clear,
    /// `[>]` / `[<]` style scans: move by the step until a zero cell is found.
    Scan(isize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseOptions {
    /// Treat `#` as the start of a line comment (so prose like `# done.`
    /// doesn't print). Standard Brainfuck has no comment syntax, so `--strict`
    /// turns this off.
    pub hash_comments: bool,
    /// Fold runs and recognise idioms. Off = one op per source command.
    pub optimize: bool,
    /// Never fold across a line break, so every source line starts a fresh
    /// op. The GUI uses this so gutter breakpoints can be toggled live.
    pub split_lines: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self { hash_comments: true, optimize: true, split_lines: false }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyntaxError {
    UnmatchedClose { offset: usize, line: usize, col: usize },
    UnclosedOpen { offset: usize, line: usize, col: usize },
}

impl SyntaxError {
    pub fn offset(&self) -> usize {
        match *self {
            SyntaxError::UnmatchedClose { offset, .. } | SyntaxError::UnclosedOpen { offset, .. } => offset,
        }
    }
    pub fn line(&self) -> usize {
        match *self {
            SyntaxError::UnmatchedClose { line, .. } | SyntaxError::UnclosedOpen { line, .. } => line,
        }
    }
}

impl std::fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyntaxError::UnmatchedClose { line, col, .. } => {
                write!(f, "{}:{}: unmatched `]` (no `[` to close)", line + 1, col + 1)
            }
            SyntaxError::UnclosedOpen { line, col, .. } => {
                write!(f, "{}:{}: unclosed `[` (missing `]`)", line + 1, col + 1)
            }
        }
    }
}

impl std::error::Error for SyntaxError {}

#[derive(Debug, Clone, Default)]
pub struct Program {
    pub ops: Vec<Op>,
    /// Byte range in the source for each op.
    pub spans: Vec<Range<usize>>,
    /// 0-based source line of each op.
    pub lines: Vec<usize>,
    /// `breaks[pc]` is true when execution should pause before op `pc`.
    pub breaks: Vec<bool>,
}

impl Program {
    pub fn len(&self) -> usize {
        self.ops.len()
    }
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
    pub fn breakpoint_count(&self) -> usize {
        self.breaks.iter().filter(|b| **b).count()
    }
}

/// A raw source command with its position.
#[derive(Debug, Clone, Copy)]
pub struct Token {
    pub ch: u8,
    pub offset: usize,
    pub line: usize,
    pub col: usize,
}

/// Something the lexer found in a source line.
#[derive(Debug, Clone, Copy)]
pub enum Lexeme {
    Cmd(Token),
    /// A `# BREAKPOINT` marker (applies before the next command).
    Break { line: usize },
}

pub fn is_command(c: u8) -> bool {
    matches!(c, b'+' | b'-' | b'<' | b'>' | b'.' | b',' | b'[' | b']')
}

/// Returns the byte offset where a `#` comment starts in `line`, if any.
pub fn hash_comment_start(line: &str) -> Option<usize> {
    line.find('#')
}

/// True when the comment text (starting at `#`) is a breakpoint marker.
pub fn is_breakpoint_comment(comment: &str) -> bool {
    comment.trim_start_matches('#').trim_start().starts_with(BREAKPOINT_MARKER)
}

/// Split source into commands and breakpoint markers.
pub fn lex(src: &str, opts: ParseOptions, extra_break_lines: &[usize]) -> Vec<Lexeme> {
    let mut out = Vec::with_capacity(src.len());
    let mut offset = 0;
    for (line_no, line) in src.split('\n').enumerate() {
        if extra_break_lines.contains(&line_no) {
            out.push(Lexeme::Break { line: line_no });
        }
        let (code, comment) = match hash_comment_start(line) {
            Some(i) if opts.hash_comments => (&line[..i], Some(&line[i..])),
            Some(i) => (line, Some(&line[i..])),
            None => (line, None),
        };
        for (col, &b) in code.as_bytes().iter().enumerate() {
            if is_command(b) {
                out.push(Lexeme::Cmd(Token { ch: b, offset: offset + col, line: line_no, col }));
            }
        }
        if let Some(c) = comment
            && is_breakpoint_comment(c)
        {
            out.push(Lexeme::Break { line: line_no });
        }
        offset += line.len() + 1;
    }
    out
}

/// Parse with the default options and only in-file breakpoints.
pub fn parse(src: &str) -> Result<Program, SyntaxError> {
    parse_with(src, ParseOptions::default(), &[])
}

/// Parse `src`. `extra_break_lines` are 0-based lines that get a breakpoint
/// in addition to the `# BREAKPOINT` markers (used by the GUI gutter).
pub fn parse_with(src: &str, opts: ParseOptions, extra_break_lines: &[usize]) -> Result<Program, SyntaxError> {
    let lexemes = lex(src, opts, extra_break_lines);
    let mut prog = Program::default();
    let mut pending_break = false;
    let mut stack: Vec<(usize, Token)> = Vec::new();

    let mut i = 0;
    while i < lexemes.len() {
        let tok = match lexemes[i] {
            Lexeme::Break { .. } => {
                pending_break = true;
                i += 1;
                continue;
            }
            Lexeme::Cmd(t) => t,
        };
        let start = tok.offset;
        let brk = std::mem::take(&mut pending_break);

        // Try idioms first: [-] [+] [>] [<]
        if opts.optimize
            && tok.ch == b'['
            && let (Some(Lexeme::Cmd(mid)), Some(Lexeme::Cmd(end))) = (lexemes.get(i + 1), lexemes.get(i + 2))
            && end.ch == b']'
        {
            let op = match mid.ch {
                b'-' | b'+' => Some(Op::Clear),
                b'>' => Some(Op::Scan(1)),
                b'<' => Some(Op::Scan(-1)),
                _ => None,
            };
            if let Some(op) = op {
                push(&mut prog, op, start..end.offset + 1, tok.line, brk);
                i += 3;
                continue;
            }
        }

        match tok.ch {
            b'+' | b'-' | b'<' | b'>' => {
                let is_add = matches!(tok.ch, b'+' | b'-');
                let mut amount: i64 = 0;
                let mut end = start;
                // Fold a run of the same family, stopping at breakpoints.
                while let Some(Lexeme::Cmd(t)) = lexemes.get(i) {
                    if opts.split_lines && t.line != tok.line {
                        break;
                    }
                    let delta = match (t.ch, is_add) {
                        (b'+', true) | (b'>', false) => 1,
                        (b'-', true) | (b'<', false) => -1,
                        _ => break,
                    };
                    amount += delta;
                    end = t.offset + 1;
                    i += 1;
                    if !opts.optimize {
                        break;
                    }
                }
                // A run that cancels out (`+-`) still gets a no-op slot if it
                // carries a breakpoint, so the breakpoint isn't lost.
                if amount != 0 || brk {
                    let op = if is_add { Op::Add(amount as i32) } else { Op::Move(amount as isize) };
                    push(&mut prog, op, start..end, tok.line, brk);
                }
                continue;
            }
            b'.' => push(&mut prog, Op::Out, start..start + 1, tok.line, brk),
            b',' => push(&mut prog, Op::In, start..start + 1, tok.line, brk),
            b'[' => {
                stack.push((prog.ops.len(), tok));
                push(&mut prog, Op::JumpIfZero(0), start..start + 1, tok.line, brk);
            }
            b']' => {
                let Some((open, _)) = stack.pop() else {
                    return Err(SyntaxError::UnmatchedClose { offset: tok.offset, line: tok.line, col: tok.col });
                };
                let close = prog.ops.len();
                push(&mut prog, Op::JumpIfNonZero(open + 1), start..start + 1, tok.line, brk);
                prog.ops[open] = Op::JumpIfZero(close + 1);
            }
            _ => unreachable!(),
        }
        i += 1;
    }

    if let Some((_, tok)) = stack.pop() {
        return Err(SyntaxError::UnclosedOpen { offset: tok.offset, line: tok.line, col: tok.col });
    }
    Ok(prog)
}

fn push(prog: &mut Program, op: Op, span: Range<usize>, line: usize, brk: bool) {
    prog.ops.push(op);
    prog.spans.push(span);
    prog.lines.push(line);
    prog.breaks.push(brk);
}

/// Breakpoint flags for "pause at the first op on or after each of
/// `lines`", OR-ed with the program's own `base` flags.
pub fn breaks_for_lines(prog_lines: &[usize], base: &[bool], lines: &[usize]) -> Vec<bool> {
    let mut out = base.to_vec();
    for &l in lines {
        if let Some(pc) = prog_lines.iter().position(|&ol| ol >= l) {
            out[pc] = true;
        }
    }
    out
}

/// Line/column (0-based) for a byte offset.
pub fn line_col(src: &str, offset: usize) -> (usize, usize) {
    let before = &src[..offset.min(src.len())];
    let line = before.matches('\n').count();
    let col = before.len() - before.rfind('\n').map_or(0, |i| i + 1);
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_runs() {
        let p = parse("+++-->><").unwrap();
        assert_eq!(p.ops, vec![Op::Add(1), Op::Move(1)]);
    }

    #[test]
    fn cancelling_runs_vanish() {
        assert!(parse("+-<>").unwrap().is_empty());
    }

    #[test]
    fn jump_targets() {
        let p = parse("+[>+<-]").unwrap();
        // 0:Add 1:[ 2:> 3:+ 4:< 5:- 6:]
        assert_eq!(p.ops[1], Op::JumpIfZero(7));
        assert_eq!(p.ops[6], Op::JumpIfNonZero(2));
    }

    #[test]
    fn idioms() {
        let p = parse("[-][+][>][<]").unwrap();
        assert_eq!(p.ops, vec![Op::Clear, Op::Clear, Op::Scan(1), Op::Scan(-1)]);
    }

    #[test]
    fn unoptimized_is_one_to_one() {
        let opts = ParseOptions { optimize: false, ..Default::default() };
        let p = parse_with("++[-]", opts, &[]).unwrap();
        assert_eq!(p.len(), 5);
    }

    #[test]
    fn syntax_errors() {
        assert!(matches!(parse("+]"), Err(SyntaxError::UnmatchedClose { col: 1, .. })));
        assert!(matches!(parse("\n [+"), Err(SyntaxError::UnclosedOpen { line: 1, col: 1, .. })));
    }

    #[test]
    fn hash_comments_hide_commands() {
        assert!(parse("# hello, world.").unwrap().is_empty());
        let strict = ParseOptions { hash_comments: false, ..Default::default() };
        assert_eq!(parse_with("# hello, world.", strict, &[]).unwrap().ops, vec![Op::In, Op::Out]);
    }

    #[test]
    fn breakpoint_marker_splits_runs() {
        let p = parse("++\n# BREAKPOINT\n++").unwrap();
        assert_eq!(p.ops, vec![Op::Add(2), Op::Add(2)]);
        assert_eq!(p.breaks, vec![false, true]);
        assert_eq!(p.lines, vec![0, 2]);
    }

    #[test]
    fn inline_breakpoint_applies_to_next_line() {
        let p = parse("+ # BREAKPOINT\n>").unwrap();
        assert_eq!(p.breaks, vec![false, true]);
    }

    #[test]
    fn extra_breakpoint_lines() {
        let p = parse_with("+\n+\n+", ParseOptions::default(), &[1]).unwrap();
        assert_eq!(p.ops, vec![Op::Add(1), Op::Add(2)]);
        assert_eq!(p.breaks, vec![false, true]);
    }

    #[test]
    fn spans_cover_source() {
        let src = "ab++c[-]";
        let p = parse(src).unwrap();
        assert_eq!(&src[p.spans[0].clone()], "++");
        assert_eq!(&src[p.spans[1].clone()], "[-]");
    }

    #[test]
    fn split_lines_and_gutter_breaks() {
        let opts = ParseOptions { split_lines: true, ..Default::default() };
        let p = parse_with("++\n++\n\n>", opts, &[]).unwrap();
        assert_eq!(p.ops, vec![Op::Add(2), Op::Add(2), Op::Move(1)]);
        assert_eq!(breaks_for_lines(&p.lines, &p.breaks, &[2]), vec![false, false, true]);
    }

    #[test]
    fn line_col_works() {
        assert_eq!(line_col("ab\ncd", 4), (1, 1));
        assert_eq!(line_col("ab\ncd", 0), (0, 0));
    }
}
