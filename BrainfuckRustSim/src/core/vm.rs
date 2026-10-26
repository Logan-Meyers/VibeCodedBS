//! The Brainfuck machine: tape, pointer, program counter and I/O buffers.
//!
//! The VM is driven in slices (`run(budget)`) so both the CLI (big budgets)
//! and the GUI (a few thousand ops per frame) can share it.

use std::collections::VecDeque;

use super::parser::{Op, Program};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellWidth {
    #[default]
    U8,
    U16,
    U32,
}

impl CellWidth {
    pub fn mask(self) -> u32 {
        match self {
            CellWidth::U8 => 0xFF,
            CellWidth::U16 => 0xFFFF,
            CellWidth::U32 => u32::MAX,
        }
    }
    pub fn bits(self) -> u32 {
        match self {
            CellWidth::U8 => 8,
            CellWidth::U16 => 16,
            CellWidth::U32 => 32,
        }
    }
}

/// What `,` stores when the input is exhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EofBehavior {
    #[default]
    Zero,
    Unchanged,
    MinusOne,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmConfig {
    pub tape_len: usize,
    pub cell: CellWidth,
    pub eof: EofBehavior,
    /// Wrap the pointer around the tape ends instead of erroring.
    pub wrap_pointer: bool,
    /// Count reads/writes per cell (for the usage view). Costs a little speed.
    pub track_usage: bool,
}

impl Default for VmConfig {
    fn default() -> Self {
        Self {
            tape_len: 30_000,
            cell: CellWidth::U8,
            eof: EofBehavior::Zero,
            wrap_pointer: false,
            track_usage: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmError {
    PointerUnderflow { pc: usize },
    PointerOverflow { pc: usize },
}

impl std::fmt::Display for VmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VmError::PointerUnderflow { pc } => write!(f, "pointer moved left of cell 0 (op #{pc})"),
            VmError::PointerOverflow { pc } => write!(f, "pointer ran off the end of the tape (op #{pc})"),
        }
    }
}

impl std::error::Error for VmError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Budget ran out; call `run` again.
    Running,
    /// Paused before a breakpoint op (`pc` points at it).
    Breakpoint,
    /// `,` hit an empty input buffer that isn't closed yet.
    NeedInput,
    Halted,
    Error(VmError),
}

#[derive(Debug, Clone)]
pub struct Machine {
    pub config: VmConfig,
    pub program: Program,
    pub tape: Vec<u32>,
    pub ptr: usize,
    pub pc: usize,
    pub input: VecDeque<u8>,
    /// When closed, an empty input means EOF instead of `NeedInput`.
    pub input_closed: bool,
    pub output: Vec<u8>,
    /// Ops executed so far.
    pub steps: u64,
    /// Per-cell access counts (only when `track_usage`).
    pub usage: Vec<u32>,
    /// Highest cell index the pointer has reached.
    pub high_water: usize,
    /// Set after reporting a breakpoint so the next run steps over it.
    resume_past_break: bool,
    ignore_breakpoints: bool,
}

impl Machine {
    pub fn new(program: Program, config: VmConfig) -> Self {
        let tape_len = config.tape_len.max(1);
        Self {
            usage: if config.track_usage { vec![0; tape_len] } else { Vec::new() },
            tape: vec![0; tape_len],
            config: VmConfig { tape_len, ..config },
            program,
            ptr: 0,
            pc: 0,
            input: VecDeque::new(),
            input_closed: false,
            output: Vec::new(),
            steps: 0,
            high_water: 0,
            resume_past_break: false,
            ignore_breakpoints: false,
        }
    }

    /// Reset machine state but keep the program, config and pending input.
    pub fn reset(&mut self) {
        self.tape.iter_mut().for_each(|c| *c = 0);
        self.usage.iter_mut().for_each(|c| *c = 0);
        self.ptr = 0;
        self.pc = 0;
        self.output.clear();
        self.steps = 0;
        self.high_water = 0;
        self.resume_past_break = false;
    }

    pub fn set_ignore_breakpoints(&mut self, ignore: bool) {
        self.ignore_breakpoints = ignore;
    }

    pub fn feed_input(&mut self, bytes: &[u8]) {
        self.input.extend(bytes);
    }

    pub fn close_input(&mut self) {
        self.input_closed = true;
    }

    pub fn is_halted(&self) -> bool {
        self.pc >= self.program.ops.len()
    }

    pub fn current_cell(&self) -> u32 {
        self.tape[self.ptr]
    }

    /// Execute exactly one op (stepping over a breakpoint we're paused on).
    pub fn step(&mut self) -> Status {
        self.resume_past_break = true;
        self.run(1)
    }

    /// Run until halt, breakpoint, input starvation, error, or `budget` ops.
    pub fn run(&mut self, budget: u64) -> Status {
        let mask = self.config.cell.mask();
        let len = self.tape.len();
        let track = self.config.track_usage;
        let ops = &self.program.ops;
        let breaks = &self.program.breaks;
        let tape = &mut self.tape;
        let usage = &mut self.usage;
        let mut ptr = self.ptr;
        let mut pc = self.pc;
        let mut executed: u64 = 0;
        let check_breaks = !self.ignore_breakpoints;

        let status = loop {
            if pc >= ops.len() {
                break Status::Halted;
            }
            if executed >= budget {
                break Status::Running;
            }
            if check_breaks && breaks[pc] && !self.resume_past_break {
                self.resume_past_break = true;
                break Status::Breakpoint;
            }
            self.resume_past_break = false;

            match ops[pc] {
                Op::Add(n) => {
                    tape[ptr] = tape[ptr].wrapping_add(n as u32) & mask;
                    if track {
                        usage[ptr] += 1;
                    }
                }
                Op::Move(n) => {
                    let target = ptr as isize + n;
                    if target < 0 || target as usize >= len {
                        if !self.config.wrap_pointer {
                            break Status::Error(if target < 0 {
                                VmError::PointerUnderflow { pc }
                            } else {
                                VmError::PointerOverflow { pc }
                            });
                        }
                        ptr = target.rem_euclid(len as isize) as usize;
                    } else {
                        ptr = target as usize;
                    }
                    if ptr > self.high_water {
                        self.high_water = ptr;
                    }
                }
                Op::Out => {
                    self.output.push(tape[ptr] as u8);
                    if track {
                        usage[ptr] += 1;
                    }
                }
                Op::In => match self.input.pop_front() {
                    Some(b) => {
                        tape[ptr] = b as u32 & mask;
                        if track {
                            usage[ptr] += 1;
                        }
                    }
                    None if !self.input_closed => {
                        // Retry this op without re-reporting its breakpoint.
                        self.resume_past_break = true;
                        break Status::NeedInput;
                    }
                    None => match self.config.eof {
                        EofBehavior::Zero => tape[ptr] = 0,
                        EofBehavior::MinusOne => tape[ptr] = mask,
                        EofBehavior::Unchanged => {}
                    },
                },
                Op::JumpIfZero(target) => {
                    if tape[ptr] == 0 {
                        pc = target;
                        executed += 1;
                        continue;
                    }
                }
                Op::JumpIfNonZero(target) => {
                    if tape[ptr] != 0 {
                        pc = target;
                        executed += 1;
                        continue;
                    }
                }
                Op::Clear => {
                    tape[ptr] = 0;
                    if track {
                        usage[ptr] += 1;
                    }
                }
                Op::Scan(step) => {
                    while tape[ptr] != 0 {
                        let target = ptr as isize + step;
                        if target < 0 || target as usize >= len {
                            if !self.config.wrap_pointer {
                                self.ptr = ptr;
                                self.pc = pc;
                                self.steps += executed;
                                return Status::Error(if target < 0 {
                                    VmError::PointerUnderflow { pc }
                                } else {
                                    VmError::PointerOverflow { pc }
                                });
                            }
                            ptr = target.rem_euclid(len as isize) as usize;
                        } else {
                            ptr = target as usize;
                        }
                        if track {
                            usage[ptr] += 1;
                        }
                    }
                    if ptr > self.high_water {
                        self.high_water = ptr;
                    }
                }
            }
            pc += 1;
            executed += 1;
        };

        self.ptr = ptr;
        self.pc = pc;
        self.steps += executed;
        status
    }

    /// Run to completion with the input closed (ignores breakpoints).
    pub fn run_to_end(&mut self) -> Status {
        self.close_input();
        let old = self.ignore_breakpoints;
        self.ignore_breakpoints = true;
        let status = loop {
            match self.run(u64::MAX) {
                Status::Running => continue,
                s => break s,
            }
        };
        self.ignore_breakpoints = old;
        status
    }

    /// The largest value currently on the tape (for heat-map scaling).
    pub fn max_cell(&self) -> u32 {
        self.tape.iter().copied().max().unwrap_or(0)
    }

    /// Source line of the op about to run, if any.
    pub fn current_line(&self) -> Option<usize> {
        self.program.lines.get(self.pc).copied()
    }

    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }
}

/// Convenience: parse + run `src` with `input`, returning the output bytes.
pub fn run_source(src: &str, input: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let program = super::parser::parse(src)?;
    let mut vm = Machine::new(program, VmConfig::default());
    vm.feed_input(input);
    match vm.run_to_end() {
        Status::Error(e) => Err(Box::new(e)),
        _ => Ok(vm.output),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser::parse;

    fn vm(src: &str) -> Machine {
        Machine::new(parse(src).unwrap(), VmConfig { track_usage: true, ..Default::default() })
    }

    #[test]
    fn hello_world() {
        let src = "++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.>>.<-.<.+++.------.--------.>>+.>++.";
        assert_eq!(run_source(src, b"").unwrap(), b"Hello World!\n");
    }

    #[test]
    fn wraps_cells() {
        let mut m = vm("-");
        m.run_to_end();
        assert_eq!(m.tape[0], 255);
        let mut m = Machine::new(parse("-").unwrap(), VmConfig { cell: CellWidth::U16, ..Default::default() });
        m.run_to_end();
        assert_eq!(m.tape[0], 65535);
    }

    #[test]
    fn pointer_errors() {
        assert!(matches!(vm("<").run_to_end(), Status::Error(VmError::PointerUnderflow { .. })));
        let mut m = Machine::new(parse(">>>").unwrap(), VmConfig { tape_len: 2, ..Default::default() });
        assert!(matches!(m.run_to_end(), Status::Error(VmError::PointerOverflow { .. })));
        let mut m = Machine::new(parse("<").unwrap(), VmConfig { tape_len: 4, wrap_pointer: true, ..Default::default() });
        assert_eq!(m.run_to_end(), Status::Halted);
        assert_eq!(m.ptr, 3);
    }

    #[test]
    fn input_and_eof() {
        let mut m = vm(",.,.");
        m.feed_input(b"A");
        assert_eq!(m.run(100), Status::NeedInput);
        assert_eq!(m.output, b"A");
        m.close_input();
        assert_eq!(m.run(100), Status::Halted);
        assert_eq!(m.output, b"A\0");
    }

    #[test]
    fn eof_modes() {
        for (eof, want) in [(EofBehavior::Zero, 0), (EofBehavior::MinusOne, 255), (EofBehavior::Unchanged, 7)] {
            let mut m = Machine::new(parse("+++++++,").unwrap(), VmConfig { eof, ..Default::default() });
            m.run_to_end();
            assert_eq!(m.tape[0], want);
        }
    }

    #[test]
    fn breakpoints_pause_and_resume() {
        let mut m = vm("+\n# BREAKPOINT\n++\n# BREAKPOINT\n+++");
        assert_eq!(m.run(u64::MAX), Status::Breakpoint);
        assert_eq!(m.tape[0], 1);
        assert_eq!(m.current_line(), Some(2));
        assert_eq!(m.run(u64::MAX), Status::Breakpoint);
        assert_eq!(m.tape[0], 3);
        assert_eq!(m.run(u64::MAX), Status::Halted);
        assert_eq!(m.tape[0], 6);
    }

    #[test]
    fn breakpoint_inside_loop_hits_each_iteration() {
        let mut m = vm("+++[\n# BREAKPOINT\n-]");
        let mut hits = 0;
        while m.run(u64::MAX) == Status::Breakpoint {
            hits += 1;
        }
        assert_eq!(hits, 3);
    }

    #[test]
    fn step_moves_one_op() {
        let mut m = vm("++>+");
        assert_eq!(m.step(), Status::Running);
        assert_eq!((m.pc, m.tape[0]), (1, 2));
        m.step();
        assert_eq!(m.ptr, 1);
    }

    #[test]
    fn scan_and_usage() {
        let mut m = vm("+>+>+>>+<<<<[>]");
        m.run_to_end();
        assert_eq!(m.ptr, 3);
        assert!(m.usage[0] > 0 && m.usage[3] > 0);
        assert_eq!(m.usage[10], 0);
        assert_eq!(m.high_water, 4);
    }

    #[test]
    fn reset_clears_state() {
        let mut m = vm("+++.");
        m.run_to_end();
        m.reset();
        assert_eq!((m.pc, m.ptr, m.steps, m.tape[0]), (0, 0, 0, 0));
        assert!(m.output.is_empty());
    }
}
