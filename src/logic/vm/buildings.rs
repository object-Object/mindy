use std::{cell::RefCell, rc::Rc};

use strum_macros::IntoStaticStr;

use super::{
    LogicVM, VMLoadError, VMLoadResult,
    processor::{Processor, ProcessorBuilder, ProcessorState},
};
use crate::types::{
    Object, Point2, ProcessorConfig, SchematicTile,
    content::{self, Block},
};

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

pub struct Building {
    pub block: &'static Block,
    pub position: Point2,
    pub data: Rc<RefCell<BuildingData>>,
}

impl Building {
    pub fn new(name: &str, position: Point2, data: BuildingData) -> VMLoadResult<Self> {
        let block = *content::blocks::FROM_NAME
            .get(name)
            .ok_or_else(|| VMLoadError::UnknownBlockType(name.to_string()))?;

        Ok(Self {
            block,
            position,
            data: Rc::new(RefCell::new(data)),
        })
    }

    pub fn from_config(
        name: &str,
        position: Point2,
        config: &Object,
        vm: &LogicVM,
    ) -> VMLoadResult<Self> {
        let data = match name {
            MICRO_PROCESSOR | LOGIC_PROCESSOR | HYPER_PROCESSOR | WORLD_PROCESSOR => {
                return Self::from_processor_config(
                    name,
                    position,
                    &ProcessorBuilder::parse_config(config)?,
                    vm,
                );
            }

            MEMORY_CELL => BuildingData::Memory([0.; 64].into()),
            MEMORY_BANK => BuildingData::Memory([0.; 512].into()),
            WORLD_CELL => BuildingData::Memory([0.; 512].into()),

            MESSAGE | WORLD_MESSAGE => BuildingData::Message(match config {
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

            SWITCH | WORLD_SWITCH => BuildingData::Switch(match config {
                &Object::Bool(value) => value,
                _ => false,
            }),

            _ => BuildingData::Unknown {
                config: config.clone(),
            },
        };

        Self::new(name, position, data)
    }

    pub fn from_processor_config(
        name: &str,
        position: Point2,
        config: &ProcessorConfig,
        vm: &LogicVM,
    ) -> VMLoadResult<Self> {
        let data = match name {
            MICRO_PROCESSOR => BuildingData::Processor(
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
            LOGIC_PROCESSOR => BuildingData::Processor(
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
            HYPER_PROCESSOR => BuildingData::Processor(
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
            WORLD_PROCESSOR => BuildingData::Processor(
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

        Self::new(name, position, data)
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
        Self::from_config(name, (*position).into(), config, vm)
    }
}

#[derive(IntoStaticStr)]
pub enum BuildingData {
    Processor(Processor),
    Memory(Box<[f64]>),
    Message(Vec<u16>),
    Switch(bool),
    Unknown { config: Object },
}

impl BuildingData {
    /// # Panics
    ///
    /// Panics if this building is not a processor.
    pub fn into_processor(self) -> Processor {
        match self {
            Self::Processor(processor) => processor,
            _ => panic!(
                "called `BuildingData::into_processor()` on a `BuildingData::{}` value",
                <&str>::from(self)
            ),
        }
    }

    /// # Panics
    ///
    /// Panics if this building is not a processor.
    pub fn unwrap_processor(&self) -> &Processor {
        match self {
            BuildingData::Processor(processor) => processor,
            _ => panic!(
                "called `BuildingData::unwrap_processor()` on a `BuildingData::{}` value",
                <&str>::from(self)
            ),
        }
    }

    /// # Panics
    ///
    /// Panics if this building is not a processor.
    pub fn unwrap_processor_mut(&mut self) -> &mut Processor {
        match self {
            BuildingData::Processor(processor) => processor,
            _ => panic!(
                "called `BuildingData::unwrap_processor_mut()` on a `BuildingData::{}` value",
                <&str>::from(&*self)
            ),
        }
    }
}
