use mindy::{
    types::content,
    vm::buildings::{
        HYPER_PROCESSOR, LOGIC_PROCESSOR, MEMORY_BANK, MEMORY_CELL, MICRO_PROCESSOR, WORLD_CELL,
        WORLD_PROCESSOR,
    },
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub enum DisplayKind {
    Logic,
    Large,
    Tiled,
}

impl DisplayKind {
    pub fn name(&self) -> &str {
        match self {
            Self::Logic => "logic-display",
            Self::Large => "large-logic-display",
            Self::Tiled => "tile-logic-display",
        }
    }
}

#[wasm_bindgen]
pub fn display_size(kind: DisplayKind) -> i16 {
    content::blocks::FROM_NAME[kind.name()].size
}

#[wasm_bindgen]
pub enum MemoryKind {
    Cell,
    Bank,
    WorldCell,
}

impl MemoryKind {
    pub fn name(&self) -> &str {
        match self {
            Self::Cell => MEMORY_CELL,
            Self::Bank => MEMORY_BANK,
            Self::WorldCell => WORLD_CELL,
        }
    }
}

#[wasm_bindgen]
pub fn memory_size(kind: MemoryKind) -> i16 {
    content::blocks::FROM_NAME[kind.name()].size
}

#[wasm_bindgen]
pub enum ProcessorKind {
    Micro,
    Logic,
    Hyper,
    World,
}

impl ProcessorKind {
    pub fn name(&self) -> &str {
        match self {
            Self::Micro => MICRO_PROCESSOR,
            Self::Logic => LOGIC_PROCESSOR,
            Self::Hyper => HYPER_PROCESSOR,
            Self::World => WORLD_PROCESSOR,
        }
    }
}

#[wasm_bindgen]
pub fn processor_size(kind: ProcessorKind) -> i16 {
    content::blocks::FROM_NAME[kind.name()].size
}
