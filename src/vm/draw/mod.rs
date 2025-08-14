use bitflags::bitflags;
use widestring::U16String;

use super::Content;

#[cfg(feature = "embedded_graphics")]
pub mod embedded;

// note: this allows larger values than mindustry does
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrawCommand {
    Clear {
        r: u8,
        g: u8,
        b: u8,
    },
    Color {
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    },
    Stroke {
        width: i16,
    },
    Line {
        x1: i16,
        y1: i16,
        x2: i16,
        y2: i16,
    },
    Rect {
        x: i16,
        y: i16,
        width: i16,
        height: i16,
        fill: bool,
    },
    Poly {
        x: i16,
        y: i16,
        sides: i16,
        radius: i16,
        rotation: i16,
        fill: bool,
    },
    Triangle {
        x1: i16,
        y1: i16,
        x2: i16,
        y2: i16,
        x3: i16,
        y3: i16,
    },
    Image {
        x: i16,
        y: i16,
        image: Option<Content>,
        size: i16,
        rotation: i16,
    },
    Print {
        x: i16,
        y: i16,
        alignment: TextAlignment,
        text: U16String,
    },
    Translate {
        x: i16,
        y: i16,
    },
    Scale {
        x: i16,
        y: i16,
    },
    Rotate {
        degrees: i16,
    },
    Reset,
}

impl DrawCommand {
    pub const SCALE_STEP: f32 = 0.05;
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct TextAlignment: u8 {
        const CENTER = 1 << 0;
        const TOP    = 1 << 1;
        const BOTTOM = 1 << 2;
        const LEFT   = 1 << 3;
        const RIGHT  = 1 << 4;

        const TOP_LEFT = Self::TOP.bits() | Self::LEFT.bits();
        const TOP_RIGHT = Self::TOP.bits() | Self::RIGHT.bits();
        const BOTTOM_LEFT = Self::BOTTOM.bits() | Self::LEFT.bits();
        const BOTTOM_RIGHT = Self::BOTTOM.bits() | Self::RIGHT.bits();
    }
}
