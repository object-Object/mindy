use std::{
    borrow::Borrow,
    fmt,
    hash::{Hash, Hasher},
    ops::{Deref, DerefMut},
};

use binrw::prelude::*;

#[binrw]
#[brw(big)]
#[derive(Debug, Clone, Default)]
pub struct JavaString {
    /// The length of the string.
    #[bw(try_calc = u16::try_from(value.len()))]
    count: u16,

    /// The string value.
    #[br(
        count = count,
        try_map = |s: Vec<u8>| cesu8::from_java_cesu8(&s).map(|v| v.to_string()),
    )]
    #[bw(map = |s| cesu8::to_java_cesu8(s).to_vec())]
    pub value: String,
}

impl From<&str> for JavaString {
    fn from(value: &str) -> Self {
        Self {
            value: value.to_string(),
        }
    }
}

impl From<String> for JavaString {
    fn from(value: String) -> Self {
        Self { value }
    }
}

impl From<JavaString> for String {
    fn from(value: JavaString) -> Self {
        value.value
    }
}

impl Deref for JavaString {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl DerefMut for JavaString {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl PartialEq for JavaString {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl Eq for JavaString {}

impl Hash for JavaString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state)
    }
}

impl Borrow<str> for JavaString {
    fn borrow(&self) -> &str {
        &self.value
    }
}

impl Borrow<String> for JavaString {
    fn borrow(&self) -> &String {
        &self.value
    }
}

impl fmt::Display for JavaString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}
