use std::{
    borrow::Cow,
    fmt::Display,
    hash::{Hash, Hasher},
    num::TryFromIntError,
    ops::Deref,
    rc::Rc,
};

use num_traits::AsPrimitive;
use strum::VariantArray;
use thiserror::Error;
use velcro::map_iter_from;
use widestring::{U16Str, U16String};

use crate::{
    types::{
        ContentID, ContentType, LAccess, Team, colors,
        content::{self, Block, Item, Liquid, Unit},
    },
    utils::{RapidIndexMap, u16format},
};

use super::{
    Building,
    processor::{ProcessorLink, ProcessorState},
};

#[allow(clippy::approx_constant)]
pub(super) const PI: f32 = 3.1415927;
#[allow(clippy::approx_constant)]
pub(super) const E: f32 = 2.7182818;

pub(super) const DEG_RAD: f32 = PI / 180.;
pub(super) const RAD_DEG: f32 = 180. / PI;

pub(super) const F64_DEG_RAD: f64 = 0.017453292519943295;
pub(super) const F64_RAD_DEG: f64 = 57.29577951308232;

pub(super) type Constants = RapidIndexMap<U16String, LVar>;
pub(super) type Variables = RapidIndexMap<U16String, LValue>;

#[derive(Debug, Clone, PartialEq)]
pub enum LVar {
    Variable(usize),
    Constant(LValue),
    Counter,
    Ipt,
    Time,
    Tick,
    Second,
    Minute,
}

impl LVar {
    // https://github.com/Anuken/Mindustry/blob/e95c543fb224b8d8cb21f834e0d02cbdb9f34d48/core/src/mindustry/logic/GlobalVars.java#L41
    pub fn create_global_constants() -> Constants {
        let mut globals: Constants = map_iter_from! {
            "@counter": Self::Counter,
            "@ipt": Self::Ipt,

            "false": constant(0),
            "true": constant(1),
            "null": constant(LObject::Null),

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
        }
        .collect();

        globals.extend(
            Team::BASE_TEAMS
                .iter()
                .map(|&t| named_constant(t.name(), t)),
        );

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
                    let mut name = U16String::from("@color");
                    name.push_char(k.chars().next().unwrap().to_ascii_uppercase());
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

    pub(super) fn create_local_constants(
        locals: &mut Constants,
        building: &Building,
        links: &[ProcessorLink],
    ) {
        locals.extend(map_iter_from! {
            "@this": constant(LObject::Building(building.clone())),
            "@thisx": constant(building.position.x),
            "@thisy": constant(building.position.y),
            "@links": constant(links.len()),
        });

        // if multiple links have the same name, the last one wins
        for link in links {
            locals.insert(
                U16String::from_str(&link.name),
                constant(LObject::Building(link.building.clone())),
            );
        }
    }

    #[inline(always)]
    pub fn get<'a>(&'a self, state: &'a ProcessorState) -> Cow<'a, LValue> {
        self.get_inner(state, &state.variables)
    }

    /// Same as get, but state.variables is explicitly passed separately to help with lifetime issues.
    #[inline(always)]
    pub fn get_inner<'a>(
        &'a self,
        state: &ProcessorState,
        variables: &'a Variables,
    ) -> Cow<'a, LValue> {
        match self {
            Self::Variable(i) => Cow::Borrowed(&variables[*i]),
            Self::Constant(value) => Cow::Borrowed(value),
            Self::Counter => Cow::Owned(state.counter.into()),
            Self::Ipt => Cow::Owned(state.ipt.into()),
            Self::Time => Cow::Owned(state.time.get().into()),
            Self::Tick => Cow::Owned(state.tick().into()),
            Self::Second => Cow::Owned((state.tick() / 60.).into()),
            Self::Minute => Cow::Owned((state.tick() / 60. / 60.).into()),
        }
    }

    #[inline(always)]
    pub fn set(&self, state: &mut ProcessorState, value: LValue) {
        match self {
            Self::Variable(i) => {
                state.variables[*i] = value;
            }
            Self::Counter => {
                ProcessorState::try_set_counter(&mut state.counter, &value);
            }
            _ => {}
        }
    }

    #[inline(always)]
    pub fn set_from(&self, state: &mut ProcessorState, other: &LVar) {
        let value = other.get_inner(state, &state.variables);
        if *self != LVar::Counter {
            self.set(state, value.into_owned());
        } else {
            ProcessorState::try_set_counter(&mut state.counter, &value);
        }
    }
}

fn constant<T>(value: T) -> LVar
where
    T: Into<LValue>,
{
    LVar::Constant(value.into())
}

fn named_constant<K, V>(name: K, value: V) -> (U16String, LVar)
where
    K: Display,
    V: Into<LValue>,
{
    (u16format!("@{name}"), constant(value))
}

#[derive(Debug, Clone, PartialEq)]
pub struct LValue {
    numval: f64,
    objval: Option<LObject>,
}

impl LValue {
    pub const NULL: Self = Self {
        numval: 0.,
        objval: Some(LObject::Null),
    };

    #[inline(always)]
    fn non_null(value: LObject) -> Self {
        Self {
            numval: 1.,
            objval: value.into(),
        }
    }

    #[inline(always)]
    pub fn num(&self) -> f64 {
        self.numval
    }

    pub fn try_num(&self) -> Option<f64> {
        if self.isnum() {
            Some(self.numval)
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn numi(&self) -> i32 {
        self.num() as i32
    }

    #[inline(always)]
    pub fn numu(&self) -> u32 {
        self.num() as u32
    }

    #[inline(always)]
    pub fn num_usize(&self) -> Result<usize, TryFromIntError> {
        self.numi().try_into()
    }

    #[inline(always)]
    pub fn numf(&self) -> f32 {
        self.num() as f32
    }

    #[inline(always)]
    pub fn obj(&self) -> &Option<LObject> {
        &self.objval
    }

    #[inline(always)]
    pub fn isnum(&self) -> bool {
        self.objval.is_none()
    }

    #[inline(always)]
    pub fn isobj(&self) -> bool {
        self.objval.is_some()
    }
}

impl Default for LValue {
    fn default() -> Self {
        Self::NULL
    }
}

impl<T> From<T> for LValue
where
    T: AsPrimitive<f64> + Numeric,
{
    fn from(value: T) -> Self {
        let value = value.as_();
        if value.is_nan() || value.is_infinite() {
            Self::NULL
        } else {
            Self {
                numval: value,
                objval: None,
            }
        }
    }
}

impl From<bool> for LValue {
    fn from(value: bool) -> Self {
        (if value { 1. } else { 0. }).into()
    }
}

impl From<LObject> for LValue {
    fn from(value: LObject) -> Self {
        if value == LObject::Null {
            Self::NULL
        } else {
            Self::non_null(value)
        }
    }
}

impl From<LString> for LValue {
    fn from(value: LString) -> Self {
        Self::non_null(LObject::String(value))
    }
}

impl From<Rc<U16Str>> for LValue {
    fn from(value: Rc<U16Str>) -> Self {
        LString::Rc(value).into()
    }
}

impl From<&'static U16Str> for LValue {
    fn from(value: &'static U16Str) -> Self {
        LString::Static(value).into()
    }
}

impl From<String> for LValue {
    fn from(value: String) -> Self {
        LString::rc(U16String::from_str(&value).as_ustr()).into()
    }
}

impl From<Content> for LValue {
    fn from(value: Content) -> Self {
        Self::non_null(LObject::Content(value))
    }
}

impl From<Team> for LValue {
    fn from(value: Team) -> Self {
        Self::non_null(LObject::Team(value))
    }
}

impl From<Building> for LValue {
    fn from(value: Building) -> Self {
        Self::non_null(LObject::Building(value))
    }
}

impl From<LAccess> for LValue {
    fn from(value: LAccess) -> Self {
        Self::non_null(LObject::Sensor(value))
    }
}

impl<T> From<Option<T>> for LValue
where
    LValue: From<T>,
{
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => value.into(),
            None => Self::NULL,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LObject {
    Null,
    String(LString),
    Content(Content),
    Team(Team),
    Building(Building),
    Sensor(LAccess),
}

impl Default for LObject {
    fn default() -> Self {
        Self::Null
    }
}

#[derive(Debug, Clone)]
pub enum LString {
    Rc(Rc<U16Str>),
    Static(&'static U16Str),
}

impl LString {
    pub fn rc(value: &U16Str) -> Self {
        // see implementation of From<&str> for Rc<str>
        let rc = Rc::<[u16]>::from(value.as_slice());
        // SAFETY: U16Str is just a wrapper around [u16]
        let rc = unsafe { Rc::from_raw(Rc::into_raw(rc) as *const U16Str) };
        Self::Rc(rc)
    }
}

impl Deref for LString {
    type Target = U16Str;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Rc(rc) => rc,
            Self::Static(s) => s,
        }
    }
}

impl PartialEq for LString {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl Eq for LString {}

impl PartialOrd for LString {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LString {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (**self).cmp(&**other)
    }
}

impl Hash for LString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Block(&'static Block),
    Item(&'static Item),
    Liquid(&'static Liquid),
    Unit(&'static Unit),
}

impl Content {
    pub fn name(&self) -> &'static U16Str {
        match self {
            Self::Block(Block { name, .. }) => name.as_u16str(),
            Self::Item(Item { name, .. }) => name.as_u16str(),
            Self::Liquid(Liquid { name, .. }) => name.as_u16str(),
            Self::Unit(Unit { name, .. }) => name.as_u16str(),
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
