use std::{fmt::Display, ops::Deref};

use serde::Deserialize;
use widestring::{U16Str, U16String};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub name: MultiString,
    pub id: i32,
    pub logic_id: i32,
    pub size: i32,
    pub legacy: bool,
    pub range: f64,
    pub item_capacity: i32,
    pub liquid_capacity: f32,
    /*
    pub visibility: Visibility,
    pub subclass: MultiString,
    pub configurable: bool,
    pub category: Category,
    pub has_items: bool,
    pub accepts_items: bool,
    pub separate_item_capacity: bool,
    pub no_side_blend: bool,
    pub unloadable: bool,
    pub has_liquids: bool,
    pub outputs_liquid: bool,
    pub has_power: bool,
    pub consumes_power: bool,
    pub outputs_power: bool,
    pub connected_power: bool,
    pub conductive_power: bool,
    pub max_nodes: i32,
    pub output_facing: bool,
    pub rotate: bool,
    pub unit_plans: MultiString,
    */
}

impl PartialEq for Block {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Block {}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub name: MultiString,
    pub id: i32,
    pub logic_id: i32,
}

impl PartialEq for Item {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Item {}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Liquid {
    pub name: MultiString,
    pub id: i32,
    pub logic_id: i32,
}

impl PartialEq for Liquid {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Liquid {}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Unit {
    pub name: MultiString,
    pub id: i32,
    pub logic_id: i32,
}

impl PartialEq for Unit {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Unit {}

#[derive(Debug, Clone, Deserialize)]
#[serde(from = "String")]
pub struct MultiString {
    string: String,
    u16string: U16String,
}

impl MultiString {
    pub fn as_string(&self) -> &String {
        &self.string
    }

    pub fn as_str(&self) -> &str {
        &self.string
    }

    pub fn as_u16string(&self) -> &U16String {
        &self.u16string
    }

    pub fn as_u16str(&self) -> &U16Str {
        &self.u16string
    }
}

impl From<String> for MultiString {
    fn from(string: String) -> Self {
        Self {
            u16string: U16String::from_str(&string),
            string,
        }
    }
}

impl From<MultiString> for String {
    fn from(value: MultiString) -> Self {
        value.string
    }
}

impl From<MultiString> for U16String {
    fn from(value: MultiString) -> Self {
        value.u16string
    }
}

impl Display for MultiString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.string.fmt(f)
    }
}

impl Deref for MultiString {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.string
    }
}

macro_rules! include_content {
    ($typ:ident, $file:expr) => {
        use lazy_static::lazy_static;
        use rapidhash::fast::RapidHashMap;

        use super::$typ;

        lazy_static! {
            pub static ref VALUES: Vec<$typ> = csv::ReaderBuilder::new()
                .delimiter(b';')
                .comment(Some(b'/'))
                .from_reader(&include_bytes!($file)[..])
                .deserialize()
                .map(|v| v.unwrap())
                .collect();
            /// Only includes values that have a valid logic id.
            pub static ref FROM_ID: RapidHashMap<i32, &'static $typ> = VALUES
                .iter()
                .filter(|v| v.logic_id >= 0)
                .map(|v| (v.id, v))
                .collect();
            pub static ref FROM_LOGIC_ID: RapidHashMap<i32, &'static $typ> = VALUES
                .iter()
                .filter(|v| v.logic_id >= 0)
                .map(|v| (v.logic_id, v))
                .collect();
            pub static ref FROM_NAME: RapidHashMap<&'static str, &'static $typ> =
                VALUES.iter().map(|v| (v.name.as_str(), v)).collect();
        }
    };
}

pub mod blocks {
    include_content!(
        Block,
        "../../submodules/mimex-data/data/be/mimex-blocks.txt"
    );

    lazy_static! {
        pub static ref AIR: &'static Block = FROM_NAME["air"];
        pub static ref STONE: &'static Block = FROM_NAME["stone"];
    }
}

pub mod items {
    include_content!(Item, "../../submodules/mimex-data/data/be/mimex-items.txt");
}

pub mod liquids {
    include_content!(
        Liquid,
        "../../submodules/mimex-data/data/be/mimex-liquids.txt"
    );
}

pub mod units {
    include_content!(Unit, "../../submodules/mimex-data/data/be/mimex-units.txt");
}
