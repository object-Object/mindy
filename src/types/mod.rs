#![allow(deprecated)]

pub mod colors;
pub mod content;
#[cfg(feature = "std")]
pub mod schematics;

mod enums;
mod java;
mod logic;
mod math;
mod type_io;

pub use enums::*;
pub use java::*;
pub use logic::*;
pub use math::*;
pub use type_io::*;
