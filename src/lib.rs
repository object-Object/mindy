#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod parser;
pub mod types;
mod utils;
pub mod vm;
