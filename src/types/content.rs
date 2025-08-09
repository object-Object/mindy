use core::{fmt::Display, ops::Deref};

use itertools::Itertools;
use serde::Deserialize;
use widestring::U16Str;

macro_rules! impl_content {
    ($typ:ident) => {
        impl PartialEq for $typ {
            fn eq(&self, other: &Self) -> bool {
                self.id == other.id
            }
        }

        impl Eq for $typ {}
    };
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub name: MultiStr,
    pub id: i32,
    pub logic_id: i32,
    pub size: i16,
    pub legacy: bool,
    pub range: f64,
    pub item_capacity: i32,
    pub liquid_capacity: f32,
    /*
    pub visibility: Visibility,
    pub subclass: MultiStr,
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
    pub unit_plans: MultiStr,
    */
}

impl_content!(Block);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub name: MultiStr,
    pub id: i32,
    pub logic_id: i32,
}

impl_content!(Item);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Liquid {
    pub name: MultiStr,
    pub id: i32,
    pub logic_id: i32,
}

impl_content!(Liquid);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Unit {
    pub name: MultiStr,
    pub id: i32,
    pub logic_id: i32,
}

impl_content!(Unit);

const MULTISTR_LEN: usize = 32;

#[derive(Debug, Clone, Deserialize)]
#[serde(from = "&str")]
pub struct MultiStr {
    string: [u8; MULTISTR_LEN],
    u16string: [u16; MULTISTR_LEN],
    len: u8,
}

impl MultiStr {
    pub const fn new(s: &str, u16s: &[u16]) -> Self {
        assert!(s.len() <= MULTISTR_LEN, "MultiStr arguments are too long");
        assert!(
            s.len() == u16s.len(),
            "MultiStr arguments must be the same length"
        );
        unsafe { Self::new_unchecked(s, u16s) }
    }

    const unsafe fn new_unchecked(s: &str, u16s: &[u16]) -> Self {
        let mut i = 0;
        let mut string = [0; MULTISTR_LEN];
        while i < s.len() {
            string[i] = s.as_bytes()[i];
            i += 1;
        }

        i = 0;
        let mut u16string = [0; MULTISTR_LEN];
        while i < u16s.len() {
            u16string[i] = u16s[i];
            i += 1;
        }

        Self {
            string,
            u16string,
            len: s.len() as u8,
        }
    }

    pub fn as_str(&self) -> &str {
        unsafe { str::from_utf8_unchecked(&self.string[0..(self.len as usize)]) }
    }

    pub fn as_u16str(&self) -> &U16Str {
        U16Str::from_slice(&self.u16string[0..(self.len as usize)])
    }
}

#[macro_export]
macro_rules! multistr {
    ($text:expr) => {
        $crate::types::content::MultiStr::new($text, widestring::u16str!($text).as_slice())
    };
}

impl From<&str> for MultiStr {
    fn from(s: &str) -> Self {
        assert!(
            s.len() <= MULTISTR_LEN,
            "MultiStr too long (want <={MULTISTR_LEN}, got {}): {s}",
            s.len()
        );

        let u16s = s.encode_utf16().collect_vec();
        assert!(
            s.len() == u16s.len(),
            "{s} is {} bytes, but takes {} bytes as UTF-16",
            s.len(),
            u16s.len()
        );

        unsafe { Self::new_unchecked(s, &u16s) }
    }
}

impl Display for MultiStr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.as_str().fmt(f)
    }
}

impl Deref for MultiStr {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

macro_rules! include_content {
    ($typ:ident, $file_std:expr, $file_no_std:expr; $num:expr) => {
        #[cfg(feature = "std")]
        use alloc::vec::Vec;

        use lazy_static::lazy_static;

        use super::$typ;
        use crate::utils::RapidHashMap;

        #[cfg(feature = "std")]
        lazy_static! {
            pub static ref VALUES: Vec<$typ> = csv::ReaderBuilder::new()
                .delimiter(b';')
                .comment(Some(b'/'))
                .from_reader(&include_bytes!($file_std)[..])
                .deserialize()
                .map(|v| v.unwrap())
                .collect();
        }

        #[cfg(all(not(feature = "std"), feature = "no_std"))]
        lazy_static! {
            pub static ref VALUES: [$typ; $num] =
                serde_json_core::from_slice(&include_bytes!($file_no_std)[..])
                    .unwrap()
                    .0;
        }

        lazy_static! {
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
        "../../submodules/mimex-data/data/be/mimex-blocks.txt",
        "content/blocks.json"; 2
    );

    lazy_static! {
        pub static ref AIR: &'static Block = FROM_NAME["air"];
        pub static ref STONE: &'static Block = FROM_NAME["stone"];
    }
}

pub mod items {
    include_content!(
        Item,
        "../../submodules/mimex-data/data/be/mimex-items.txt",
        "content/items.json"; 0
    );
}

pub mod liquids {
    include_content!(
        Liquid,
        "../../submodules/mimex-data/data/be/mimex-liquids.txt",
        "content/liquids.json"; 0
    );
}

pub mod units {
    include_content!(
        Unit,
        "../../submodules/mimex-data/data/be/mimex-units.txt",
        "content/units.json"; 0
    );
}
