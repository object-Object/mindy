use std::{borrow::Cow, cell::RefCell, collections::HashMap, rc::Rc};

use super::{
    LogicVM, VMLoadError, VMLoadResult,
    processor::ProcessorState,
    variables::{LValue, LVar, LVarPtr},
};
use crate::logic::ast;

const MAX_TEXT_BUFFER: usize = 400;

pub fn parse_instruction(
    name: &str,
    mut args: Vec<ast::Value>,
    variables: &mut HashMap<String, LVarPtr>,
    labels: &HashMap<String, usize>,
) -> VMLoadResult<Box<dyn Instruction>> {
    Ok(match name {
        "noop" => Box::new(Noop),
        "stop" => Box::new(Stop),
        "print" => Box::new(Print {
            value: lvar(name, &mut args, variables, 0)?,
        }),
        _ => panic!(),
    })
}

/// Must be called in reverse order.
fn lvar(
    name: &str,
    args: &mut Vec<ast::Value>,
    variables: &mut HashMap<String, LVarPtr>,
    idx: usize,
) -> VMLoadResult<LVar> {
    if idx >= args.len() {
        return Err(VMLoadError::BadProcessorCode(format!(
            "{name}: missing argument {idx}"
        )));
    }

    Ok(match args.swap_remove(idx) {
        ast::Value::Variable(name) => {
            LVar::Variable(variables.get(&name).map(Rc::clone).unwrap_or_else(|| {
                let ptr = Rc::new(RefCell::new(LValue::Null));
                variables.insert(name, Rc::clone(&ptr));
                ptr
            }))
        }
        ast::Value::String(value) => LVar::Constant(LValue::String(value.into())),
        ast::Value::Number(value) => LVar::Constant(value.into()),
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

// instruction impls

struct Noop;

impl SimpleInstruction for Noop {
    fn execute(&self, _: &mut ProcessorState, _: &LogicVM) {}
}

struct Stop;

impl Instruction for Stop {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) -> InstructionResult {
        state.counter -= 1;
        state.set_stopped(true);
        InstructionResult::Yield
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
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) {
        if state.printbuffer.len() < MAX_TEXT_BUFFER {
            let value = self.value.get(state);
            state.printbuffer.push_str(&Print::to_string(&value))
        }
    }
}
