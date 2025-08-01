use std::{borrow::Cow, collections::HashMap};

use super::{
    LogicVM, VMLoadError, VMLoadResult,
    processor::ProcessorState,
    variables::{LValue, LVar},
};
use crate::logic::ast::{self, ConditionOp};

const MAX_TEXT_BUFFER: usize = 400;
const MAX_IPT: usize = 1000;
const EQUALITY_EPSILON: f64 = 0.000001;
const PRINT_EPSILON: f64 = 0.00001;

pub fn parse_instruction(
    instruction: ast::Instruction,
    variables: &mut HashMap<String, LVar>,
    labels: &HashMap<String, usize>,
    privileged: bool,
    num_instructions: usize,
) -> VMLoadResult<Box<dyn Instruction>> {
    let mut lvar = |value| match value {
        ast::Value::Variable(name) => variables.get(&name).cloned().unwrap_or_else(|| {
            let var = LVar::new_variable();
            variables.insert(name, LVar::clone(&var));
            var
        }),
        ast::Value::String(value) => LVar::Constant(LValue::String(value.into())),
        ast::Value::Number(value) => LVar::Constant(value.into()),
        ast::Value::None => LVar::Constant(LValue::Null),
    };

    let jump_target = |value| match value {
        ast::Value::Variable(name) => labels
            .get(&name)
            .copied()
            .ok_or_else(|| VMLoadError::BadProcessorCode(format!("label not found: {name}"))),

        ast::Value::Number(address) => {
            let counter = address as usize;
            if (0..num_instructions).contains(&counter) {
                Ok(counter)
            } else {
                Err(VMLoadError::BadProcessorCode(format!(
                    "jump out of range: {}",
                    address.trunc()
                )))
            }
        }

        _ => unreachable!(),
    };

    Ok(match instruction {
        // unprivileged instructions
        ast::Instruction::Noop => Box::new(Noop),
        ast::Instruction::End => Box::new(End),
        ast::Instruction::Stop => Box::new(Stop),
        ast::Instruction::Jump { target, op, x, y } => Box::new(Jump {
            target: jump_target(target)?,
            op,
            x: lvar(x),
            y: lvar(y),
        }),
        ast::Instruction::Set { to, from } => Box::new(Set {
            to: lvar(to),
            from: lvar(from),
        }),
        ast::Instruction::Print { value } => Box::new(Print { value: lvar(value) }),
        // TODO: implement?
        ast::Instruction::Draw { .. } => Box::new(Noop),

        // unknown
        // do this here so it isn't ignored for unprivileged procs
        ast::Instruction::Unknown(name) => {
            return Err(VMLoadError::BadProcessorCode(format!(
                "unknown instruction: {name}"
            )));
        }

        // convert privileged instructions to noops if the proc is unprivileged
        _ if !privileged => Box::new(Noop),

        // privileged instructions
        ast::Instruction::SetRate { value } => Box::new(SetRate { value: lvar(value) }),
    })
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

struct Jump {
    target: usize,
    op: ConditionOp,
    x: LVar,
    y: LVar,
}

impl Jump {
    fn test(op: ConditionOp, x: LValue, y: LValue) -> bool {
        match op {
            ConditionOp::Equal => {
                if x.isobj() && y.isobj() {
                    x == y
                } else {
                    (x.num() - y.num()).abs() < EQUALITY_EPSILON
                }
            }
            ConditionOp::NotEqual => {
                if x.isobj() && y.isobj() {
                    x != y
                } else {
                    (x.num() - y.num()).abs() >= EQUALITY_EPSILON
                }
            }
            ConditionOp::LessThan => x.num() < y.num(),
            ConditionOp::LessThanEq => x.num() <= y.num(),
            ConditionOp::GreaterThan => x.num() > y.num(),
            ConditionOp::GreaterThanEq => x.num() >= y.num(),
            ConditionOp::StrictEqual => x == y,
            ConditionOp::Always => true,
        }
    }
}

impl SimpleInstruction for Jump {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        if Jump::test(self.op, self.x.get(state), self.y.get(state)) {
            // we do the bounds check while parsing
            state.counter = self.target;
        }
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
                Cow::from(if (n - (rounded as f64)).abs() < PRINT_EPSILON {
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
