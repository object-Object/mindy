use std::hash::Hash;

use binrw::prelude::*;
use itertools::Itertools;

use crate::types::{
    ContentType, JavaString, LAccess, PackedPoint2, Point2, Team, UnitCommand, Vec2,
};

#[binrw]
#[brw(big)]
#[derive(Debug, Clone, PartialEq)]
pub enum Object {
    #[brw(magic = 0i8)]
    Null,

    #[brw(magic = 1i8)]
    Int(i32),

    #[brw(magic = 2i8)]
    Long(i64),

    #[brw(magic = 3i8)]
    Float(f32),

    #[brw(magic = 4i8)]
    String(
        #[br(parse_with = parse_string)]
        #[bw(write_with = write_string)]
        Option<JavaString>,
    ),

    #[brw(magic = 5i8)]
    Content(ContentID),

    #[brw(magic = 6i8)]
    IntSeq {
        #[bw(try_calc = values.len().try_into())]
        count: i16,
        #[br(count = count)]
        values: Vec<i32>,
    },

    #[brw(magic = 7i8)]
    Point2(Point2),

    #[brw(magic = 8i8)]
    Point2Array {
        #[bw(try_calc = values.len().try_into())]
        count: i8,
        #[br(count = count)]
        values: Vec<PackedPoint2>,
    },

    #[brw(magic = 9i8)]
    TechNode(ContentID),

    #[brw(magic = 10i8)]
    Bool(
        #[br(map = |v: i8| v != 0)]
        #[bw(map = |&v| v as i8)]
        bool,
    ),

    #[brw(magic = 11i8)]
    Double(f64),

    #[brw(magic = 12i8)]
    Building(i32),

    #[brw(magic = 13i8)]
    LAccess(LAccess),

    #[brw(magic = 14i8)]
    ByteArray {
        #[bw(try_calc = values.len().try_into())]
        count: i32,
        #[br(count = count)]
        values: Vec<u8>,
    },

    #[deprecated]
    #[brw(magic = 15i8)]
    LegacyUnitCommand { value: i8 },

    #[brw(magic = 16i8)]
    BoolArray {
        #[bw(try_calc = values.len().try_into())]
        count: i32,
        // TODO: this seems inefficent
        #[br(count = count, map = |v: Vec<i8>| v.iter().map(|&b| b != 0).collect())]
        #[bw(map = |v| v.iter().map(|&b| b as i8).collect_vec())]
        values: Vec<bool>,
    },

    #[brw(magic = 17i8)]
    Unit { id: i32 },

    #[brw(magic = 18i8)]
    Vec2Array {
        #[bw(try_calc = values.len().try_into())]
        count: i16,
        #[br(count = count)]
        values: Vec<Vec2>,
    },

    #[brw(magic = 19i8)]
    Vec2(Vec2),

    #[brw(magic = 20i8)]
    Team(Team),

    #[brw(magic = 21i8)]
    IntArray {
        #[bw(try_calc = values.len().try_into())]
        count: i16,
        #[br(count = count)]
        values: Vec<i32>,
    },

    #[brw(magic = 22i8)]
    ObjectArray {
        #[bw(try_calc = values.len().try_into())]
        count: i32,
        #[br(count = count)]
        values: Vec<Object>,
    },

    #[brw(magic = 23i8)]
    UnitCommand(UnitCommand),
}

macro_rules! impl_object_from {
    ($arg:tt : $from:path, $obj:ident $body:tt) => {
        impl From<$from> for Object {
            fn from($arg: $from) -> Self {
                Self::$obj$body
            }
        }
    };
    ($obj:ident) => {
        impl_object_from! { $obj, $obj }
    };
    (Vec<$t:path>, $obj:ident) => {
        impl_object_from! { values: Vec<$t>, $obj { values } }
    };
    ($from:path, $obj:ident) => {
        impl_object_from! { value: $from, $obj(value) }
    };
}

impl_object_from! { i32, Int }
impl_object_from! { i64, Long }
impl_object_from! { f32, Float }
impl_object_from! { Option<JavaString>, String }
impl_object_from! { value: Option<String>, String(value.map(|s| s.into())) }
impl_object_from! { value: JavaString, String(Some(value)) }
impl_object_from! { value: String, String(Some(value.into())) }
impl_object_from! { ContentID, Content }
impl_object_from! { Point2 }
impl_object_from! { Vec<PackedPoint2>, Point2Array }
impl_object_from! { bool, Bool }
impl_object_from! { f64, Double }
impl_object_from! { LAccess }
impl_object_from! { Vec<u8>, ByteArray }
impl_object_from! { Vec<bool>, BoolArray }
impl_object_from! { Vec<Vec2>, Vec2Array }
impl_object_from! { Vec2 }
impl_object_from! { Team }
impl_object_from! { Vec<Object>, ObjectArray }
impl_object_from! { UnitCommand }

#[binrw::parser(reader, endian)]
fn parse_string() -> BinResult<Option<JavaString>> {
    match reader.read_type::<i8>(endian)? {
        0 => Ok(None),
        _ => reader.read_type(endian).map(Some),
    }
}

#[binrw::writer(writer, endian)]
fn write_string(value: &Option<JavaString>) -> BinResult<()> {
    writer.write_type(&(value.is_some() as i8), endian)?;
    match value {
        Some(s) => writer.write_type(s, endian),
        None => Ok(()),
    }
}

#[binrw]
#[brw(big)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentID {
    pub type_: ContentType,
    pub id: i16,
}
