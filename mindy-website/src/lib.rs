#![allow(clippy::boxed_local)]

use std::{collections::HashMap, time::Duration};

use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics_web_simulator::{
    display::WebSimulatorDisplay, output_settings::OutputSettings,
};
use js_sys::JsString;
use mindy::{
    parser::LogicParser,
    types::{PackedPoint2, ProcessorConfig, ProcessorLinkConfig, content},
    vm::{
        Building, BuildingData, EmbeddedDisplayData, InstructionResult, LVar, LogicVM,
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
pub fn pack_point(x: i16, y: i16) -> u32 {
    ((y as u32) << 16) | (x as u32)
}

fn unpack_point(position: u32) -> PackedPoint2 {
    PackedPoint2 {
        x: position as i16,
        y: (position >> 16) as i16,
    }
}

#[wasm_bindgen]
pub struct WebLogicVM {
    vm: LogicVM,
    globals: Constants,
    logic_parser: LogicParser,
    prev_timestamp: Option<f64>,
}

#[wasm_bindgen]
impl WebLogicVM {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            vm: LogicVM::new(),
            globals: LVar::create_global_constants(),
            logic_parser: LogicParser::new(),
            prev_timestamp: None,
        }
    }

    #[wasm_bindgen(getter)]
    pub fn time(&self) -> f64 {
        self.vm.time().as_secs_f64()
    }

    pub fn add_processor(
        &mut self,
        position: u32,
        kind: ProcessorKind,
        code: String,
    ) -> Result<(), String> {
        let position = unpack_point(position);
        self.vm
            .add_building(
                Building::from_processor_config(
                    kind.name(),
                    position,
                    &ProcessorConfig {
                        code,
                        links: vec![],
                    },
                    &self.vm,
                )
                .map_err(|e| e.to_string())?,
                &self.globals,
            )
            .map_err(|e| e.to_string())
    }

    pub fn add_display(
        &mut self,
        position: u32,
        width: u32,
        height: u32,
        parent: &Element,
    ) -> Result<(), String> {
        let display = WebSimulatorDisplay::<Rgb888>::new(
            (width, height),
            &OutputSettings::default(),
            Some(parent),
        );

        let display_data = EmbeddedDisplayData::new(
            display,
            Some(Box::new(|display| {
                display.flush().expect("failed to flush display");
                InstructionResult::Ok
            })),
        )
        .expect("failed to initialize display");

        self.vm
            .add_building(
                Building::new(
                    content::blocks::FROM_NAME["tile-logic-display"],
                    unpack_point(position),
                    display_data.into(),
                ),
                &self.globals,
            )
            .map_err(|e| e.to_string())
    }

    pub fn set_processor_config(
        &mut self,
        position: u32,
        code: &str,
        links: Box<[u32]>,
    ) -> Result<LinkNames, String> {
        let ast = self.logic_parser.parse(code).map_err(|e| e.to_string())?;

        let position = unpack_point(position);
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
                Some(
                    &links
                        .iter()
                        .map(|p| {
                            let p = unpack_point(*p);
                            ProcessorLinkConfig::unnamed(p.x - position.x, p.y - position.y)
                        })
                        .collect::<Vec<_>>(),
                ),
                &self.vm,
                building,
                &self.globals,
            )
            .map_err(|e| e.to_string())?;

        Ok(LinkNames(
            processor
                .state
                .links()
                .iter()
                .map(|l| {
                    (
                        pack_point(l.building.position.x, l.building.position.y),
                        l.name.clone(),
                    )
                })
                .collect(),
        ))
    }

    pub fn building_name(&self, position: u32) -> Option<JsString> {
        self.vm
            .building(unpack_point(position))
            .map(|b| JsString::from_char_code(b.block.name.as_u16str().as_slice()))
    }

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
}

impl Default for WebLogicVM {
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

#[wasm_bindgen]
pub struct LinkNames(HashMap<u32, String>);

#[wasm_bindgen]
impl LinkNames {
    #[wasm_bindgen(indexing_getter)]
    pub fn get(&self, position: u32) -> Option<String> {
        self.0.get(&position).cloned()
    }
}
