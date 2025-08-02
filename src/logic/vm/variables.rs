use std::{cell::RefCell, collections::HashMap, rc::Rc};

use num_traits::AsPrimitive;
use velcro::hash_map_from;

use crate::types::{
    Point2, Team, colors,
    content::{self, Block, Item, Liquid, Unit},
};

use super::processor::ProcessorState;

#[allow(clippy::approx_constant)]
pub const PI: f32 = 3.1415927;
#[allow(clippy::approx_constant)]
pub const E: f32 = 2.7182818;

pub const DEG_RAD: f32 = PI / 180.;
pub const RAD_DEG: f32 = 180. / PI;

pub const F64_DEG_RAD: f64 = 0.017453292519943295;
pub const F64_RAD_DEG: f64 = 57.29577951308232;

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

    // https://github.com/Anuken/Mindustry/blob/e95c543fb224b8d8cb21f834e0d02cbdb9f34d48/core/src/mindustry/logic/GlobalVars.java#L41
    pub fn create_globals() -> HashMap<String, LVar> {
        let mut globals = hash_map_from! {
            "@ipt": Self::Ipt,

            "false": constant(0),
            "true": constant(1),
            "null": constant(LValue::Null),

            "@pi": constant(PI),
            "π": constant(PI),
            "@e": constant(E),
            "@degToRad": constant(DEG_RAD),
            "@radToDeg": constant(RAD_DEG),

            "@time": Self::Time,
            "@tick": Self::Tick,
            "@second": Self::Second,
            "@minute": Self::Minute,
            "@waveNumber": constant(0),
            "@waveTime": constant(0),

            "@server": constant(1),
            "@client": constant(0),

            "@blockCount": constant(content::blocks::FROM_LOGIC_ID.len()),
            "@itemCount": constant(content::items::FROM_LOGIC_ID.len()),
            "@liquidCount": constant(content::liquids::FROM_LOGIC_ID.len()),
            "@unitCount": constant(content::units::FROM_LOGIC_ID.len()),
        };

        globals.extend(Team::base_teams().map(|t| (format!("@{}", t.name()), constant(t))));

        globals.extend(
            content::items::VALUES
                .iter()
                .map(|v| (format!("@{}", v.name), constant(Content::Item(v)))),
        );

        globals.extend(
            content::liquids::VALUES
                .iter()
                .map(|v| (format!("@{}", v.name), constant(Content::Liquid(v)))),
        );

        globals.extend(
            content::blocks::VALUES
                .iter()
                .filter(|v| !content::items::FROM_NAME.contains_key(v.name.as_str()) && !v.legacy)
                .map(|v| (format!("@{}", v.name), constant(Content::Block(v)))),
        );

        globals.extend(
            colors::COLORS
                .iter()
                .filter(|(k, _)| matches!(k.chars().next(), Some(c) if c.is_lowercase()))
                .map(|(k, v)| {
                    let mut name = "@color".to_string();
                    name.push(k.chars().next().unwrap().to_ascii_uppercase());
                    name.extend(k.chars().skip(1));
                    (name, constant(*v))
                }),
        );

        // skip adding weathers and alignments since they aren't useful in a headless environment

        globals.extend(
            content::units::VALUES
                .iter()
                .map(|v| (format!("@{}", v.name), constant(Content::Unit(v)))),
        );

        globals
    }

    pub fn create_locals() -> HashMap<String, LVar> {
        hash_map_from! {
            // we want other processors to be able to write @counter as if it's a local
            "@counter": Self::Counter,
        }
    }

    pub fn get(&self, state: &ProcessorState) -> LValue {
        match self {
            Self::Variable(_) | Self::Constant(_) => self.try_get().unwrap(),
            Self::Counter => state.counter.into(),
            Self::Ipt => state.ipt.into(),
            Self::Time => state.time.get().into(),
            Self::Tick => state.tick().into(),
            Self::Second => (state.tick() / 60.).into(),
            Self::Minute => (state.tick() / 60. / 60.).into(),
        }
    }

    /// Returns `None` if this is a variable that requires access to a specific processor's state.
    pub fn try_get(&self) -> Option<LValue> {
        match self {
            Self::Variable(ptr) => Some(LValue::clone(&ptr.borrow())),
            Self::Constant(value) => Some(value.to_owned()),
            _ => None,
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

    pub fn set_from(&self, state: &mut ProcessorState, other: &LVar) {
        self.set(state, other.get(state));
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
    Content(Content),
    Team(Team),
    Building {
        block: &'static Block,
        position: Point2,
    },
}

impl LValue {
    pub fn num(&self) -> f64 {
        match *self {
            Self::Number(n) if !invalid(n) => n,
            Self::Null => 0.,
            _ => 1.,
        }
    }

    pub fn numf(&self) -> f32 {
        self.num() as f32
    }

    pub fn isobj(&self) -> bool {
        !matches!(self, Self::Number(_))
    }
}

impl<T: AsPrimitive<f64> + Numeric> From<T> for LValue {
    fn from(value: T) -> Self {
        let value = value.as_();
        if invalid(value) {
            Self::Null
        } else {
            Self::Number(value)
        }
    }
}

impl From<bool> for LValue {
    fn from(value: bool) -> Self {
        Self::Number(if value { 1. } else { 0. })
    }
}

impl From<Rc<str>> for LValue {
    fn from(value: Rc<str>) -> Self {
        Self::String(value)
    }
}

impl From<String> for LValue {
    fn from(value: String) -> Self {
        Self::String(value.into())
    }
}

impl From<&str> for LValue {
    fn from(value: &str) -> Self {
        Self::String(value.into())
    }
}

impl From<Content> for LValue {
    fn from(value: Content) -> Self {
        Self::Content(value)
    }
}

impl From<Team> for LValue {
    fn from(value: Team) -> Self {
        Self::Team(value)
    }
}

impl<T> From<Option<T>> for LValue
where
    LValue: From<T>,
{
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => value.into(),
            None => Self::Null,
        }
    }
}

fn invalid(n: f64) -> bool {
    n.is_nan() || n.is_infinite()
}

#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Block(&'static Block),
    Item(&'static Item),
    Liquid(&'static Liquid),
    Unit(&'static Unit),
}

impl Content {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Block(Block { name, .. }) => name,
            Self::Item(Item { name, .. }) => name,
            Self::Liquid(Liquid { name, .. }) => name,
            Self::Unit(Unit { name, .. }) => name,
        }
    }

    pub fn logic_id(&self) -> i32 {
        match self {
            Self::Block(Block { logic_id, .. }) => *logic_id,
            Self::Item(Item { logic_id, .. }) => *logic_id,
            Self::Liquid(Liquid { logic_id, .. }) => *logic_id,
            Self::Unit(Unit { logic_id, .. }) => *logic_id,
        }
    }
}

// https://stackoverflow.com/a/66537661
trait Numeric {}
impl Numeric for u8 {}
impl Numeric for i8 {}
impl Numeric for u16 {}
impl Numeric for i16 {}
impl Numeric for u32 {}
impl Numeric for i32 {}
impl Numeric for u64 {}
impl Numeric for i64 {}
impl Numeric for u128 {}
impl Numeric for i128 {}
impl Numeric for usize {}
impl Numeric for isize {}
impl Numeric for f32 {}
impl Numeric for f64 {}
impl Numeric for char {}
