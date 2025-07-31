use std::{cell::RefCell, rc::Rc};

use num_traits::AsPrimitive;

use super::processor::ProcessorState;

#[derive(Debug, Clone)]
pub enum LVar {
    Variable(LVarPtr),
    Constant(LValue),
    Counter,
    Ipt,
    Time,
    Tick,
    Second,
    Minute,
}

impl LVar {
    pub fn get(&self, state: &ProcessorState) -> LValue {
        match self {
            Self::Variable(ptr) => LValue::clone(&ptr.borrow()),
            Self::Constant(value) => value.to_owned(),
            Self::Counter => state.counter.into(),
            Self::Ipt => state.ipt.into(),
            Self::Time => state.time.get().into(),
            Self::Tick => state.tick().into(),
            Self::Second => (state.tick() / 60.).into(),
            Self::Minute => (state.tick() / 60. / 60.).into(),
        }
    }

    pub fn set(&mut self, state: &mut ProcessorState, value: LValue) {
        match self {
            Self::Variable(ptr) => {
                *ptr.borrow_mut() = value;
            }
            Self::Counter => {
                if let LValue::Number(n) = value {
                    let counter = n as usize;
                    state.counter = if (0..state.num_instructions).contains(&counter) {
                        counter
                    } else {
                        0
                    };
                }
            }
            _ => {}
        }
    }
}

pub type LVarPtr = Rc<RefCell<LValue>>;

#[derive(Debug, Clone, PartialEq)]
pub enum LValue {
    Null,
    Number(f64),
    String(Rc<str>),
}

impl LValue {
    pub fn num(&self) -> f64 {
        match *self {
            Self::Number(n) if !invalid(n) => n,
            _ => 0.,
        }
    }
}

impl<T: AsPrimitive<f64>> From<T> for LValue {
    fn from(value: T) -> Self {
        Self::Number(value.as_())
    }
}

fn invalid(n: f64) -> bool {
    n.is_nan() || n.is_infinite()
}
