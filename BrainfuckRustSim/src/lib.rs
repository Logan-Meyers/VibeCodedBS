//! `bfsim`: a Brainfuck interpreter, debugger and simulator.
//!
//! - [`core`] — parser, VM, formatter and diagnostics (dependency-free)
//! - [`gui`] — the egui desktop app (behind the `gui` feature)

pub mod core;

#[cfg(feature = "gui")]
pub mod gui;
