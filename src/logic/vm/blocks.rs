use std::{rc::Rc, str::FromStr};

use strum_macros::{EnumString, IntoStaticStr};

use self::BlockType::*;
use super::{
    LogicVM, VMLoadError, VMLoadResult,
    processor::{Processor, ProcessorBuilder},
};
use crate::types::{Object, ProcessorConfig, SchematicTile, content};

const MESSAGE_MAX_LEN: usize = 220;
const MESSAGE_MAX_LINES: usize = 24;

#[derive(IntoStaticStr)]
pub enum Block {
    Processor(Processor),
    Memory(Box<[f64]>),
    Message(String),
    Switch(bool),
    Unknown { block: String, config: Object },
}

impl Block {
    /// Returns: block, size
    pub fn new(block: BlockType, config: &Object, vm: &LogicVM) -> VMLoadResult<(Self, i32)> {
        Ok(match block {
            MicroProcessor | LogicProcessor | HyperProcessor | WorldProcessor => {
                Self::new_processor(block, &ProcessorBuilder::parse_config(config)?, vm)?
            }

            MemoryCell => (Self::Memory([0.; 64].into()), 1),
            MemoryBank => (Self::Memory([0.; 512].into()), 2),
            WorldCell => (Self::Memory([0.; 512].into()), 1),

            Message | WorldMessage => (
                Self::Message(match config {
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
                        result
                    }
                    _ => String::new(),
                }),
                1,
            ),

            Switch | WorldSwitch => (
                Self::Switch(match config {
                    &Object::Bool(value) => value,
                    _ => false,
                }),
                1,
            ),
        })
    }

    /// Returns: block, size
    pub fn new_processor(
        block: BlockType,
        config: &ProcessorConfig,
        vm: &LogicVM,
    ) -> VMLoadResult<(Self, i32)> {
        Ok(match block {
            MicroProcessor => (
                Self::Processor(
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
                1,
            ),
            LogicProcessor => (
                Self::Processor(
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
                2,
            ),
            HyperProcessor => (
                Self::Processor(
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
                3,
            ),
            WorldProcessor => (
                Self::Processor(
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
                1,
            ),
            _ => {
                return Err(VMLoadError::BadBlockType {
                    want: "processor".into(),
                    got: block,
                });
            }
        })
    }

    /// Returns: block, size
    pub fn from_schematic_tile(
        SchematicTile { block, config, .. }: &SchematicTile,
        vm: &LogicVM,
    ) -> VMLoadResult<(Self, i32)> {
        match BlockType::from_str(block) {
            Ok(block) => Self::new(block, config, vm),
            Err(_) => Ok((
                Self::Unknown {
                    block: block.clone(),
                    config: config.clone(),
                },
                content::blocks::FROM_NAME
                    .get(block.as_str())
                    .map(|v| v.size)
                    .unwrap_or(1),
            )),
        }
    }

    /// # Panics
    ///
    /// Panics if this block is not a processor.
    pub fn into_processor(self) -> Processor {
        match self {
            Block::Processor(processor) => processor,
            _ => panic!(
                "called `Block::into_processor()` on a `Block::{}` value",
                <&str>::from(self)
            ),
        }
    }

    /// # Panics
    ///
    /// Panics if this block is not a processor.
    pub fn unwrap_processor(&self) -> &Processor {
        match self {
            Block::Processor(processor) => processor,
            _ => panic!(
                "called `Block::unwrap_processor()` on a `Block::{}` value",
                <&str>::from(self)
            ),
        }
    }

    /// # Panics
    ///
    /// Panics if this block is not a processor.
    pub fn unwrap_processor_mut(&mut self) -> &mut Processor {
        match self {
            Block::Processor(processor) => processor,
            _ => panic!(
                "called `Block::unwrap_processor_mut()` on a `Block::{}` value",
                <&str>::from(&*self)
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumString, strum_macros::Display)]
pub enum BlockType {
    #[strum(serialize = "micro-processor")]
    MicroProcessor,
    #[strum(serialize = "logic-processor")]
    LogicProcessor,
    #[strum(serialize = "hyper-processor")]
    HyperProcessor,
    #[strum(serialize = "world-processor")]
    WorldProcessor,

    #[strum(serialize = "memory-cell")]
    MemoryCell,
    #[strum(serialize = "memory-bank")]
    MemoryBank,
    #[strum(serialize = "world-cell")]
    WorldCell,

    #[strum(serialize = "message")]
    Message,
    #[strum(serialize = "world-message")]
    WorldMessage,

    #[strum(serialize = "switch")]
    Switch,
    #[strum(serialize = "world-switch")]
    WorldSwitch,
}
