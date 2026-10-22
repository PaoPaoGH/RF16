#![allow(dead_code)]
use std::collections::{HashMap, VecDeque};

pub const RAM_SIZE: usize = 65536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Add(i16),
    Move(i16),

    Set(u8),

    Input,
    Output,
    Debug,

    JumpIfZero(usize),
    JumpIfNonZero(usize),

    Clear,
    Halt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepEvent {
    Continue,
    Output,
    Halt,
}

pub struct VM {
    pub ram: [u8; RAM_SIZE],
    pub ptr: usize,
    pub pc: usize,

    pub running: bool,

    pub program: Vec<Op>,

    pub input_buffer: VecDeque<u8>,
    pub output_buffer: Vec<u8>,

    #[cfg(feature = "bench")]
    pub steps: u64,
}

impl VM {
    pub fn new(program: Vec<Op>) -> Self {
        Self {
            ram: [0; RAM_SIZE],
            ptr: 0,
            pc: 0,

            running: true,

            program,
            input_buffer: VecDeque::new(),
            output_buffer: Vec::new(),

            #[cfg(feature = "bench")]
            steps: 0,
        }
    }

    pub fn current_cell(&mut self) -> &mut u8 {
        &mut self.ram[self.ptr]
    }

    pub fn reset(&mut self) {
        self.ram = [0; RAM_SIZE];
        self.ptr = 0;
        self.pc = 0;
        self.running = true;
        self.input_buffer.clear();
        self.output_buffer.clear();
    }

    pub fn step(&mut self) -> StepEvent {
        self.step_with_input(|| None)
    }

    pub fn step_with_input<F>(&mut self, mut read_input: F) -> StepEvent
    where
        F: FnMut() -> Option<u8>,
    {
        if !self.running {
            return StepEvent::Halt;
        }

        if self.pc >= self.program.len() {
            self.running = false;
            return StepEvent::Halt;
        }

        let op = self.program[self.pc];

        match op {
            Op::Add(v) => {
                let cell = self.current_cell();
                *cell = cell.wrapping_add(v as u8);
            }

            Op::Move(v) => {
                self.ptr = ((self.ptr as isize + v as isize) as usize) % RAM_SIZE;
            }

            Op::Set(v) => {
                *self.current_cell() = v;
            }

            Op::Input => {
                let value = read_input()
                    .or_else(|| self.input_buffer.pop_front())
                    .unwrap_or(0);

                *self.current_cell() = value;
            }

            Op::Output => {
                self.output_buffer.push(self.ram[self.ptr]);
                self.pc += 1;
                return StepEvent::Output;
            }

            Op::Debug => {
                println!("memory[{}]: {}", self.ptr, self.ram[self.ptr]);
            }

            Op::JumpIfZero(target) => {
                if *self.current_cell() == 0 {
                    self.pc = target;
                    return StepEvent::Continue;
                }
            }

            Op::JumpIfNonZero(target) => {
                if *self.current_cell() != 0 {
                    self.pc = target;
                    return StepEvent::Continue;
                }
            }

            Op::Clear => {
                *self.current_cell() = 0;
            }

            Op::Halt => {
                self.running = false;
                return StepEvent::Halt;
            }
        }

        #[cfg(feature = "bench")]
        {
            self.steps += 1;
        }
        self.pc += 1;
        StepEvent::Continue
    }

    pub fn run(&mut self) {
        while self.running {
            self.step();
        }
    }

    pub fn run_until_output_with_input<F>(&mut self, mut read_input: F) -> bool
    where
        F: FnMut() -> u8,
    {
        loop {
            match self.step_with_input(|| Some(read_input())) {
                StepEvent::Continue => {}
                StepEvent::Output => return true,
                StepEvent::Halt => return false,
            }
        }
    }
}

pub fn compile_bf(source: &str) -> Vec<Op> {
    let chars: Vec<char> = source.chars().collect();

    let mut program = Vec::new();
    let mut loop_stack = Vec::new();
    let mut jump_map: HashMap<usize, usize> = HashMap::new();

    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            '+' | '-' => {
                let mut value = 0i16;

                while i < chars.len() {
                    match chars[i] {
                        '+' => value += 1,
                        '-' => value -= 1,
                        _ => break,
                    }

                    i += 1;
                }

                if value != 0 {
                    program.push(Op::Add(value));
                }

                continue;
            }

            '>' | '<' => {
                let mut value = 0i16;

                while i < chars.len() {
                    match chars[i] {
                        '>' => value += 1,
                        '<' => value -= 1,
                        _ => break,
                    }

                    i += 1;
                }

                if value != 0 {
                    program.push(Op::Move(value));
                }

                continue;
            }

            '.' => {
                program.push(Op::Output);
            }

            ',' => {
                program.push(Op::Input);
            }

            '?' => {
                program.push(Op::Debug);
            }

            '[' => {
                // Peephole: [-] and [+] are both clear loops. Since cells are u8 and
                // wrapping is defined, decrementing or incrementing any nonzero value
                // will eventually reach zero, so both forms are safe to fold into Clear.
                let is_clear_loop = i + 2 < chars.len()
                    && (chars[i + 1] == '-' || chars[i + 1] == '+')
                    && chars[i + 2] == ']';

                if is_clear_loop {
                    program.push(Op::Clear);
                    i += 3;
                    continue;
                }

                let pos = program.len();
                loop_stack.push(pos);
                program.push(Op::JumpIfZero(0));
            }

            ']' => {
                let start = loop_stack.pop().expect("Unmatched closing bracket");

                let end = program.len();

                jump_map.insert(start, end);
                jump_map.insert(end, start);

                program.push(Op::JumpIfNonZero(start));
            }

            _ => {}
        }

        i += 1;
    }

    assert!(loop_stack.is_empty(), "Unmatched opening bracket");

    for (from, to) in jump_map {
        match &mut program[from] {
            Op::JumpIfZero(target) => *target = to,
            Op::JumpIfNonZero(target) => *target = to,
            _ => {}
        }
    }

    program.push(Op::Halt);

    program
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_coalesces_runs_and_keeps_frame_outputs() {
        assert_eq!(
            compile_bf("+++-->>.<"),
            vec![Op::Add(1), Op::Move(2), Op::Output, Op::Move(-1), Op::Halt]
        );
    }

    #[test]
    fn compile_folds_decrement_clear_loop_into_clear_op() {
        assert_eq!(compile_bf("[-]"), vec![Op::Clear, Op::Halt]);
    }

    #[test]
    fn compile_folds_increment_clear_loop_into_clear_op() {
        // [+] also clears — wrapping u8 arithmetic guarantees it terminates at zero.
        assert_eq!(compile_bf("[+]"), vec![Op::Clear, Op::Halt]);
    }

    #[test]
    fn clear_op_zeroes_nonzero_cell() {
        let mut vm = VM::new(compile_bf("+++[-]"));
        vm.run();
        assert_eq!(vm.ram[0], 0);
    }

    #[test]
    fn vm_runs_loops_and_wraps_cells() {
        let mut vm = VM::new(compile_bf("+++[>+<-]>."));

        assert!(vm.run_until_output_with_input(|| 0));
        assert_eq!(vm.output_buffer, vec![3]);
        assert_eq!(vm.ram[0], 0);
        assert_eq!(vm.ram[1], 3);
    }

    #[test]
    fn input_callback_is_used_for_each_input_instruction() {
        let inputs = [0x12, 0x34];
        let mut input_idx = 0;
        let mut vm = VM::new(compile_bf(",>,.<."));

        assert!(vm.run_until_output_with_input(|| {
            let value = inputs[input_idx];
            input_idx += 1;
            value
        }));
        assert_eq!(vm.output_buffer, vec![0x34]);

        assert!(vm.run_until_output_with_input(|| 0));
        assert_eq!(vm.output_buffer, vec![0x34, 0x12]);
    }
}
