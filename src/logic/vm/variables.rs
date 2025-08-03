use std::{cell::RefCell, collections::HashMap, fmt::Display, num::TryFromIntError, rc::Rc};

use num_traits::AsPrimitive;
use strum::VariantArray;
use thiserror::Error;
use velcro::{hash_map_from, map_iter_from};

use crate::types::{
    ContentID, ContentType, LAccess, Point2, Team, colors,
    content::{self, Block, Item, Liquid, Unit},
};

use super::processor::{ProcessorLink, ProcessorState};

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

        globals.extend(Team::base_teams().map(|t| named_constant(t, t)));

        globals.extend(
            content::items::VALUES
                .iter()
                .map(|v| named_constant(&v.name, Content::Item(v))),
        );

        globals.extend(
            content::liquids::VALUES
                .iter()
                .map(|v| named_constant(&v.name, Content::Liquid(v))),
        );

        globals.extend(
            content::blocks::VALUES
                .iter()
                .filter(|v| !content::items::FROM_NAME.contains_key(v.name.as_str()) && !v.legacy)
                .map(|v| named_constant(&v.name, Content::Block(v))),
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

        globals.extend(LAccess::VARIANTS.iter().map(|&v| named_constant(v, v)));

        globals.extend(
            content::units::VALUES
                .iter()
                .map(|v| named_constant(&v.name, Content::Unit(v))),
        );

        globals
    }

    pub fn late_init_locals(
        variables: &mut HashMap<String, LVar>,
        position: Point2,
        links: &[ProcessorLink],
    ) {
        variables.extend(map_iter_from! {
            // we want other processors to be able to write @counter as if it's a local
            "@counter": Self::Counter,

            "@this": constant(LValue::Building(position)),
            "@thisx": constant(position.x),
            "@thisy": constant(position.y),
            "@links": constant(links.len()),
        });

        // if multiple links have the same name, the last one wins
        for link in links {
            variables.insert(link.name.clone(), constant(LValue::Building(link.position)));
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

fn named_constant<K, V>(name: K, value: V) -> (String, LVar)
where
    K: Display,
    V: Into<LValue>,
{
    (format!("@{name}"), constant(value))
}

#[derive(Debug, Clone, PartialEq)]
pub enum LValue {
    Null,
    Number(f64),
    RcString(Rc<str>),
    StaticString(&'static str),
    Content(Content),
    Team(Team),
    Building(Point2),
    Sensor(LAccess),
}

impl LValue {
    pub fn num(&self) -> f64 {
        match *self {
            Self::Number(n) if !invalid(n) => n,
            Self::Null => 0.,
            _ => 1.,
        }
    }

    pub fn numi(&self) -> i32 {
        self.num() as i32
    }

    pub fn num_usize(&self) -> Result<usize, TryFromIntError> {
        self.numi().try_into()
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
        Self::RcString(value)
    }
}

impl From<&'static str> for LValue {
    fn from(value: &'static str) -> Self {
        Self::StaticString(value)
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

impl From<Point2> for LValue {
    fn from(value: Point2) -> Self {
        Self::Building(value)
    }
}

impl From<LAccess> for LValue {
    fn from(value: LAccess) -> Self {
        Self::Sensor(value)
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

impl TryFrom<ContentID> for Content {
    type Error = ContentIDLookupError;

    fn try_from(ContentID { type_, id }: ContentID) -> Result<Self, Self::Error> {
        let id = id as i32;
        let unknown_id_err = ContentIDLookupError::UnknownID(id);
        match type_ {
            ContentType::Block => content::blocks::FROM_ID
                .get(&id)
                .map(|&v| Self::Block(v))
                .ok_or(unknown_id_err),
            ContentType::Item => content::items::FROM_ID
                .get(&id)
                .map(|&v| Self::Item(v))
                .ok_or(unknown_id_err),
            ContentType::Liquid => content::liquids::FROM_ID
                .get(&id)
                .map(|&v| Self::Liquid(v))
                .ok_or(unknown_id_err),
            ContentType::Unit => content::units::FROM_ID
                .get(&id)
                .map(|&v| Self::Unit(v))
                .ok_or(unknown_id_err),
            _ => Err(ContentIDLookupError::UnsupportedType(type_)),
        }
    }
}

#[derive(Debug, Clone, Copy, Error)]
pub enum ContentIDLookupError {
    #[error("unsupported content type: {0:?}")]
    UnsupportedType(ContentType),
    #[error("id not found: {0}")]
    UnknownID(i32),
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
