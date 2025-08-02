use std::rc::Rc;

use strum_macros::IntoStaticStr;

use super::{
    LogicVM, VMLoadError, VMLoadResult,
    processor::{Processor, ProcessorBuilder, ProcessorState},
};
use crate::types::{Object, Point2, ProcessorConfig, SchematicTile, content};

pub const MICRO_PROCESSOR: &str = "micro-processor";
pub const LOGIC_PROCESSOR: &str = "logic-processor";
pub const HYPER_PROCESSOR: &str = "hyper-processor";
pub const WORLD_PROCESSOR: &str = "world-processor";

pub const MEMORY_CELL: &str = "memory-cell";
pub const MEMORY_BANK: &str = "memory-bank";
pub const WORLD_CELL: &str = "world-cell";

pub const MESSAGE: &str = "message";
pub const WORLD_MESSAGE: &str = "world-message";

pub const SWITCH: &str = "switch";
pub const WORLD_SWITCH: &str = "world-switch";

const MESSAGE_MAX_LEN: usize = 220;
const MESSAGE_MAX_LINES: usize = 24;

pub struct Block {
    pub name: String,
    pub position: Point2,
    pub content: &'static content::Block,
    pub data: BlockData,
}

impl Block {
    pub fn new(name: &str, position: Point2, config: &Object, vm: &LogicVM) -> VMLoadResult<Self> {
        let data = match name {
            MICRO_PROCESSOR | LOGIC_PROCESSOR | HYPER_PROCESSOR | WORLD_PROCESSOR => {
                return Self::new_processor(
                    name,
                    position,
                    &ProcessorBuilder::parse_config(config)?,
                    vm,
                );
            }

            MEMORY_CELL => BlockData::Memory([0.; 64].into()),
            MEMORY_BANK => BlockData::Memory([0.; 512].into()),
            WORLD_CELL => BlockData::Memory([0.; 512].into()),

            MESSAGE | WORLD_MESSAGE => BlockData::Message(match config {
                Object::String(Some(value)) if value.len() <= MESSAGE_MAX_LEN => {
                    let mut result = String::new();
                    let mut count = 0;
                    for c in value.trim().chars() {
                        if c == '\n' {
                            if count >= MESSAGE_MAX_LINES {
                                continue;
                            }
                            count += 1;
                        }
                        result.push(c);
                    }
                    ProcessorState::encode_utf16(&result).collect()
                }
                _ => Vec::new(),
            }),

            SWITCH | WORLD_SWITCH => BlockData::Switch(match config {
                &Object::Bool(value) => value,
                _ => false,
            }),

            _ => BlockData::Unknown {
                config: config.clone(),
            },
        };

        Self::from_data(name, position, data)
    }

    pub fn new_processor(
        name: &str,
        position: Point2,
        config: &ProcessorConfig,
        vm: &LogicVM,
    ) -> VMLoadResult<Self> {
        let data = match name {
            MICRO_PROCESSOR => BlockData::Processor(
                ProcessorBuilder {
                    ipt: 2,
                    range: 8. * 10.,
                    privileged: false,
                    running_processors: Rc::clone(&vm.running_processors),
                    time: Rc::clone(&vm.time),
                    globals: &vm.globals,
                    config,
                }
                .build()?,
            ),
            LOGIC_PROCESSOR => BlockData::Processor(
                ProcessorBuilder {
                    ipt: 8,
                    range: 8. * 22.,
                    privileged: false,
                    running_processors: Rc::clone(&vm.running_processors),
                    time: Rc::clone(&vm.time),
                    globals: &vm.globals,
                    config,
                }
                .build()?,
            ),
            HYPER_PROCESSOR => BlockData::Processor(
                ProcessorBuilder {
                    ipt: 25,
                    range: 8. * 42.,
                    privileged: false,
                    running_processors: Rc::clone(&vm.running_processors),
                    time: Rc::clone(&vm.time),
                    globals: &vm.globals,
                    config,
                }
                .build()?,
            ),
            WORLD_PROCESSOR => BlockData::Processor(
                ProcessorBuilder {
                    ipt: 8,
                    range: f32::MAX,
                    privileged: true,
                    running_processors: Rc::clone(&vm.running_processors),
                    time: Rc::clone(&vm.time),
                    globals: &vm.globals,
                    config,
                }
                .build()?,
            ),
            _ => {
                return Err(VMLoadError::BadBlockType {
                    want: "processor".to_string(),
                    got: name.to_string(),
                });
            }
        };

        Self::from_data(name, position, data)
    }

    pub fn from_schematic_tile(
        SchematicTile {
            block: name,
            position,
            config,
            ..
        }: &SchematicTile,
        vm: &LogicVM,
    ) -> VMLoadResult<Self> {
        Self::new(name, (*position).into(), config, vm)
    }

    pub fn from_data(name: &str, position: Point2, data: BlockData) -> VMLoadResult<Self> {
        let content = *content::blocks::FROM_NAME
            .get(name)
            .ok_or_else(|| VMLoadError::UnknownBlockType(name.to_string()))?;

        Ok(Self {
            name: name.to_string(),
            position,
            content,
            data,
        })
    }
}

#[derive(IntoStaticStr)]
pub enum BlockData {
    Processor(Processor),
    Memory(Box<[f64]>),
    Message(Vec<u16>),
    Switch(bool),
    Unknown { config: Object },
}

impl BlockData {
    /// # Panics
    ///
    /// Panics if this block is not a processor.
    pub fn into_processor(self) -> Processor {
        match self {
            Self::Processor(processor) => processor,
            _ => panic!(
                "called `BlockData::into_processor()` on a `BlockData::{}` value",
                <&str>::from(self)
            ),
        }
    }

    /// # Panics
    ///
    /// Panics if this block is not a processor.
    pub fn unwrap_processor(&self) -> &Processor {
        match self {
            BlockData::Processor(processor) => processor,
            _ => panic!(
                "called `BlockData::unwrap_processor()` on a `BlockData::{}` value",
                <&str>::from(self)
            ),
        }
    }

    /// # Panics
    ///
    /// Panics if this block is not a processor.
    pub fn unwrap_processor_mut(&mut self) -> &mut Processor {
        match self {
            BlockData::Processor(processor) => processor,
            _ => panic!(
                "called `BlockData::unwrap_processor_mut()` on a `BlockData::{}` value",
                <&str>::from(&*self)
            ),
        }
    }
}
