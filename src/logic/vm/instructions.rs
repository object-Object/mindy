use std::{borrow::Cow, collections::HashMap};

use super::{
    LogicVM, VMLoadError, VMLoadResult,
    processor::ProcessorState,
    variables::{LValue, LVar},
};
use crate::logic::ast;

const MAX_TEXT_BUFFER: usize = 400;
const MAX_IPT: usize = 1000;

pub fn parse_instruction(
    instruction: ast::Instruction,
    variables: &mut HashMap<String, LVar>,
    _labels: &HashMap<String, usize>,
    privileged: bool,
) -> VMLoadResult<Box<dyn Instruction>> {
    Ok(match instruction {
        // unprivileged
        ast::Instruction::Noop => Box::new(Noop),
        ast::Instruction::End => Box::new(End),
        ast::Instruction::Stop => Box::new(Stop),
        ast::Instruction::Set { to, from } => Box::new(Set {
            to: lvar(to, variables),
            from: lvar(from, variables),
        }),
        ast::Instruction::Print { value } => Box::new(Print {
            value: lvar(value, variables),
        }),

        // unknown
        // do this here so it isn't ignored for unprivileged procs
        ast::Instruction::Unknown(name) => {
            return Err(VMLoadError::BadProcessorCode(format!(
                "unknown instruction: {name}"
            )));
        }

        // privileged
        _ if !privileged => Box::new(Noop),
        ast::Instruction::SetRate { value } => Box::new(SetRate {
            value: lvar(value, variables),
        }),
    })
}

fn lvar(value: ast::Value, variables: &mut HashMap<String, LVar>) -> LVar {
    match value {
        ast::Value::Variable(name) => variables.get(&name).cloned().unwrap_or_else(|| {
            let var = LVar::new_variable();
            variables.insert(name, LVar::clone(&var));
            var
        }),
        ast::Value::String(value) => LVar::Constant(LValue::String(value.into())),
        ast::Value::Number(value) => LVar::Constant(value.into()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionResult {
    Ok,
    Yield,
}

pub trait Instruction {
    /// Returns true if more instructions can be executed,
    /// or false if the processor should yield for the rest of this tick.
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) -> InstructionResult;
}

trait SimpleInstruction {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM);
}

impl<T: SimpleInstruction> Instruction for T {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) -> InstructionResult {
        self.execute(state, vm);
        InstructionResult::Ok
    }
}

// unprivileged instructions

struct Noop;

impl SimpleInstruction for Noop {
    fn execute(&self, _: &mut ProcessorState, _: &LogicVM) {}
}

struct End;

impl SimpleInstruction for End {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        state.counter = state.num_instructions;
    }
}

struct Stop;

impl Instruction for Stop {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) -> InstructionResult {
        state.counter -= 1;
        state.set_stopped(true);
        InstructionResult::Yield
    }
}

struct Set {
    to: LVar,
    from: LVar,
}

impl SimpleInstruction for Set {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        self.to.set(state, self.from.get(state));
    }
}

struct Print {
    value: LVar,
}

impl Print {
    fn to_string(value: &LValue) -> Cow<'_, str> {
        match value {
            LValue::Null => Cow::from("null"),
            LValue::Number(n) => {
                let rounded = n.round() as u64;
                Cow::from(if (n - (rounded as f64)).abs() < 0.00001 {
                    rounded.to_string()
                } else {
                    n.to_string()
                })
            }
            LValue::String(s) => Cow::Borrowed(s),
        }
    }
}

impl SimpleInstruction for Print {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        if state.printbuffer.len() < MAX_TEXT_BUFFER {
            let value = self.value.get(state);
            state.printbuffer.push_str(&Print::to_string(&value))
        }
    }
}

// privileged instructions

struct SetRate {
    value: LVar,
}

impl SimpleInstruction for SetRate {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        state.ipt = (self.value.get(state).num() as usize).clamp(1, MAX_IPT);
    }
}
