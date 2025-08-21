use std::borrow::Cow;

use mindy::{
    types::LAccess,
    vm::{Building, CustomBuildingData, InstructionResult, LValue, LogicVM, instructions},
};
use widestring::U16String;

use crate::utils::{on_building_change, u16str_to_js};

pub struct WebMessageData {
    value: U16String,
    on_building_change: js_sys::Function,
}

impl WebMessageData {
    pub fn new(on_building_change: js_sys::Function) -> Self {
        Self {
            value: U16String::new(),
            on_building_change,
        }
    }
}

impl CustomBuildingData for WebMessageData {
    fn read(&mut self, _: &Building, _: &LogicVM, address: Cow<'_, LValue>) -> Option<LValue> {
        Some(instructions::Read::read_slice(self.value.as_slice(), &address).into())
    }

    fn printflush(
        &mut self,
        building: &Building,
        _: &LogicVM,
        printbuffer: U16String,
    ) -> InstructionResult {
        self.value = printbuffer;
        on_building_change(
            &self.on_building_change,
            building.position,
            "message",
            u16str_to_js(&self.value),
        );
        InstructionResult::Ok
    }

    fn sensor(&mut self, _: &Building, _: &LogicVM, sensor: LAccess) -> Option<LValue> {
        Some(match sensor {
            LAccess::BufferSize => self.value.len().into(),
            _ => return None,
        })
    }
}

pub struct WebSwitchData {
    enabled: bool,
    on_building_change: js_sys::Function,
}

impl WebSwitchData {
    pub fn new(on_building_change: js_sys::Function) -> Self {
        Self {
            enabled: false,
            on_building_change,
        }
    }
}

impl CustomBuildingData for WebSwitchData {
    fn control(
        &mut self,
        building: &Building,
        _: &LogicVM,
        control: LAccess,
        p1: Cow<'_, LValue>,
        _: Cow<'_, LValue>,
        _: Cow<'_, LValue>,
    ) -> InstructionResult {
        if control == LAccess::Enabled {
            self.enabled = p1.bool();
            on_building_change(
                &self.on_building_change,
                building.position,
                "switch",
                self.enabled,
            );
        }
        InstructionResult::Ok
    }

    fn sensor(&mut self, _: &Building, _: &LogicVM, sensor: LAccess) -> Option<LValue> {
        Some(match sensor {
            LAccess::Enabled => self.enabled.into(),
            _ => return None,
        })
    }
}
