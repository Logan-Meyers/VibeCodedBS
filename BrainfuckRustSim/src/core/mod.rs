//! Interpreter core: no GUI or CLI dependencies.

pub mod diagnostics;
pub mod formatter;
pub mod parser;
pub mod vm;

pub use diagnostics::{Diagnostic, Severity, check};
pub use formatter::{FormatOptions, Indent, format};
pub use parser::{Op, ParseOptions, Program, SyntaxError, parse, parse_with};
pub use vm::{CellWidth, EofBehavior, Machine, Status, VmConfig, VmError, run_source};
