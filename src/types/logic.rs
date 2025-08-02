use binrw::{io::NoSeek, prelude::*};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};

use crate::types::JavaString;

#[binrw]
#[brw(big)]
#[br(map_stream = |s| NoSeek::new(ZlibDecoder::new(s)))]
#[bw(map_stream = |s| NoSeek::new(ZlibEncoder::new(s, Compression::default())))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessorConfig {
    #[brw(magic = 1u8)] // config format version
    #[bw(try_calc = code.len().try_into())]
    code_count: i32,
    #[br(count = code_count, try_map = |v: Vec<u8>| v.try_into())]
    #[bw(map = |s| s.clone().into_bytes())]
    pub code: String,

    #[bw(try_calc = links.len().try_into())]
    links_count: i32,
    #[br(count = links_count)]
    pub links: Vec<ProcessorLinkConfig>,
}

impl ProcessorConfig {
    pub fn from_code(code: &str) -> Self {
        Self {
            code: code.to_string(),
            links: Vec::new(),
        }
    }
}

/// A relative processor link.
#[binrw]
#[brw(big)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessorLinkConfig {
    pub name: JavaString,
    pub x: i16,
    pub y: i16,
}

impl ProcessorLinkConfig {
    /// Create a relative link without assigning a custom name.
    pub fn unnamed(x: i16, y: i16) -> Self {
        Self {
            name: "".into(),
            x,
            y,
        }
    }
}
