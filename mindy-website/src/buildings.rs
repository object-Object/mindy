use std::borrow::Cow;

use mindy::{
    types::{LAccess, content::Item},
    vm::{
        Building, Content, CustomBuildingData, InstructionResult, LObject, LValue, LogicVM,
        instructions,
    },
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

pub struct WebSorterData {
    item: Option<&'static Item>,
    on_building_change: js_sys::Function,
}

impl WebSorterData {
    pub fn new(on_building_change: js_sys::Function) -> Self {
        Self {
            item: None,
            on_building_change,
        }
    }
}

impl CustomBuildingData for WebSorterData {
    fn control(
        &mut self,
        building: &Building,
        _: &LogicVM,
        control: LAccess,
        p1: Cow<'_, LValue>,
        _: Cow<'_, LValue>,
        _: Cow<'_, LValue>,
    ) -> InstructionResult {
        if control == LAccess::Config {
            self.item = match p1.obj() {
                Some(LObject::Content(Content::Item(item))) => Some(*item),
                Some(LObject::Null) => None,
                _ => return InstructionResult::Ok,
            };

            on_building_change(
                &self.on_building_change,
                building.position,
                "sorter",
                self.item.map(|v| v.logic_id),
            );
        }
        InstructionResult::Ok
    }

    fn sensor(&mut self, _: &Building, _: &LogicVM, sensor: LAccess) -> Option<LValue> {
        Some(match sensor {
            LAccess::Config => match self.item {
                Some(item) => Content::Item(item).into(),
                None => LValue::NULL,
            },
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
