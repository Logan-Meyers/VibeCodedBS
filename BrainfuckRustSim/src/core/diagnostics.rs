//! Static checks: syntax errors plus a few lint-style warnings.

use std::ops::Range;

use super::parser::{Lexeme, ParseOptions, SyntaxError, Token, lex, parse_with};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Hint,
    Warning,
    Error,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Hint => "hint",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// 0-based line.
    pub line: usize,
    /// 0-based column.
    pub col: usize,
    /// Byte span in the source.
    pub span: Range<usize>,
    pub message: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}: {}", self.line + 1, self.col + 1, self.severity.label(), self.message)
    }
}

fn diag(severity: Severity, t: &Token, len: usize, message: impl Into<String>) -> Diagnostic {
    Diagnostic { severity, line: t.line, col: t.col, span: t.offset..t.offset + len, message: message.into() }
}

/// Run all checks. Results are sorted by position.
pub fn check(src: &str, opts: ParseOptions) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let lexemes = lex(src, opts, &[]);
    let tokens: Vec<Token> = lexemes
        .iter()
        .filter_map(|l| match l {
            Lexeme::Cmd(t) => Some(*t),
            Lexeme::Break { .. } => None,
        })
        .collect();

    // Bracket balance — report *every* problem, not just the first.
    let mut stack: Vec<Token> = Vec::new();
    for t in &tokens {
        match t.ch {
            b'[' => stack.push(*t),
            b']' if stack.pop().is_none() => {
                out.push(diag(Severity::Error, t, 1, "unmatched `]` (no `[` to close)"));
            }
            _ => {}
        }
    }
    for t in &stack {
        out.push(diag(Severity::Error, t, 1, "unclosed `[` (missing `]`)"));
    }

    // Leading loop on a fresh (all-zero) tape never runs: the classic
    // "comment loop". Worth a hint since it's often intentional.
    if let Some(first) = tokens.first()
        && first.ch == b'['
    {
        out.push(diag(Severity::Hint, first, 1, "loop at program start never runs (cell is 0): comment block?"));
    }

    // Straight-line prefix: does the pointer go left of 0 before any loop?
    let mut pos: isize = 0;
    for t in &tokens {
        match t.ch {
            b'>' => pos += 1,
            b'<' => {
                pos -= 1;
                if pos < 0 {
                    out.push(diag(Severity::Warning, t, 1, "pointer moves left of cell 0"));
                    break;
                }
            }
            b'[' | b']' => break,
            _ => {}
        }
    }

    for w in tokens.windows(2) {
        let (a, b) = (w[0], w[1]);
        // Empty loop: infinite if entered.
        if a.ch == b'[' && b.ch == b']' {
            out.push(diag(Severity::Warning, &a, b.offset + 1 - a.offset, "empty loop `[]` spins forever if entered"));
        }
        // Self-cancelling neighbours, only when adjacent in the source.
        let cancels = matches!((a.ch, b.ch), (b'+', b'-') | (b'-', b'+') | (b'<', b'>') | (b'>', b'<'));
        if cancels && b.offset == a.offset + 1 {
            out.push(diag(Severity::Hint, &a, 2, format!("`{}{}` cancels out", a.ch as char, b.ch as char)));
        }
    }

    // Breakpoint with nothing after it.
    if let Some(Lexeme::Break { line }) = lexemes.last() {
        let offset = src.split('\n').take(*line).map(|l| l.len() + 1).sum();
        let len = src.split('\n').nth(*line).map_or(0, str::len);
        out.push(Diagnostic {
            severity: Severity::Warning,
            line: *line,
            col: 0,
            span: offset..offset + len,
            message: "breakpoint has no instruction after it".into(),
        });
    }

    out.sort_by_key(|d| (d.span.start, std::cmp::Reverse(d.severity)));
    out
}

/// True when any diagnostic is an error.
pub fn has_errors(diags: &[Diagnostic]) -> bool {
    diags.iter().any(|d| d.severity == Severity::Error)
}

impl From<SyntaxError> for Diagnostic {
    fn from(e: SyntaxError) -> Self {
        let (line, col) = match e {
            SyntaxError::UnmatchedClose { line, col, .. } | SyntaxError::UnclosedOpen { line, col, .. } => (line, col),
        };
        Diagnostic { severity: Severity::Error, line, col, span: e.offset()..e.offset() + 1, message: e.to_string() }
    }
}

/// Parse and check in one go (keeps `parse_with` and `check` consistent).
pub fn validate(src: &str, opts: ParseOptions) -> Result<(), Vec<Diagnostic>> {
    match parse_with(src, opts, &[]) {
        Ok(_) => Ok(()),
        Err(_) => Err(check(src, opts).into_iter().filter(|d| d.severity == Severity::Error).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msgs(src: &str) -> Vec<(Severity, usize, usize)> {
        check(src, ParseOptions::default()).iter().map(|d| (d.severity, d.line, d.col)).collect()
    }

    #[test]
    fn finds_all_bracket_errors() {
        let d = msgs("]\n[[+]");
        assert_eq!(d.iter().filter(|x| x.0 == Severity::Error).count(), 2);
        assert!(d.contains(&(Severity::Error, 0, 0)));
        assert!(d.contains(&(Severity::Error, 1, 0)));
    }

    #[test]
    fn clean_program_is_quiet() {
        assert!(msgs("++[>+<-]>.").is_empty());
    }

    #[test]
    fn lints() {
        assert!(msgs("+<").contains(&(Severity::Warning, 0, 1)));
        assert!(msgs("+[]").contains(&(Severity::Warning, 0, 1)));
        assert!(msgs("++-").contains(&(Severity::Hint, 0, 1)));
        assert!(msgs("[comment]+").contains(&(Severity::Hint, 0, 0)));
        assert!(msgs("+\n# BREAKPOINT").contains(&(Severity::Warning, 1, 0)));
    }

    #[test]
    fn validate_reports_errors() {
        assert!(validate("[[]", ParseOptions::default()).is_err());
        assert!(validate("[]", ParseOptions::default()).is_ok());
    }
}
