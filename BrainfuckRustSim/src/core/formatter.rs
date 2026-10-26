//! Pretty-printer: puts every `[` and `]` on its own line and indents loop
//! bodies, while keeping comments (both free text and `#` comments).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatOptions {
    pub indent: Indent,
    /// Keep idioms like `[-]` on one line instead of splitting them.
    pub keep_short_loops: bool,
    pub hash_comments: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indent {
    Tab,
    Spaces(usize),
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self { indent: Indent::Spaces(4), keep_short_loops: true, hash_comments: true }
    }
}

impl Indent {
    fn write(self, out: &mut String, depth: usize) {
        match self {
            Indent::Tab => out.extend(std::iter::repeat_n('\t', depth)),
            Indent::Spaces(n) => out.extend(std::iter::repeat_n(' ', n * depth)),
        }
    }
}

struct Writer {
    out: String,
    opts: FormatOptions,
    depth: usize,
    /// Is there unfinished content on the current output line?
    open_line: bool,
}

impl Writer {
    fn newline(&mut self) {
        if self.open_line {
            self.out.push('\n');
            self.open_line = false;
        }
    }
    fn text(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        if !self.open_line {
            self.opts.indent.write(&mut self.out, self.depth);
            self.open_line = true;
        }
        self.out.push_str(s);
    }
    fn own_line(&mut self, s: &str) {
        self.newline();
        self.text(s);
        self.newline();
    }
}

/// Format Brainfuck source. Unbalanced brackets are tolerated (indentation
/// just clamps at zero) so formatting never destroys a broken file.
pub fn format(src: &str, opts: FormatOptions) -> String {
    let mut w = Writer { out: String::with_capacity(src.len() * 2), opts, depth: 0, open_line: false };
    let mut blank_run = 0;

    for line in src.lines() {
        let (code, comment) = match line.find('#') {
            Some(i) if opts.hash_comments => (&line[..i], Some(line[i..].trim_end())),
            _ => (line, None),
        };
        let code = code.trim();

        if code.is_empty() && comment.is_none() {
            blank_run += 1;
            if blank_run == 1 && !w.out.is_empty() {
                w.newline();
                w.out.push('\n');
            }
            continue;
        }
        blank_run = 0;

        format_code(&mut w, code);
        if let Some(c) = comment {
            if w.open_line {
                w.text(" ");
                w.text(c);
            } else {
                w.text(c);
            }
        }
        w.newline();
    }
    w.newline();
    // Trim trailing blank lines, end with exactly one newline.
    let trimmed = w.out.trim_end_matches('\n').len();
    w.out.truncate(trimmed);
    if !w.out.is_empty() {
        w.out.push('\n');
    }
    w.out
}

/// Split one line of code at brackets and emit it.
fn format_code(w: &mut Writer, code: &str) {
    let bytes = code.as_bytes();
    let mut seg_start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'[' => {
                if w.opts.keep_short_loops && is_short_loop(&bytes[i..]) {
                    i += 3;
                    continue;
                }
                w.text(code[seg_start..i].trim());
                w.own_line("[");
                w.depth += 1;
                seg_start = i + 1;
            }
            b']' => {
                w.text(code[seg_start..i].trim());
                w.depth = w.depth.saturating_sub(1);
                w.own_line("]");
                seg_start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    w.text(code[seg_start..].trim());
}

fn is_short_loop(b: &[u8]) -> bool {
    b.len() >= 3 && b[0] == b'[' && b[2] == b']' && matches!(b[1], b'-' | b'+' | b'<' | b'>')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{parser::parse, vm::run_source};

    #[test]
    fn indents_nested_loops() {
        let out = format("++[>++[>+<-]<-]", FormatOptions::default());
        assert_eq!(out, "++\n[\n    >++\n    [\n        >+<-\n    ]\n    <-\n]\n");
    }

    #[test]
    fn tabs_and_short_loops() {
        let opts = FormatOptions { indent: Indent::Tab, ..Default::default() };
        assert_eq!(format("+[[-]>]", opts), "+\n[\n\t[-]>\n]\n");
    }

    #[test]
    fn keeps_comments_and_breakpoints() {
        let src = "this adds two\n++ # two\n# BREAKPOINT\n[-]";
        let out = format(src, FormatOptions::default());
        assert_eq!(out, "this adds two\n++ # two\n# BREAKPOINT\n[-]\n");
    }

    #[test]
    fn collapses_blank_lines() {
        assert_eq!(format("+\n\n\n\n+\n\n", FormatOptions::default()), "+\n\n+\n");
    }

    #[test]
    fn idempotent_and_semantics_preserving() {
        let src = "++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.>>.<-.<.+++.------.--------.>>+.>++.";
        let once = format(src, FormatOptions::default());
        let twice = format(&once, FormatOptions::default());
        assert_eq!(once, twice);
        assert_eq!(run_source(&once, b"").unwrap(), run_source(src, b"").unwrap());
        assert_eq!(parse(&once).unwrap().ops, parse(src).unwrap().ops);
    }

    #[test]
    fn tolerates_unbalanced() {
        assert_eq!(format("]]+", FormatOptions::default()), "]\n]\n+\n");
    }
}
