use std::{cell::RefCell, collections::HashMap, rc::Rc};

use num_traits::AsPrimitive;
use velcro::map_iter_from;

use super::processor::ProcessorState;

#[allow(clippy::approx_constant)]
const PI: f64 = 3.1415927;
#[allow(clippy::approx_constant)]
const E: f64 = 2.7182818;

#[derive(Debug, Clone, PartialEq)]
pub enum LVar {
    Variable(Rc<RefCell<LValue>>),
    Constant(LValue),
    Counter,
    Ipt,
    Time,
    Tick,
    Second,
    Minute,
}

impl LVar {
    pub fn new_variable() -> Self {
        Self::Variable(Rc::new(RefCell::new(LValue::Null)))
    }

    pub fn init_globals(variables: &mut HashMap<String, LVar>) {
        // https://github.com/Anuken/Mindustry/blob/e95c543fb224b8d8cb21f834e0d02cbdb9f34d48/core/src/mindustry/logic/GlobalVars.java#L41
        variables.extend(map_iter_from! {
            "@counter": Self::Counter,
            "@ipt": Self::Ipt,

            "false": constant(0),
            "true": constant(1),
            "null": constant(LValue::Null),

            "@pi": constant(PI),
            "π": constant(PI),
            "@e": constant(E),
            "@degToRad": constant(PI / 180.),
            "@radToDeg": constant(180. / PI),

            "@time": Self::Time,
            "@tick": Self::Tick,
            "@second": Self::Second,
            "@minute": Self::Minute,
            "@waveNumber": constant(0),
            "@waveTime": constant(0),

            "@server": constant(1),
            "@client": constant(0),
        });
    }

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

    pub fn set(&self, state: &mut ProcessorState, value: LValue) {
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

fn constant<T>(value: T) -> LVar
where
    T: Into<LValue>,
{
    LVar::Constant(value.into())
}

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
