use alloc::{boxed::Box, rc::Rc, string::ToString};
use core::cell::RefCell;

use derivative::Derivative;
use itertools::Itertools;
use strum::IntoStaticStr;
use widestring::U16String;

use super::{
    InstructionResult, LObject, LValue, LVar, LogicVM, Processor, ProcessorBuilder, ProcessorState,
    VMLoadError, VMLoadResult,
};
use crate::types::{
    LAccess, Object, PackedPoint2,
    content::{self, Block},
};
#[cfg(feature = "std")]
use crate::types::{ProcessorConfig, SchematicTile};

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

#[derive(Debug, Clone)]
pub struct Building {
    pub block: &'static Block,
    pub position: PackedPoint2,
    pub data: Rc<RefCell<BuildingData>>,
}

impl Building {
    pub fn new(block: &'static Block, position: PackedPoint2, data: BuildingData) -> Self {
        Self {
            block,
            position,
            data: Rc::new(RefCell::new(data)),
        }
    }

    pub fn from_name(name: &str, position: PackedPoint2, data: BuildingData) -> VMLoadResult<Self> {
        Ok(Self::new(Self::get_block(name)?, position, data))
    }

    pub fn from_config(
        name: &str,
        position: PackedPoint2,
        config: &Object,
        _vm: impl AsRef<LogicVM>,
    ) -> VMLoadResult<Self> {
        let data = match name {
            MICRO_PROCESSOR | LOGIC_PROCESSOR | HYPER_PROCESSOR | WORLD_PROCESSOR => {
                #[cfg(feature = "std")]
                return Self::from_processor_config(
                    name,
                    position,
                    &ProcessorConfig::parse(config)?,
                    _vm,
                );
                #[cfg(not(feature = "std"))]
                panic!("processor config parsing is not supported on no_std");
            }

            MEMORY_CELL => BuildingData::Memory([0.; 64].into()),
            MEMORY_BANK => BuildingData::Memory([0.; 512].into()),
            WORLD_CELL => BuildingData::Memory([0.; 512].into()),

            MESSAGE | WORLD_MESSAGE => BuildingData::Message(match config {
                Object::String(Some(value)) if value.len() <= MESSAGE_MAX_LEN => {
                    let mut result = U16String::new();
                    let mut count = 0;
                    for c in value.trim().chars() {
                        if c == '\n' {
                            if count >= MESSAGE_MAX_LINES {
                                continue;
                            }
                            count += 1;
                        }
                        result.push_char(c);
                    }
                    result
                }
                _ => U16String::new(),
            }),

            SWITCH | WORLD_SWITCH => BuildingData::Switch(match config {
                &Object::Bool(value) => value,
                _ => false,
            }),

            _ => BuildingData::Unknown {
                senseable_config: match *config {
                    Object::Content(content) => {
                        content.try_into().map(|v| LObject::Content(v).into()).ok()
                    }
                    _ => None,
                },
            },
        };

        Self::from_name(name, position, data)
    }

    #[cfg(feature = "std")]
    pub fn from_processor_config(
        name: &str,
        position: PackedPoint2,
        config: &ProcessorConfig,
        vm: impl AsRef<LogicVM>,
    ) -> VMLoadResult<Self> {
        let code = ProcessorBuilder::parse_code(&config.code)?;

        let data = match name {
            MICRO_PROCESSOR => ProcessorBuilder {
                ipt: 2.,
                privileged: false,
                code,
                links: &config.links,
                instruction_hook: None,
            },
            LOGIC_PROCESSOR => ProcessorBuilder {
                ipt: 8.,
                privileged: false,
                code,
                links: &config.links,
                instruction_hook: None,
            },
            HYPER_PROCESSOR => ProcessorBuilder {
                ipt: 25.,
                privileged: false,
                code,
                links: &config.links,
                instruction_hook: None,
            },
            WORLD_PROCESSOR => ProcessorBuilder {
                ipt: 8.,
                privileged: true,
                code,
                links: &config.links,
                instruction_hook: None,
            },
            _ => {
                return Err(VMLoadError::BadBlockType {
                    want: "processor".to_string(),
                    got: name.to_string(),
                });
            }
        };

        Ok(Self::from_processor_builder(
            Self::get_block(name)?,
            position,
            data,
            vm,
        ))
    }

    pub fn from_processor_builder(
        block: &'static Block,
        position: PackedPoint2,
        config: ProcessorBuilder,
        vm: impl AsRef<LogicVM>,
    ) -> Self {
        Self::new(
            block,
            position,
            BuildingData::Processor(config.build(position, vm)),
        )
    }

    #[cfg(feature = "std")]
    pub fn from_schematic_tile(
        SchematicTile {
            block: name,
            position,
            config,
            ..
        }: &SchematicTile,
        vm: impl AsRef<LogicVM>,
    ) -> VMLoadResult<Self> {
        Self::from_config(name, *position, config, vm)
    }

    /// Returns an iterator over all of the points contained within this building.
    ///
    /// For example, a building with size 2 would return an iterator yielding the following items:
    /// - `(x, y)`
    /// - `(x, y + 1)`
    /// - `(x + 1, y)`
    /// - `(x + 1, y + 1)`
    pub fn iter_positions(&self) -> impl Iterator<Item = PackedPoint2> + use<> {
        let PackedPoint2 { x, y } = self.position;
        let size = self.block.size;
        (x..x + size)
            .cartesian_product(y..y + size)
            .map(PackedPoint2::from)
    }

    fn get_block(name: &str) -> VMLoadResult<&'static Block> {
        content::blocks::FROM_NAME
            .get(name)
            .copied()
            .ok_or_else(|| VMLoadError::UnknownBlockType(name.to_string()))
    }
}

impl PartialEq for Building {
    fn eq(&self, other: &Self) -> bool {
        self.block == other.block && self.position == other.position
    }
}

macro_rules! borrow_data {
    (
        mut $ref:expr,
        $state:ident : $bind:ident => $expr1:expr,
        $data:ident => $expr2:expr $(,)?
    ) => {
        borrow_data!(
            @impl
            mut, (Rc::clone(&$ref).try_borrow_mut()),
            $bind, let $bind = &$state => $expr1,
            $data => $expr2
        )
    };
    (
        mut $ref:expr,
        $state:ident => $expr1:expr,
        $data:ident => $expr2:expr $(,)?
    ) => {
        borrow_data!(
            @impl
            mut, (Rc::clone(&$ref).try_borrow_mut()),
            $state => $expr1,
            $data => $expr2
        )
    };
    (
        $ref:expr,
        $state:ident : $bind:ident => $expr1:expr,
        $data:ident => $expr2:expr $(,)?
    ) => {
        borrow_data!(
            @impl
            (Rc::clone(&$ref).try_borrow()),
            $bind, let $bind = &$state => $expr1,
            $data => $expr2
        )
    };
    (
        $ref:expr,
        $state:ident => $expr1:expr,
        $data:ident => $expr2:expr $(,)?
    ) => {
        borrow_data!(
            @impl
            (Rc::clone(&$ref).try_borrow()),
            $state => $expr1,
            $data => $expr2
        )
    };
    (
        @impl
        $($mut:ident,)? $ref:expr,
        $state:ident $(, $bind:stmt)? => $expr1:expr,
        $data:ident => $expr2:expr
    ) => {
        match $ref {
            Ok($($mut)? data) => match &$($mut)? *data {
                BuildingData::Processor(p) => {
                    let $state = &$($mut)? p.state;
                    $expr1
                },
                $data => $expr2,
            },
            Err(_) => {
                $($bind)?
                $expr1
            },
        }
    };
}

pub(super) use borrow_data;

#[derive(Derivative, IntoStaticStr)]
#[derivative(Debug)]
#[non_exhaustive]
pub enum BuildingData {
    Processor(Box<Processor>),
    Memory(Box<[f64]>),
    Message(U16String),
    Switch(bool),
    Unknown { senseable_config: Option<LValue> },
    Custom(#[derivative(Debug = "ignore")] Box<dyn CustomBuildingData>),
}

impl BuildingData {
    /// # Panics
    ///
    /// Panics if this building is not a processor.
    pub fn into_processor(self) -> Processor {
        match self {
            Self::Processor(processor) => *processor,
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

impl<T> From<T> for BuildingData
where
    T: CustomBuildingData + 'static,
{
    fn from(value: T) -> Self {
        Self::Custom(Box::new(value))
    }
}

#[allow(unused_variables)]
pub trait CustomBuildingData {
    #[must_use]
    fn read(
        &mut self,
        state: &mut ProcessorState,
        vm: &LogicVM,
        address: LValue,
    ) -> Option<LValue> {
        None
    }

    #[must_use]
    fn write(
        &mut self,
        state: &mut ProcessorState,
        vm: &LogicVM,
        address: LValue,
        value: LValue,
    ) -> InstructionResult {
        InstructionResult::Ok
    }

    #[must_use]
    fn drawflush(&mut self, state: &mut ProcessorState, vm: &LogicVM) -> InstructionResult {
        InstructionResult::Ok
    }

    #[must_use]
    fn printflush(&mut self, state: &mut ProcessorState, vm: &LogicVM) -> InstructionResult {
        InstructionResult::Ok
    }

    #[must_use]
    fn control(
        &mut self,
        state: &mut ProcessorState,
        vm: &LogicVM,
        control: LAccess,
        p1: &LVar,
        p2: &LVar,
        p3: &LVar,
    ) -> InstructionResult {
        InstructionResult::Ok
    }

    #[must_use]
    fn sensor(
        &mut self,
        state: &mut ProcessorState,
        vm: &LogicVM,
        sensor: LAccess,
    ) -> Option<LValue> {
        None
    }
}
