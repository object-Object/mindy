#![allow(clippy::boxed_local)]

use std::{borrow::Cow, time::Duration};

use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics_web_simulator::{
    display::WebSimulatorDisplay, output_settings::OutputSettings,
};
use js_sys::JsString;
use mindy::{
    parser::LogicParser,
    types::{LAccess, Object, ProcessorConfig, ProcessorLinkConfig, content},
    vm::{
        Building, BuildingData, Content, EmbeddedDisplayData, InstructionResult, LValue, LVar,
        LogicVM,
        buildings::{MESSAGE, SWITCH},
        variables::Constants,
    },
};
use wasm_bindgen::prelude::*;
use web_sys::{OffscreenCanvas, Performance, WorkerGlobalScope};

pub use self::enums::*;
use self::{buildings::*, utils::*};

mod buildings;
mod enums;
mod utils;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[allow(unused_macros)]
macro_rules! log {
    ($($t:tt)*) => {
        web_sys::console::log_1(&format!($($t)*).into());
    };
}

#[wasm_bindgen]
pub fn init_logging() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub struct WebLogicVM {
    vm: LogicVM,
    globals: Constants,
    logic_parser: LogicParser,
    performance: Performance,
    delta: f64,
    tick_secs: f64,
    next_tick_end: f64,
    on_building_change: js_sys::Function,
}

#[wasm_bindgen]
impl WebLogicVM {
    #[wasm_bindgen(constructor)]
    pub fn new(target_fps: f64, on_building_change: js_sys::Function) -> Self {
        let delta = fps_to_delta(target_fps);
        let tick_secs = delta_to_time(delta);
        Self {
            vm: LogicVM::new(),
            globals: LVar::create_global_constants(),
            logic_parser: LogicParser::new(),
            performance: js_sys::global()
                .dyn_into::<WorkerGlobalScope>()
                .expect("failed to cast global to WorkerGlobalScope")
                .performance()
                .expect("failed to get performance object"),
            delta,
            tick_secs,
            next_tick_end: 0.,
            on_building_change,
        }
    }

    #[wasm_bindgen(getter)]
    pub fn time(&self) -> f64 {
        self.vm.time().as_secs_f64()
    }

    pub fn add_display(
        &mut self,
        position: u32,
        kind: DisplayKind,
        width: u32,
        height: u32,
        canvas: &OffscreenCanvas,
    ) -> Result<(), String> {
        let display = WebSimulatorDisplay::<Rgb888, _>::from_offscreen_canvas(
            (width, height),
            &OutputSettings::default(),
            canvas,
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
                    content::blocks::FROM_NAME[kind.name()],
                    unpack_point(position),
                    display_data.into(),
                ),
                &self.globals,
            )
            .map_err(|e| e.to_string())
    }

    pub fn add_memory(&mut self, position: u32, kind: MemoryKind) -> Result<(), String> {
        self.vm
            .add_building(
                Building::from_config(kind.name(), unpack_point(position), &Object::Null, &self.vm)
                    .map_err(|e| e.to_string())?,
                &self.globals,
            )
            .map_err(|e| e.to_string())
    }

    pub fn add_message(&mut self, position: u32) -> Result<(), String> {
        self.vm
            .add_building(
                Building::new(
                    content::blocks::FROM_NAME[MESSAGE],
                    unpack_point(position),
                    WebMessageData::new(self.on_building_change.clone()).into(),
                ),
                &self.globals,
            )
            .map_err(|e| e.to_string())
    }

    pub fn add_processor(&mut self, position: u32, kind: ProcessorKind) -> Result<(), String> {
        let position = unpack_point(position);
        self.vm
            .add_building(
                Building::from_processor_config(
                    kind.name(),
                    position,
                    &ProcessorConfig::default(),
                    &self.vm,
                )
                .map_err(|e| e.to_string())?,
                &self.globals,
            )
            .map_err(|e| e.to_string())
    }

    pub fn add_sorter(&mut self, position: u32) -> Result<(), String> {
        self.vm
            .add_building(
                Building::new(
                    content::blocks::FROM_NAME["sorter"],
                    unpack_point(position),
                    WebSorterData::new(self.on_building_change.clone()).into(),
                ),
                &self.globals,
            )
            .map_err(|e| e.to_string())
    }

    pub fn add_switch(&mut self, position: u32) -> Result<(), String> {
        self.vm
            .add_building(
                Building::new(
                    content::blocks::FROM_NAME[SWITCH],
                    unpack_point(position),
                    WebSwitchData::new(self.on_building_change.clone()).into(),
                ),
                &self.globals,
            )
            .map_err(|e| e.to_string())
    }

    pub fn set_message_text(&mut self, position: u32, value: &JsString) -> Result<(), String> {
        let position = unpack_point(position);
        let building = self
            .vm
            .building(position)
            .ok_or_else(|| format!("building does not exist: {position}"))?;

        let BuildingData::Custom(custom) = &mut *building.data.borrow_mut() else {
            return Err(format!(
                "expected message at {position} but got {}",
                building.block.name
            ));
        };

        let _ = custom.printflush(building, &self.vm, u16string_from_js(value));

        Ok(())
    }

    pub fn set_processor_config(
        &mut self,
        position: u32,
        code: &str,
        links: Box<[u32]>,
    ) -> Result<js_sys::Map, String> {
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

        let names = js_sys::Map::new();
        for link in processor.state.links() {
            names.set(
                &pack_point(link.building.position).into(),
                &link.name.clone().into(),
            );
        }
        Ok(names)
    }

    pub fn set_sorter_config(
        &mut self,
        position: u32,
        logic_id: Option<i32>,
    ) -> Result<(), String> {
        let position = unpack_point(position);

        let item = match logic_id {
            Some(logic_id) => Content::Item(
                *content::items::FROM_LOGIC_ID
                    .get(&logic_id)
                    .ok_or_else(|| format!("invalid logic id: {logic_id}"))?,
            )
            .into(),
            None => LValue::NULL,
        };

        let building = self
            .vm
            .building(position)
            .ok_or_else(|| format!("building does not exist: {position}"))?;

        let BuildingData::Custom(custom) = &mut *building.data.borrow_mut() else {
            return Err(format!(
                "expected switch at {position} but got {}",
                building.block.name
            ));
        };

        let _ = custom.control(
            building,
            &self.vm,
            LAccess::Config,
            Cow::Owned(item),
            Default::default(),
            Default::default(),
        );

        Ok(())
    }

    pub fn set_switch_enabled(&mut self, position: u32, value: bool) -> Result<(), String> {
        let position = unpack_point(position);
        let building = self
            .vm
            .building(position)
            .ok_or_else(|| format!("building does not exist: {position}"))?;

        let BuildingData::Custom(custom) = &mut *building.data.borrow_mut() else {
            return Err(format!(
                "expected switch at {position} but got {}",
                building.block.name
            ));
        };

        let _ = custom.control(
            building,
            &self.vm,
            LAccess::Enabled,
            Cow::Owned(value.into()),
            Default::default(),
            Default::default(),
        );

        Ok(())
    }

    pub fn remove_building(&mut self, position: u32) {
        self.vm.remove_building(unpack_point(position));
    }

    pub fn building_name(&self, position: u32) -> Option<JsString> {
        self.vm
            .building(unpack_point(position))
            .map(|b| u16str_to_js(b.block.name.as_u16str()))
    }

    pub fn set_target_fps(&mut self, target_fps: f64) {
        self.delta = fps_to_delta(target_fps);
        self.tick_secs = delta_to_time(self.delta);
        self.next_tick_end = 0.;
    }

    pub fn do_tick(&mut self) {
        let mut time;
        loop {
            time = self.performance.now() / 1000.;
            self.vm
                .do_tick_with_delta(Duration::from_secs_f64(time), self.delta);
            if time >= self.next_tick_end {
                break;
            }
        }
        self.next_tick_end = time + self.tick_secs;
    }
}
