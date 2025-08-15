#![allow(clippy::boxed_local)]

use std::time::Duration;

use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics_web_simulator::{
    display::WebSimulatorDisplay, output_settings::OutputSettings,
};
use mindy::{
    parser::LogicParser,
    types::{PackedPoint2, ProcessorConfig, ProcessorLinkConfig, content},
    vm::{
        Building, BuildingData, EmbeddedDisplayData, InstructionResult, LVar, LogicVM,
        LogicVMBuilder,
        buildings::{HYPER_PROCESSOR, LOGIC_PROCESSOR, MICRO_PROCESSOR, WORLD_PROCESSOR},
        variables::Constants,
    },
};
use wasm_bindgen::prelude::*;
use web_sys::Element;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

const MAX_DELTA: f64 = 6.;

#[allow(unused_macros)]
macro_rules! log {
    ($($t:tt)*) => {
        web_sys::console::log_1(&format!($($t)*).into());
    };
}

#[wasm_bindgen]
pub fn init() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub struct WebLogicVM {
    vm: LogicVM,
    globals: Constants,
    logic_parser: LogicParser,
    prev_timestamp: Option<f64>,
}

impl WebLogicVM {
    fn new(vm: LogicVM, globals: Constants) -> Self {
        Self {
            vm,
            globals,
            logic_parser: LogicParser::new(),
            prev_timestamp: None,
        }
    }
}

#[wasm_bindgen]
impl WebLogicVM {
    pub fn do_tick(&mut self, timestamp: f64) {
        // convert to seconds
        let timestamp = timestamp / 1000.;

        // nominal 60 ticks per second
        let delta = match self.prev_timestamp {
            Some(prev_timestamp) => (timestamp - prev_timestamp) * 60.,
            None => 1.,
        }
        .min(MAX_DELTA);

        self.prev_timestamp = Some(timestamp);
        self.vm
            .do_tick_with_delta(Duration::from_secs_f64(timestamp), delta);
    }

    #[wasm_bindgen(getter)]
    pub fn time(&self) -> f64 {
        self.vm.time().as_secs_f64()
    }

    pub fn set_processor_config(
        &mut self,
        position: PackedPoint2,
        code: &str,
        links: Option<Box<[PackedPoint2]>>,
    ) -> Result<(), String> {
        let ast = self.logic_parser.parse(code).map_err(|e| e.to_string())?;

        let building = self
            .vm
            .building(position)
            .ok_or_else(|| format!("building does not exist: {position}"))?;

        let BuildingData::Processor(processor) = &mut *building.data.borrow_mut() else {
            return Err(format!(
                "expected processor at {position} but got {}",
                building.block.name
            ));
        };

        processor
            .update_config(
                ast,
                links
                    .map(|v| {
                        v.iter()
                            .map(|p| {
                                ProcessorLinkConfig::unnamed(p.x - position.x, p.y - position.y)
                            })
                            .collect::<Vec<_>>()
                    })
                    .as_deref(),
                &self.vm,
                building,
                &self.globals,
            )
            .map_err(|e| e.to_string())
    }
}

#[wasm_bindgen]
pub struct WebLogicVMBuilder {
    builder: LogicVMBuilder,
}

#[wasm_bindgen]
impl WebLogicVMBuilder {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            builder: LogicVMBuilder::new(),
        }
    }

    pub fn add_processor(&mut self, position: PackedPoint2, kind: ProcessorKind) {
        self.builder.add_building(
            Building::from_processor_config(
                kind.name(),
                position,
                &ProcessorConfig::default(),
                &self.builder,
            )
            .expect("failed to create processor"),
        );
    }

    pub fn add_display(
        &mut self,
        position: PackedPoint2,
        width: u32,
        height: u32,
        parent: &Element,
    ) {
        let display = WebSimulatorDisplay::<Rgb888>::new(
            (width, height),
            &OutputSettings::default(),
            Some(parent),
        );

        let display_data = EmbeddedDisplayData::new(
            display,
            Some(Box::new(|display| {
                display.flush().expect("failed to flush display");
                InstructionResult::Yield
            })),
        )
        .expect("failed to initialize display");

        self.builder.add_building(Building::new(
            content::blocks::FROM_NAME["tile-logic-display"],
            position,
            display_data.into(),
        ));
    }

    pub fn build(self) -> Result<WebLogicVM, String> {
        let globals = LVar::create_global_constants();
        match self.builder.build_with_globals(&globals) {
            Ok(vm) => Ok(WebLogicVM::new(vm, globals)),
            Err(e) => Err(e.to_string()),
        }
    }
}

impl Default for WebLogicVMBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
pub enum ProcessorKind {
    Micro,
    Logic,
    Hyper,
    World,
}

impl ProcessorKind {
    fn name(&self) -> &str {
        match self {
            Self::Micro => MICRO_PROCESSOR,
            Self::Logic => LOGIC_PROCESSOR,
            Self::Hyper => HYPER_PROCESSOR,
            Self::World => WORLD_PROCESSOR,
        }
    }
}
