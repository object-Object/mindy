use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub name: String,
    pub id: i32,
    pub logic_id: i32,
    pub size: i32,
    pub legacy: bool,
    pub range: f64,
    pub item_capacity: i32,
    pub liquid_capacity: f32,
    /*
    pub visibility: Visibility,
    pub subclass: String,
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
    pub unit_plans: String,
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
    pub name: String,
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
    pub name: String,
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
    pub name: String,
    pub id: i32,
    pub logic_id: i32,
}

impl PartialEq for Unit {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Unit {}

macro_rules! include_content {
    ($typ:ident, $file:expr) => {
        use std::collections::HashMap;

        use lazy_static::lazy_static;

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
            pub static ref FROM_ID: HashMap<i32, &'static $typ> = VALUES
                .iter()
                .filter(|v| v.logic_id >= 0)
                .map(|v| (v.id, v))
                .collect();
            pub static ref FROM_LOGIC_ID: HashMap<i32, &'static $typ> = VALUES
                .iter()
                .filter(|v| v.logic_id >= 0)
                .map(|v| (v.logic_id, v))
                .collect();
            pub static ref FROM_NAME: HashMap<&'static str, &'static $typ> =
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
