use core::{fmt::Display, hash::Hash, num::TryFromIntError};

use binrw::prelude::*;

#[binrw]
#[brw(big)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Point2 {
    pub x: i32,
    pub y: i32,
}

impl Point2 {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

impl From<PackedPoint2> for Point2 {
    fn from(value: PackedPoint2) -> Self {
        Self {
            x: value.x as i32,
            y: value.y as i32,
        }
    }
}

impl From<(i32, i32)> for Point2 {
    fn from((x, y): (i32, i32)) -> Self {
        Self { x, y }
    }
}

impl Display for Point2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

#[binrw]
#[brw(big)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PackedPoint2 {
    pub x: i16,
    pub y: i16,
}

impl PackedPoint2 {
    pub fn new(x: i16, y: i16) -> Self {
        Self { x, y }
    }
}

impl TryFrom<Point2> for PackedPoint2 {
    type Error = TryFromIntError;

    fn try_from(value: Point2) -> Result<Self, Self::Error> {
        Ok(Self {
            x: value.x.try_into()?,
            y: value.y.try_into()?,
        })
    }
}

impl From<(i16, i16)> for PackedPoint2 {
    fn from((x, y): (i16, i16)) -> Self {
        Self { x, y }
    }
}

impl Display for PackedPoint2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

#[binrw]
#[brw(big)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec2 {
    x: f32,
    y: f32,
}
