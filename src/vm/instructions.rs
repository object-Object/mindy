use alloc::{borrow::Cow, format, rc::Rc, string::String};

use enum_dispatch::enum_dispatch;
#[cfg(feature = "std")]
use lazy_static::lazy_static;
#[cfg(feature = "std")]
use noise::{NoiseFn, Simplex};
use num_traits::AsPrimitive;
#[allow(unused_imports)]
use num_traits::float::FloatCore;
use widestring::{U16Str, u16str};

use super::{
    BuildingData, Content, DrawCommand, LObject, LString, LValue, LVar, LogicVM, ProcessorState,
    TextAlignment, VMLoadError, VMLoadResult,
    buildings::borrow_data,
    processor::{MAX_DRAW_BUFFER, MAX_TEXT_BUFFER},
    variables::{Constants, F64_DEG_RAD, F64_RAD_DEG, RAD_DEG},
};
use crate::{
    parser::ast::{self, ConditionOp, DrawOp, LogicOp, TileLayer},
    types::{
        ContentType, LAccess, PackedPoint2, Team,
        colors::{self, f32_to_double_bits, f64_from_double_bits, from_double_bits},
        content,
    },
    utils::{RapidHashMap, u16format},
    vm::variables::VariableIndex,
};

const MAX_IPT: i32 = 1000;
const EQUALITY_EPSILON: f64 = 0.000001;
const PRINT_EPSILON: f64 = 0.00001;

#[cfg(feature = "std")]
lazy_static! {
    static ref SIMPLEX: Simplex = Simplex::new(0);
}

#[enum_dispatch]
pub(super) trait InstructionTrait {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) -> InstructionResult;
}

trait SimpleInstructionTrait {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM);
}

impl<T: SimpleInstructionTrait> InstructionTrait for T {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) -> InstructionResult {
        self.execute(state, vm);
        InstructionResult::Ok
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionResult {
    Ok,
    Yield,
}

#[allow(clippy::enum_variant_names)]
#[enum_dispatch(InstructionTrait)]
#[derive(Debug)]
#[non_exhaustive]
pub enum Instruction {
    InstructionBuilder,
    // input/output
    Read,
    Write,
    Draw,
    Print,
    PrintChar,
    Format,
    // block control
    DrawFlush,
    PrintFlush,
    GetLink,
    Control,
    Sensor,
    // operations
    Set,
    Op,
    Select,
    Lookup,
    PackColor,
    UnpackColor,
    // flow control
    Noop,
    Wait,
    Stop,
    End,
    Jump,
    // privileged
    GetBlock,
    SetRate,
}

impl Default for Instruction {
    fn default() -> Self {
        Self::Noop(Noop)
    }
}

#[derive(Debug)]
pub struct InstructionBuilder {
    pub(super) instruction: ast::Instruction,
    pub(super) labels: Rc<RapidHashMap<String, usize>>,
}

impl InstructionBuilder {
    pub(super) fn late_init(
        self,
        globals: &Constants,
        state: &mut ProcessorState,
    ) -> VMLoadResult<Instruction> {
        // helpers

        let num_instructions = state.num_instructions();
        let privileged = state.privileged();
        let locals = &state.locals;
        let variables = &mut state.variables;

        let mut lvar = |value| match value {
            ast::Value::Variable(name) => {
                let name = name.into();
                // first check locals
                locals
                    .get(&name)
                    // then check globals
                    .or_else(|| globals.get(&name))
                    .cloned()
                    // then see if there's already a variable with this name
                    .or_else(|| {
                        variables
                            .get_index_of(&name)
                            .map(|i| LVar::Variable(VariableIndex(i)))
                    })
                    // if none of those exist, create a new variable defaulting to null
                    .unwrap_or_else(|| {
                        let (i, _) = variables.insert_full(name, LValue::NULL);
                        LVar::Variable(VariableIndex(i))
                    })
            }
            ast::Value::String(value) => LVar::Constant(value.into()),
            ast::Value::Number(value) => LVar::Constant(value.into()),
            ast::Value::None => LVar::Constant(LValue::NULL),
        };

        let jump_target =
            |value| match value {
                ast::Value::Variable(name) => self.labels.get(&name).copied().ok_or_else(|| {
                    VMLoadError::BadProcessorCode(format!("label not found: {name}"))
                }),

                ast::Value::Number(address) => {
                    let counter = address as usize;
                    if (0..num_instructions).contains(&counter) {
                        Ok(counter)
                    } else {
                        Err(VMLoadError::BadProcessorCode(format!(
                            "jump out of range: {}",
                            address.trunc()
                        )))
                    }
                }

                _ => unreachable!(),
            };

        // map AST instructions to handlers

        Ok(match self.instruction {
            // input/output
            ast::Instruction::Read {
                result,
                target,
                address,
            } => Read {
                result: lvar(result),
                target: lvar(target),
                address: lvar(address),
            }
            .into(),
            ast::Instruction::Write {
                value,
                target,
                address,
            } => Write {
                value: lvar(value),
                target: lvar(target),
                address: lvar(address),
            }
            .into(),
            // TODO: implement draw?
            ast::Instruction::Draw {
                op,
                x,
                y,
                p1,
                p2,
                p3,
                p4,
            } => Draw {
                op,
                p1: lvar(x),
                p2: lvar(y),
                p3: lvar(p1),
                p4: lvar(p2),
                p5: lvar(p3),
                p6: lvar(p4),
            }
            .into(),
            ast::Instruction::Print { value } => Print { value: lvar(value) }.into(),
            ast::Instruction::PrintChar { value } => PrintChar { value: lvar(value) }.into(),
            ast::Instruction::Format { value } => Format { value: lvar(value) }.into(),

            // block control
            ast::Instruction::DrawFlush { target } => DrawFlush {
                target: lvar(target),
            }
            .into(),
            ast::Instruction::PrintFlush { target } => PrintFlush {
                target: lvar(target),
            }
            .into(),
            ast::Instruction::GetLink { result, index } => GetLink {
                result: lvar(result),
                index: lvar(index),
            }
            .into(),
            ast::Instruction::Control {
                control,
                target,
                p1,
                p2,
                p3,
            } => Control {
                control,
                target: lvar(target),
                p1: lvar(p1),
                p2: lvar(p2),
                p3: lvar(p3),
            }
            .into(),
            ast::Instruction::Sensor {
                result,
                target,
                sensor,
            } => Sensor {
                result: lvar(result),
                target: lvar(target),
                sensor: lvar(sensor),
            }
            .into(),

            // operations
            ast::Instruction::Set { to, from } => Set {
                to: lvar(to),
                from: lvar(from),
            }
            .into(),
            ast::Instruction::Op { op, result, x, y } => Op {
                op,
                result: lvar(result),
                x: lvar(x),
                y: lvar(y),
            }
            .into(),
            ast::Instruction::Select {
                result,
                op,
                x,
                y,
                if_true,
                if_false,
            } => Select {
                result: lvar(result),
                op,
                x: lvar(x),
                y: lvar(y),
                if_true: lvar(if_true),
                if_false: lvar(if_false),
            }
            .into(),
            ast::Instruction::Lookup {
                content_type,
                result,
                id,
            } => Lookup {
                content_type,
                result: lvar(result),
                id: lvar(id),
            }
            .into(),
            ast::Instruction::PackColor { result, r, g, b, a } => PackColor {
                result: lvar(result),
                r: lvar(r),
                g: lvar(g),
                b: lvar(b),
                a: lvar(a),
            }
            .into(),
            ast::Instruction::UnpackColor { r, g, b, a, value } => UnpackColor {
                r: lvar(r),
                g: lvar(g),
                b: lvar(b),
                a: lvar(a),
                value: lvar(value),
            }
            .into(),

            // flow control
            ast::Instruction::Noop => Noop.into(),
            ast::Instruction::Wait { value } => Wait { value: lvar(value) }.into(),
            ast::Instruction::Stop => Stop.into(),
            ast::Instruction::End => End.into(),
            ast::Instruction::Jump { target, op, x, y } => Jump {
                target: jump_target(target)?,
                op,
                x: lvar(x),
                y: lvar(y),
            }
            .into(),

            // unknown
            // do this here so it isn't ignored for unprivileged procs
            ast::Instruction::Unknown(name) => {
                return Err(VMLoadError::BadProcessorCode(format!(
                    "unknown instruction: {name}"
                )));
            }

            // convert privileged instructions to noops if the proc is unprivileged
            _ if !privileged => Noop.into(),

            // privileged
            ast::Instruction::GetBlock {
                layer,
                result,
                x,
                y,
            } => GetBlock {
                layer,
                result: lvar(result),
                x: lvar(x),
                y: lvar(y),
            }
            .into(),
            ast::Instruction::SetRate { value } => SetRate { value: lvar(value) }.into(),
        })
    }
}

impl InstructionTrait for InstructionBuilder {
    fn execute(&self, _: &mut ProcessorState, _: &LogicVM) -> InstructionResult {
        unreachable!("InstructionBuilder should always be replaced during late init")
    }
}

// input/output

#[derive(Debug)]
#[non_exhaustive]
pub struct Read {
    pub result: LVar,
    pub target: LVar,
    pub address: LVar,
}

impl Read {
    #[inline(always)]
    fn read_slice<T>(slice: &[T], address: &LValue) -> f64
    where
        T: Copy + AsPrimitive<f64>,
    {
        address
            .num_usize()
            .ok()
            .and_then(|i| slice.get(i))
            .copied()
            .map_or(f64::NAN, AsPrimitive::as_)
    }
}

impl SimpleInstructionTrait for Read {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) {
        let address = self.address.get(state);
        let target = self.target.get(state);

        match target.obj() {
            Some(LObject::Building(building)) => borrow_data!(
                mut building.data,
                state: target_state => {
                    // read variable with name, returning null for constants and undefined
                    // or no-op if the address is not a string
                    if let Some(LObject::String(name)) = address.obj() {
                        // @counter should never be in state.variables, since globals are checked first
                        if **name != u16str!("@counter") {
                            self.result.set(
                                state,
                                target_state
                                    .variables
                                    .get(&**name)
                                    .cloned()
                                    .unwrap_or(LValue::NULL),
                            );
                        } else {
                            // SAFETY: usize as f64 should always be finite
                            unsafe {
                                self.result
                                    .setnum_unchecked(state, target_state.counter as f64);
                            }
                        }
                    }
                },
                data => match data {
                    // read value at index
                    BuildingData::Memory(memory) => {
                        // coerce the address to a number, and return null if the address is not in range
                        self.result
                            .setnum(state, Self::read_slice(memory, &address));
                    }

                    // read char at index
                    BuildingData::Message(message) => {
                        self.result
                            .setnum(state, Self::read_slice(message.as_slice(), &address));
                    }

                    BuildingData::Custom(custom) => {
                        if let Some(value) = custom.read(state, vm, address.into_owned()) {
                            self.result.set(state, value);
                        }
                    }

                    // no-op if the target doesn't support reading
                    _ => {}
                },
            ),

            // read char at index
            Some(LObject::String(string)) => {
                self.result
                    .setnum(state, Self::read_slice(string.as_slice(), &address));
            }

            _ => {}
        };
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct Write {
    pub value: LVar,
    pub target: LVar,
    pub address: LVar,
}

impl InstructionTrait for Write {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) -> InstructionResult {
        if let Some(LObject::Building(building)) = self.target.get(state).obj() {
            let address = self.address.get(state);
            let value = self.value.get_inner(state, &state.variables);

            borrow_data!(
                mut building.data,
                state => {
                    if let Some(LObject::String(name)) = address.into_owned().obj() {
                        // @counter should never be in state.variables, since globals are checked first
                        if **name != u16str!("@counter") {
                            let value = value.into_owned();
                            if let Some(var) = state.variables.get_mut(&**name) {
                                *var = value;
                            }
                        } else {
                            ProcessorState::try_set_counter(&mut state.counter, &value);
                            state.set_stopped(false);
                        }
                    }
                },
                data => match data {
                    BuildingData::Memory(memory) => {
                        if let Ok(address) = address.num_usize()
                            && address < memory.len()
                        {
                            memory[address] = value.num();
                        }
                    }

                    BuildingData::Custom(custom) => {
                        return custom.write(state, vm, address.into_owned(), value.into_owned());
                    }

                    _ => {}
                }
            );
        }
        InstructionResult::Ok
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct Draw {
    pub op: DrawOp,
    pub p1: LVar,
    pub p2: LVar,
    pub p3: LVar,
    pub p4: LVar,
    pub p5: LVar,
    pub p6: LVar,
}

impl SimpleInstructionTrait for Draw {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        if state.drawbuffer_len >= MAX_DRAW_BUFFER {
            return;
        }

        let p1 = self.p1.get_inner(state, &state.variables);
        let p2 = self.p2.get_inner(state, &state.variables);
        let p3 = self.p3.get_inner(state, &state.variables);
        let p4 = self.p4.get_inner(state, &state.variables);
        let p5 = self.p5.get_inner(state, &state.variables);
        let p6 = self.p6.get_inner(state, &state.variables);

        let mut size = 1;
        state.drawbuffer.push(match self.op {
            DrawOp::Clear => DrawCommand::Clear {
                r: p1.numi() as u8,
                g: p2.numi() as u8,
                b: p3.numi() as u8,
            },
            DrawOp::Color => DrawCommand::Color {
                r: p1.numi() as u8,
                g: p2.numi() as u8,
                b: p3.numi() as u8,
                a: p4.numi() as u8,
            },
            DrawOp::Col => {
                let (r, g, b, a) = from_double_bits(p1.num());
                DrawCommand::Color { r, g, b, a }
            }
            DrawOp::Stroke => DrawCommand::Stroke {
                width: p1.numi() as i16,
            },
            DrawOp::Line => DrawCommand::Line {
                x1: p1.numi() as i16,
                y1: p2.numi() as i16,
                x2: p3.numi() as i16,
                y2: p4.numi() as i16,
            },
            DrawOp::Rect | DrawOp::LineRect => DrawCommand::Rect {
                x: p1.numi() as i16,
                y: p2.numi() as i16,
                width: p3.numi() as i16,
                height: p4.numi() as i16,
                fill: self.op == DrawOp::Rect,
            },
            DrawOp::Poly | DrawOp::LinePoly => DrawCommand::Poly {
                x: p1.numi() as i16,
                y: p2.numi() as i16,
                sides: p3.numi() as i16,
                radius: p4.numi() as i16,
                rotation: p5.numi() as i16,
                fill: self.op == DrawOp::Poly,
            },
            DrawOp::Triangle => DrawCommand::Triangle {
                x1: p1.numi() as i16,
                y1: p2.numi() as i16,
                x2: p3.numi() as i16,
                y2: p4.numi() as i16,
                x3: p5.numi() as i16,
                y3: p6.numi() as i16,
            },
            DrawOp::Image => DrawCommand::Image {
                x: p1.numi() as i16,
                y: p2.numi() as i16,
                image: match p3.obj() {
                    Some(LObject::Content(content)) => Some(*content),
                    _ => None,
                },
                size: p4.numi() as i16,
                rotation: p5.numi() as i16,
            },
            DrawOp::Print => {
                if state.printbuffer.is_empty() {
                    return;
                }

                // newlines don't count toward the length limit
                size = state
                    .printbuffer
                    .as_slice()
                    .iter()
                    .filter(|v| **v != b'\n' as u16)
                    .count();

                DrawCommand::Print {
                    x: p1.numi() as i16,
                    y: p2.numi() as i16,
                    alignment: TextAlignment::from_bits_truncate(p3.numi() as u8),
                    // FIXME: lazy
                    text: core::mem::take(&mut state.printbuffer),
                }
            }
            DrawOp::Translate => DrawCommand::Translate {
                x: p1.numi() as i16,
                y: p2.numi() as i16,
            },
            DrawOp::Scale => DrawCommand::Scale {
                x: (p1.numf() / DrawCommand::SCALE_STEP) as i16,
                y: (p2.numf() / DrawCommand::SCALE_STEP) as i16,
            },
            DrawOp::Rotate => DrawCommand::Rotate {
                degrees: p1.numi() as i16,
            },
            DrawOp::Reset => DrawCommand::Reset,
        });
        state.drawbuffer_len += size;
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct Print {
    pub value: LVar,
}

impl Print {
    #[inline(always)]
    fn to_string<'a>(value: &'a LValue) -> Cow<'a, U16Str> {
        match value.obj() {
            Some(LObject::Null) => Cow::from(u16str!("null")),
            None => {
                let n = value.num();
                let rounded = n.round() as u64;
                Cow::from(if (n - (rounded as f64)).abs() < PRINT_EPSILON {
                    u16format!("{rounded}")
                } else {
                    u16format!("{n}")
                })
            }
            Some(LObject::String(string)) => Cow::Borrowed(string),
            Some(LObject::Content(content)) => Cow::Borrowed(content.name()),
            Some(LObject::Team(team)) => Cow::from(team.name_u16()),
            Some(LObject::Building(building)) => Cow::Borrowed(building.block.name.as_u16str()),
            Some(LObject::Sensor(sensor)) => Cow::from(sensor.name_u16()),
        }
    }
}

impl SimpleInstructionTrait for Print {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        if state.printbuffer.len() >= MAX_TEXT_BUFFER {
            return;
        }

        let value = self.value.get_inner(state, &state.variables);
        state.printbuffer += Print::to_string(&value).as_ref();
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct PrintChar {
    pub value: LVar,
}

impl SimpleInstructionTrait for PrintChar {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        if state.printbuffer.len() >= MAX_TEXT_BUFFER {
            return;
        }

        // TODO: content emojis
        if let Some(c) = self.value.get(state).try_num() {
            // Java converts from float to char via int, not directly
            state.printbuffer.push_slice([c.floor() as u32 as u16]);
        }
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct Format {
    pub value: LVar,
}

impl SimpleInstructionTrait for Format {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        if state.printbuffer.len() >= MAX_TEXT_BUFFER {
            return;
        }

        let mut placeholder_index = MAX_TEXT_BUFFER;
        let mut placeholder_number = 10;

        for (i, vals) in state.printbuffer.as_vec().windows(3).enumerate() {
            let &[left, c, right] = vals else {
                unreachable!()
            };
            if left == ('{' as u16) && right == ('}' as u16) {
                let n = (c as i32) - ('0' as i32);
                if (0..=9).contains(&n) && n < placeholder_number {
                    placeholder_number = n;
                    placeholder_index = i;
                }
            }
        }

        if placeholder_index == MAX_TEXT_BUFFER {
            return;
        }

        let value = self.value.get_inner(state, &state.variables);
        state.printbuffer.as_mut_vec().splice(
            placeholder_index..placeholder_index + 3,
            // TODO: this feels scuffed
            Print::to_string(&value).into_owned().into_vec(),
        );
    }
}

// block control

#[derive(Debug)]
#[non_exhaustive]
pub struct DrawFlush {
    pub target: LVar,
}

impl InstructionTrait for DrawFlush {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) -> InstructionResult {
        let result = if let Some(LObject::Building(target)) = self.target.get(state).obj()
            && let Ok(mut data) = target.data.clone().try_borrow_mut()
            && let BuildingData::Custom(custom) = &mut *data
        {
            custom.drawflush(state, vm)
        } else {
            InstructionResult::Ok
        };
        state.drawbuffer.clear();
        result
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct PrintFlush {
    pub target: LVar,
}

impl InstructionTrait for PrintFlush {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) -> InstructionResult {
        let result = if let Some(LObject::Building(target)) =
            self.target.get_inner(state, &state.variables).obj()
            && let Ok(mut data) = target.data.clone().try_borrow_mut()
        {
            if state.printbuffer.len() > MAX_TEXT_BUFFER {
                state.printbuffer.drain(MAX_TEXT_BUFFER..);
            }

            match &mut *data {
                BuildingData::Message(message_buffer) => {
                    core::mem::swap(&mut state.printbuffer, message_buffer);
                    InstructionResult::Ok
                }

                BuildingData::Custom(custom) => custom.printflush(state, vm),

                _ => InstructionResult::Ok,
            }
        } else {
            InstructionResult::Ok
        };
        state.printbuffer.clear();
        result
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct GetLink {
    pub result: LVar,
    pub index: LVar,
}

impl SimpleInstructionTrait for GetLink {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        match self.index.get(state).num_usize() {
            // SAFETY: ProcessorLink.building can never become null
            Ok(index) if index < state.links().len() => unsafe {
                self.result
                    .setobj_non_null(state, state.links()[index].building.clone().into());
            },
            _ => self.result.set(state, LValue::NULL),
        };
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct Control {
    pub control: LAccess,
    pub target: LVar,
    pub p1: LVar,
    pub p2: LVar,
    pub p3: LVar,
}

impl InstructionTrait for Control {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) -> InstructionResult {
        if let Some(LObject::Building(building)) = self.target.get(state).obj()
            && (state.privileged() || state.linked_positions().contains(&building.position))
        {
            borrow_data!(
                mut building.data,
                state => if self.control == LAccess::Enabled {
                    let enabled = self.p1.get(state);
                    if enabled.isnum() {
                        state.set_enabled(enabled.numf() != 0.);
                    }
                },
                data => match data {
                    BuildingData::Switch(value) if self.control == LAccess::Enabled => {
                        let enabled = self.p1.get(state);
                        if enabled.isnum() {
                            *value = enabled.numf() != 0.;
                        }
                    }

                    BuildingData::Custom(custom) => {
                        return custom.control(state, vm, self.control, &self.p1, &self.p2, &self.p3);
                    }

                    _ => {}
                },
            );
        }
        InstructionResult::Ok
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct Sensor {
    pub result: LVar,
    pub target: LVar,
    pub sensor: LVar,
}

impl SimpleInstructionTrait for Sensor {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) {
        use LAccess::*;

        let target = self.target.get(state);
        let sensor = self.sensor.get(state);

        macro_rules! setnull {
            () => {{
                self.result.set(state, LValue::NULL);
                return;
            }};
        }

        macro_rules! setobj {
            (LObject::Null) => {
                setnull!()
            };
            ($value:expr) => {
                // SAFETY: we use setnull for setting null values
                unsafe {
                    self.result.setobj_non_null(state, $value.into());
                    return;
                }
            };
        }

        let result = match sensor.obj() {
            // normal sensors
            &Some(LObject::Sensor(sensor)) => match target.obj() {
                // dead
                Some(LObject::Null) if sensor == Dead => true.into(),

                // senseable
                Some(LObject::Content(content)) => match content {
                    // TODO: color, health, maxHealth, solid, powerCapacity
                    Content::Block(block) => match sensor {
                        Name => setobj!(LString::Static(block.name.as_u16str())),
                        Size => block.size as f64,
                        ItemCapacity => block.item_capacity as f64,
                        LiquidCapacity => block.liquid_capacity as f64,
                        Id => block.logic_id as f64,
                        _ => setnull!(),
                    },

                    // TODO: color
                    Content::Item(item) => match sensor {
                        Name => setobj!(LString::Static(item.name.as_u16str())),
                        Id => item.logic_id as f64,
                        _ => setnull!(),
                    },

                    // TODO: color
                    Content::Liquid(liquid) => match sensor {
                        Name => setobj!(LString::Static(liquid.name.as_u16str())),
                        Id => liquid.logic_id as f64,
                        _ => setnull!(),
                    },

                    // TODO: health, maxHealth, size, itemCapacity, speed, payloadCapacity
                    Content::Unit(unit) => match sensor {
                        Name => setobj!(LString::Static(unit.name.as_u16str())),
                        Id => unit.logic_id as f64,
                        _ => setnull!(),
                    },
                },

                Some(LObject::Team(team)) => match sensor {
                    Name => setobj!(LString::Static(team.name_u16())),
                    Id => team.0 as f64,
                    Color => team.color(),
                    _ => setnull!(),
                },

                // TODO: solid, health, maxHealth, powerCapacity
                Some(LObject::Building(building)) => match sensor {
                    X => building.position.x as f64,
                    Y => building.position.y as f64,
                    Color => colors::TEAM_SHARDED_F64,
                    Dead => false.into(),
                    Team => crate::types::Team::SHARDED.0 as f64,
                    Efficiency => 1.,
                    Timescale => 1.,
                    Range => building.block.range,
                    Rotation => 0.,
                    TotalItems | TotalLiquids | TotalPower => 0.,
                    ItemCapacity => building.block.item_capacity as f64,
                    LiquidCapacity => building.block.liquid_capacity as f64,
                    PowerNetIn | PowerNetOut | PowerNetStored | PowerNetCapacity => 0.,
                    Controlled => false.into(),
                    PayloadCount => 0.,
                    Size => building.block.size as f64,
                    CameraX | CameraY | CameraWidth | CameraHeight => 0.,
                    Type => setobj!(Content::Block(building.block)),
                    FirstItem => setnull!(),
                    PayloadType => setnull!(),

                    _ => borrow_data!(
                        mut building.data,
                        state => match sensor {
                            LAccess::Enabled => state.enabled().into(),
                            _ => setnull!(),
                        },
                        data => match data {
                            BuildingData::Memory(memory) => match sensor {
                                MemoryCapacity => memory.len() as f64,
                                Enabled => true.into(),
                                _ => setnull!(),
                            },

                            BuildingData::Message(buf) => match sensor {
                                BufferSize => buf.len() as f64,
                                Enabled => true.into(),
                                _ => setnull!(),
                            },

                            BuildingData::Switch(enabled) => match sensor {
                                Enabled => (*enabled).into(),
                                _ => setnull!(),
                            },

                            BuildingData::Unknown {
                                senseable_config, ..
                            } => match sensor {
                                Config => match senseable_config {
                                    Some(value) => {
                                        self.result.set(state, value.clone());
                                        return;
                                    }
                                    None => setnull!(),
                                },
                                Enabled => true.into(),
                                _ => setnull!(),
                            },

                            BuildingData::Custom(custom) => {
                                match custom.sensor(state, vm, sensor) {
                                    Some(value) => {
                                        self.result.set(state, value);
                                        return;
                                    }
                                    None if sensor == Enabled => true.into(),
                                    None => setnull!(),
                                }
                            },

                            BuildingData::Processor(_) => unreachable!(),
                        },
                    ),
                },

                // string length
                Some(LObject::String(string)) if matches!(sensor, BufferSize | Size) => {
                    string.len() as f64
                }

                _ => setnull!(),
            },

            // if target doesn't implement Senseable, write null
            _ if !matches!(
                target.obj(),
                Some(LObject::Content(_) | LObject::Team(_) | LObject::Building(_))
            ) =>
            {
                setnull!()
            }

            // items/liquids aren't implemented, so always write null if sensing content
            Some(LObject::Content(_)) => setnull!(),

            // if target is Senseable and sensor isn't Content or LAccess, do not write to result
            _ => return,
        };

        self.result.setnum(state, result);
    }
}

// operations

#[derive(Debug)]
#[non_exhaustive]
pub struct Set {
    pub to: LVar,
    pub from: LVar,
}

impl SimpleInstructionTrait for Set {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        self.to.set_from(state, &self.from);
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct Op {
    pub op: LogicOp,
    pub result: LVar,
    pub x: LVar,
    pub y: LVar,
}

impl SimpleInstructionTrait for Op {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        let x_val = self.x.get(state);
        let x = x_val.num();

        // TODO: this seems inefficient for unary and condition ops
        let y_val = self.y.get(state);
        let y = y_val.num();

        fn wrap_angle(a: f32) -> f32 {
            if a < 0. { a + 360. } else { a }
        }

        #[cfg(feature = "std")]
        macro_rules! libm {
            ($std:expr, $no_std:expr) => {
                $std
            };
        }

        #[cfg(all(not(feature = "std"), feature = "no_std"))]
        macro_rules! libm {
            ($std:expr, $no_std:expr) => {
                $no_std
            };
        }

        let result = match self.op {
            LogicOp::Add => x + y,
            LogicOp::Sub => x - y,
            LogicOp::Mul => x * y,
            LogicOp::Div => x / y,
            LogicOp::Idiv => (x / y).floor(),
            LogicOp::Mod => x % y,
            LogicOp::Emod => ((x % y) + y) % y,
            LogicOp::Pow => libm!(f64::powf, libm::pow)(x, y),

            LogicOp::Equal => Jump::weak_equal(x_val, y_val).into(),
            LogicOp::NotEqual => (!Jump::weak_equal(x_val, y_val)).into(),
            LogicOp::LessThan => (x < y).into(),
            LogicOp::LessThanEq => (x <= y).into(),
            LogicOp::GreaterThan => (x > y).into(),
            LogicOp::GreaterThanEq => (x >= y).into(),
            LogicOp::StrictEqual => (x_val == y_val).into(),

            LogicOp::Land => (x != 0. && y != 0.).into(),
            LogicOp::Shl => (x as i64).wrapping_shl(y as i64 as u32) as f64,
            LogicOp::Shr => (x as i64).wrapping_shr(y as i64 as u32) as f64,
            LogicOp::Ushr => ((x as i64 as u64).wrapping_shr(y as i64 as u32)) as i64 as f64,
            LogicOp::Or => ((x as i64) | (y as i64)) as f64,
            LogicOp::And => ((x as i64) & (y as i64)) as f64,
            LogicOp::Xor => ((x as i64) ^ (y as i64)) as f64,
            LogicOp::Not => (!(x as i64)) as f64,

            LogicOp::Max => x.max(y),
            LogicOp::Min => x.min(y),
            LogicOp::Angle => {
                wrap_angle(libm!(f32::atan2, libm::atan2f)(y as f32, x as f32) * RAD_DEG) as f64
            }
            LogicOp::AngleDiff => {
                let x = (x as f32) % 360.;
                let y = (y as f32) % 360.;
                f32::min(wrap_angle(x - y), wrap_angle(y - x)) as f64
            }
            LogicOp::Len => {
                let x = x as f32;
                let y = y as f32;
                libm!(f32::sqrt, libm::sqrtf)(x * x + y * y) as f64
            }
            LogicOp::Abs => x.abs(),
            // https://github.com/rust-lang/rust/issues/57543
            LogicOp::Sign => {
                if x == 0. {
                    0.
                } else {
                    x.signum()
                }
            }
            LogicOp::Log => libm!(f64::ln, libm::log)(x),
            LogicOp::Logn => libm!(x.log(y), libm::log(x) / libm::log(y)),
            LogicOp::Log10 => libm!(f64::log10, libm::log10)(x),
            LogicOp::Floor => x.floor(),
            LogicOp::Ceil => x.ceil(),
            // java's Math.round rounds toward +inf, but rust's f64::round rounds away from 0
            LogicOp::Round => (x + 0.5).floor(),
            LogicOp::Sqrt => libm!(f64::sqrt, libm::sqrt)(x),

            #[cfg(feature = "std")]
            LogicOp::Noise => SIMPLEX.get([x, y]),
            #[cfg(not(feature = "std"))]
            LogicOp::Noise => 0., // TODO: implement

            #[cfg(feature = "std")]
            LogicOp::Rand => rand::random::<f64>() * x,
            #[cfg(not(feature = "std"))]
            LogicOp::Rand => x, // TODO: implement

            LogicOp::Sin => libm!(f64::sin, libm::sin)(x * F64_DEG_RAD),
            LogicOp::Cos => libm!(f64::cos, libm::cos)(x * F64_DEG_RAD),
            LogicOp::Tan => libm!(f64::tan, libm::tan)(x * F64_DEG_RAD),

            LogicOp::Asin => libm!(f64::asin, libm::asin)(x) * F64_RAD_DEG,
            LogicOp::Acos => libm!(f64::acos, libm::acos)(x) * F64_RAD_DEG,
            LogicOp::Atan => libm!(f64::atan, libm::atan)(x) * F64_RAD_DEG,
        };

        self.result.setnum(state, result);
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct Select {
    pub result: LVar,
    pub op: ConditionOp,
    pub x: LVar,
    pub y: LVar,
    pub if_true: LVar,
    pub if_false: LVar,
}

impl SimpleInstructionTrait for Select {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        let result = if Jump::test(self.op, &self.x, &self.y, state) {
            &self.if_true
        } else {
            &self.if_false
        };
        self.result.set_from(state, result);
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct Lookup {
    pub content_type: ContentType,
    pub result: LVar,
    pub id: LVar,
}

impl SimpleInstructionTrait for Lookup {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        let id = self.id.get(state).numi();

        let result = match self.content_type {
            ContentType::Block => content::blocks::FROM_LOGIC_ID
                .get(&id)
                .map(|v| Content::Block(v))
                .into(),

            ContentType::Item => content::items::FROM_LOGIC_ID
                .get(&id)
                .map(|v| Content::Item(v))
                .into(),

            ContentType::Liquid => content::liquids::FROM_LOGIC_ID
                .get(&id)
                .map(|v| Content::Liquid(v))
                .into(),

            ContentType::Unit => content::units::FROM_LOGIC_ID
                .get(&id)
                .map(|v| Content::Unit(v))
                .into(),

            ContentType::Team => id.try_into().ok().map(Team).into(),

            _ => LObject::Null,
        };

        self.result.setobj(state, result);
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct PackColor {
    pub result: LVar,
    pub r: LVar,
    pub g: LVar,
    pub b: LVar,
    pub a: LVar,
}

impl SimpleInstructionTrait for PackColor {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        let value = f32_to_double_bits(
            self.r.get(state).numf().clamp(0., 1.),
            self.g.get(state).numf().clamp(0., 1.),
            self.b.get(state).numf().clamp(0., 1.),
            self.a.get(state).numf().clamp(0., 1.),
        );
        // SAFETY: f32_to_double_bits always returns a finite value
        unsafe { self.result.setnum_unchecked(state, value) };
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct UnpackColor {
    pub r: LVar,
    pub g: LVar,
    pub b: LVar,
    pub a: LVar,
    pub value: LVar,
}

impl SimpleInstructionTrait for UnpackColor {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        let (r, g, b, a) = f64_from_double_bits(self.value.get(state).num());
        // SAFETY: f64_from_double_bits always returns finite values
        unsafe {
            self.r.setnum_unchecked(state, r);
            self.g.setnum_unchecked(state, g);
            self.b.setnum_unchecked(state, b);
            self.a.setnum_unchecked(state, a);
        }
    }
}

// flow control

#[derive(Debug)]
#[non_exhaustive]
pub struct Noop;

impl SimpleInstructionTrait for Noop {
    fn execute(&self, _: &mut ProcessorState, _: &LogicVM) {}
}

#[derive(Debug)]
#[non_exhaustive]
pub struct Wait {
    pub value: LVar,
}

impl InstructionTrait for Wait {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) -> InstructionResult {
        let wait_ms = self.value.get(state).num() * 1000.;
        if wait_ms <= 0. {
            InstructionResult::Ok
        } else {
            state.wait_end_time = state.time.get() + wait_ms;
            InstructionResult::Yield
        }
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct Stop;

impl InstructionTrait for Stop {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) -> InstructionResult {
        state.counter -= 1;
        state.set_stopped(true);
        InstructionResult::Yield
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct End;

impl SimpleInstructionTrait for End {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        state.counter = state.num_instructions();
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct Jump {
    pub target: usize,
    pub op: ConditionOp,
    pub x: LVar,
    pub y: LVar,
}

impl Jump {
    #[inline(always)]
    fn test(op: ConditionOp, x: &LVar, y: &LVar, state: &mut ProcessorState) -> bool {
        if matches!(op, ConditionOp::Always) {
            return true;
        }

        let x = x.get(state);
        let y = y.get(state);

        match op {
            ConditionOp::Equal => Self::weak_equal(x, y),
            ConditionOp::NotEqual => !Self::weak_equal(x, y),
            ConditionOp::LessThan => x.num() < y.num(),
            ConditionOp::LessThanEq => x.num() <= y.num(),
            ConditionOp::GreaterThan => x.num() > y.num(),
            ConditionOp::GreaterThanEq => x.num() >= y.num(),
            ConditionOp::StrictEqual => x == y,
            ConditionOp::Always => unreachable!(),
        }
    }

    #[inline(always)]
    fn weak_equal(x: Cow<'_, LValue>, y: Cow<'_, LValue>) -> bool {
        if x.isobj() && y.isobj() {
            x == y
        } else {
            (x.num() - y.num()).abs() < EQUALITY_EPSILON
        }
    }
}

impl SimpleInstructionTrait for Jump {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        if Self::test(self.op, &self.x, &self.y, state) {
            // we do the bounds check while parsing
            state.counter = self.target;
        }
    }
}

// privileged

#[derive(Debug)]
#[non_exhaustive]
pub struct GetBlock {
    pub layer: TileLayer,
    pub result: LVar,
    pub x: LVar,
    pub y: LVar,
}

impl SimpleInstructionTrait for GetBlock {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) {
        let result = match vm.building(PackedPoint2 {
            x: self.x.get(state).numf().round() as i16,
            y: self.y.get(state).numf().round() as i16,
        }) {
            Some(building) => match self.layer {
                TileLayer::Floor => Content::Block(&content::blocks::STONE).into(),
                TileLayer::Ore => Content::Block(&content::blocks::AIR).into(),
                TileLayer::Block => Content::Block(building.block).into(),
                TileLayer::Building => building.clone().into(),
            },
            None => LObject::Null,
        };
        self.result.setobj(state, result);
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct SetRate {
    pub value: LVar,
}

impl SimpleInstructionTrait for SetRate {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        state.ipt = self.value.get(state).numi().clamp(1, MAX_IPT) as f64;
    }
}
