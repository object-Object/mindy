use std::{borrow::Cow, collections::HashMap, rc::Rc};

use enum_dispatch::enum_dispatch;
use lazy_static::lazy_static;
use noise::{NoiseFn, Simplex};
use widestring::{U16Str, u16str};

use super::{
    Constants, LogicVM, VMLoadError, VMLoadResult,
    buildings::BuildingData,
    processor::{MAX_TEXT_BUFFER, ProcessorState},
    variables::{Content, LValue, LVar, RAD_DEG},
};
use crate::{
    logic::{
        ast::{self, ConditionOp, LogicOp, TileLayer},
        vm::variables::{F64_DEG_RAD, F64_RAD_DEG, LString},
    },
    types::{
        ContentType, LAccess, Point2, Team,
        colors::{self, f32_to_double_bits, f64_from_double_bits},
        content,
    },
    utils::u16format,
};

const MAX_IPT: i32 = 1000;
const EQUALITY_EPSILON: f64 = 0.000001;
const PRINT_EPSILON: f64 = 0.00001;

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
pub(super) enum InstructionResult {
    Ok,
    Yield,
}

#[allow(clippy::enum_variant_names)]
#[enum_dispatch(InstructionTrait)]
pub(super) enum Instruction {
    InstructionBuilder,
    // input/output
    Read,
    Write,
    Print,
    PrintChar,
    Format,
    // block control
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

pub(super) struct InstructionBuilder {
    pub(super) instruction: ast::Instruction,
    pub(super) labels: Rc<HashMap<String, usize>>,
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
                    .or_else(|| variables.get_index_of(&name).map(LVar::Variable))
                    // if none of those exist, create a new variable defaulting to null
                    .unwrap_or_else(|| {
                        let (i, _) = variables.insert_full(name, LValue::Null);
                        LVar::Variable(i)
                    })
            }
            ast::Value::String(value) => LVar::Constant(value.into()),
            ast::Value::Number(value) => LVar::Constant(value.into()),
            ast::Value::None => LVar::Constant(LValue::Null),
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
                op: _,
                x,
                y,
                p1,
                p2,
                p3,
                p4,
            } => {
                lvar(x);
                lvar(y);
                lvar(p1);
                lvar(p2);
                lvar(p3);
                lvar(p4);
                Noop.into()
            }
            ast::Instruction::Print { value } => Print { value: lvar(value) }.into(),
            ast::Instruction::PrintChar { value } => PrintChar { value: lvar(value) }.into(),
            ast::Instruction::Format { value } => Format { value: lvar(value) }.into(),

            // block control
            ast::Instruction::DrawFlush { target } => {
                lvar(target);
                Noop.into()
            }
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
            } => {
                lvar(p2);
                lvar(p3);
                Control {
                    control,
                    target: lvar(target),
                    p1: lvar(p1),
                }
                .into()
            }
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

pub(super) struct Read {
    result: LVar,
    target: LVar,
    address: LVar,
}

impl Read {
    fn read_slice<T>(slice: &[T], address: &LValue) -> LValue
    where
        T: Copy,
        LValue: From<Option<T>>,
    {
        address
            .num_usize()
            .ok()
            .and_then(|i| slice.get(i))
            .copied()
            .into()
    }
}

impl SimpleInstructionTrait for Read {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) {
        let address = self.address.get(state);

        let result = match self.target.get(state) {
            LValue::Building(position) => match vm.building(position) {
                Some(building) => building.borrow_data(
                    state,
                    |state| match address.clone() {
                        // read variable with name, returning null for constants and undefined
                        LValue::String(name) => {
                            if *name == u16str!("@counter") {
                                Some(state.counter.into())
                            } else {
                                state.variables.get(&*name).cloned().or(Some(LValue::Null))
                            }
                        }

                        // no-op if the address is not a string
                        _ => None,
                    },
                    |data| match data {
                        // read value at index
                        BuildingData::Memory(memory) => {
                            // coerce the address to a number, and return null if the address is not in range
                            Some(Self::read_slice(memory, &address))
                        }

                        // read char at index
                        BuildingData::Message(message) => {
                            Some(Self::read_slice(message.as_slice(), &address))
                        }

                        // no-op if the target doesn't support reading
                        _ => None,
                    },
                ),
                None => None,
            },

            // read char at index
            LValue::String(string) => Some(Self::read_slice(string.as_slice(), &address)),

            _ => None,
        };

        if let Some(result) = result {
            self.result.set(state, result);
        }
    }
}

pub(super) struct Write {
    value: LVar,
    target: LVar,
    address: LVar,
}

impl SimpleInstructionTrait for Write {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) {
        if let LValue::Building(position) = self.target.get(state)
            && let Some(building) = vm.building(position)
        {
            let address = self.address.get(state);
            let value = self.value.get(state);

            building.borrow_data_mut(
                state,
                |state| match address.clone() {
                    LValue::String(name) if *name == u16str!("@counter") => {
                        state.try_set_counter(value.clone());
                        state.set_stopped(false);
                    }
                    LValue::String(name) if state.variables.contains_key(&*name) => {
                        state.variables[&*name] = value.clone();
                    }
                    _ => {}
                },
                |data| {
                    if let BuildingData::Memory(memory) = data
                        && let Ok(address) = address.num_usize()
                        && address < memory.len()
                    {
                        memory[address] = value.num();
                    }
                },
            );
        }
    }
}

pub(super) struct Print {
    value: LVar,
}

impl Print {
    fn to_string<'a>(value: &'a LValue, vm: &LogicVM) -> Cow<'a, U16Str> {
        match value {
            LValue::Null => Cow::from(u16str!("null")),
            LValue::Number(n) => {
                let rounded = n.round() as u64;
                Cow::from(if (n - (rounded as f64)).abs() < PRINT_EPSILON {
                    u16format!("{rounded}")
                } else {
                    u16format!("{n}")
                })
            }
            LValue::String(string) => Cow::Borrowed(string),
            LValue::Content(content) => Cow::Borrowed(content.name()),
            LValue::Team(team) => Cow::from(team.name_u16()),
            LValue::Building(position) => vm
                .building(*position)
                .map(|b| Cow::Borrowed(b.block.name.as_u16str()))
                .unwrap_or(Cow::from(u16str!("null"))),
            LValue::Sensor(sensor) => Cow::from(sensor.name_u16()),
        }
    }
}

impl SimpleInstructionTrait for Print {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) {
        if state.printbuffer.len() >= MAX_TEXT_BUFFER {
            return;
        }

        let value = self.value.get(state);
        state.printbuffer += Print::to_string(&value, vm).as_ref();
    }
}

pub(super) struct PrintChar {
    value: LVar,
}

impl SimpleInstructionTrait for PrintChar {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        if state.printbuffer.len() >= MAX_TEXT_BUFFER {
            return;
        }

        // TODO: content emojis
        if let LValue::Number(c) = self.value.get(state) {
            // Java converts from float to char via int, not directly
            state.printbuffer.push_slice([c.floor() as u32 as u16]);
        }
    }
}

pub(super) struct Format {
    value: LVar,
}

impl SimpleInstructionTrait for Format {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) {
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

        let value = self.value.get(state);
        state.printbuffer.as_mut_vec().splice(
            placeholder_index..placeholder_index + 3,
            // TODO: this feels scuffed
            Print::to_string(&value, vm).into_owned().into_vec(),
        );
    }
}

// block control

pub(super) struct PrintFlush {
    target: LVar,
}

impl SimpleInstructionTrait for PrintFlush {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) {
        if let LValue::Building(position) = self.target.get(state)
            && let Some(target) = vm.building(position)
            && let Ok(mut data) = target.data.try_borrow_mut()
            && let BuildingData::Message(message_buffer) = &mut *data
        {
            if state.printbuffer.len() > MAX_TEXT_BUFFER {
                state.printbuffer.drain(MAX_TEXT_BUFFER..);
            }
            std::mem::swap(&mut state.printbuffer, message_buffer);
        }
        state.printbuffer.clear();
    }
}

pub(super) struct GetLink {
    result: LVar,
    index: LVar,
}

impl SimpleInstructionTrait for GetLink {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        let result = match self.index.get(state).num_usize() {
            Ok(index) => state.link(index).into(),
            Err(_) => LValue::Null,
        };
        self.result.set(state, result);
    }
}

pub(super) struct Control {
    control: LAccess,
    target: LVar,
    p1: LVar,
}

impl SimpleInstructionTrait for Control {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) {
        if self.control == LAccess::Enabled
            && let LValue::Building(position) = self.target.get(state)
            && (state.privileged() || state.linked_positions().contains(&position))
            && let Some(building) = vm.building(position)
        {
            let enabled = self.p1.get(state);
            if !enabled.isobj() {
                let enabled = enabled.numf() != 0.;
                building.borrow_data_mut(
                    state,
                    |state| state.set_enabled(enabled),
                    |data| {
                        if let BuildingData::Switch(value) = data {
                            *value = enabled;
                        }
                    },
                );
            }
        }
    }
}

pub(super) struct Sensor {
    result: LVar,
    target: LVar,
    sensor: LVar,
}

impl SimpleInstructionTrait for Sensor {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) {
        use LAccess::*;

        let target = self.target.get(state);
        let sensor = self.sensor.get(state);

        let result = match sensor {
            // normal sensors
            LValue::Sensor(sensor) => match target {
                // dead
                LValue::Null if sensor == Dead => true.into(),

                // senseable
                LValue::Content(content) => match content {
                    // TODO: color, health, maxHealth, solid, powerCapacity
                    Content::Block(block) => match sensor {
                        Size => block.size.into(),
                        ItemCapacity => block.item_capacity.into(),
                        LiquidCapacity => block.liquid_capacity.into(),
                        Id => block.logic_id.into(),
                        Name => LString::Static(block.name.as_u16str()).into(),
                        _ => LValue::Null,
                    },

                    // TODO: color
                    Content::Item(item) => match sensor {
                        Id => item.logic_id.into(),
                        Name => LString::Static(item.name.as_u16str()).into(),
                        _ => LValue::Null,
                    },

                    // TODO: color
                    Content::Liquid(liquid) => match sensor {
                        Id => liquid.logic_id.into(),
                        Name => LString::Static(liquid.name.as_u16str()).into(),
                        _ => LValue::Null,
                    },

                    // TODO: health, maxHealth, size, itemCapacity, speed, payloadCapacity
                    Content::Unit(unit) => match sensor {
                        Id => unit.logic_id.into(),
                        Name => LString::Static(unit.name.as_u16str()).into(),
                        _ => LValue::Null,
                    },
                },

                LValue::Team(team) => match sensor {
                    Name => LString::Static(team.name_u16()).into(),
                    Id => team.0.into(),
                    Color => team.color().into(),
                    _ => LValue::Null,
                },

                LValue::Building(position) => match vm.building(position) {
                    // TODO: solid, health, maxHealth, powerCapacity
                    Some(building) => match sensor {
                        X => building.position.x.into(),
                        Y => building.position.y.into(),
                        Color => colors::TEAM_SHARDED.into(),
                        Dead => false.into(),
                        Team => crate::types::Team::SHARDED.0.into(),
                        Efficiency => 1.into(),
                        Timescale => 1.into(),
                        Range => building.block.range.into(),
                        Rotation => 0.into(),
                        TotalItems | TotalLiquids | TotalPower => 0.into(),
                        ItemCapacity => building.block.item_capacity.into(),
                        LiquidCapacity => building.block.liquid_capacity.into(),
                        PowerNetIn | PowerNetOut | PowerNetStored | PowerNetCapacity => 0.into(),
                        Controlled => false.into(),
                        PayloadCount => 0.into(),
                        Size => building.block.size.into(),
                        CameraX | CameraY | CameraWidth | CameraHeight => 0.into(),
                        Type => Content::Block(building.block).into(),
                        FirstItem => LValue::Null,
                        PayloadType => LValue::Null,

                        _ => building.borrow_data(
                            state,
                            |state| match sensor {
                                LAccess::Enabled => state.enabled().into(),
                                _ => LValue::Null,
                            },
                            |data| match data {
                                BuildingData::Memory(memory) => match sensor {
                                    MemoryCapacity => memory.len().into(),
                                    Enabled => true.into(),
                                    _ => LValue::Null,
                                },

                                BuildingData::Message(buf) => match sensor {
                                    BufferSize => buf.len().into(),
                                    Enabled => true.into(),
                                    _ => LValue::Null,
                                },

                                BuildingData::Switch(enabled) => match sensor {
                                    Enabled => (*enabled).into(),
                                    _ => LValue::Null,
                                },

                                BuildingData::Unknown {
                                    senseable_config, ..
                                } => match sensor {
                                    Config => senseable_config.clone().unwrap_or(LValue::Null),
                                    Enabled => true.into(),
                                    _ => LValue::Null,
                                },

                                BuildingData::Processor(_) => unreachable!(),
                            },
                        ),
                    },
                    None => LValue::Null,
                },

                // string length
                LValue::String(string) if matches!(sensor, BufferSize | Size) => {
                    string.len().into()
                }

                _ => LValue::Null,
            },

            // if target doesn't implement Senseable, write null
            _ if !matches!(
                target,
                LValue::Content(_) | LValue::Team(_) | LValue::Building(_)
            ) =>
            {
                LValue::Null
            }

            // items/liquids aren't implemented, so always write null if sensing content
            LValue::Content(_) => LValue::Null,

            // if target is Senseable and sensor isn't Content or LAccess, do not write to result
            _ => return,
        };

        self.result.set(state, result);
    }
}

// operations

pub(super) struct Set {
    to: LVar,
    from: LVar,
}

impl SimpleInstructionTrait for Set {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        self.to.set_from(state, &self.from);
    }
}

pub(super) struct Op {
    op: LogicOp,
    result: LVar,
    x: LVar,
    y: LVar,
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

        let result = match self.op {
            LogicOp::Add => (x + y).into(),
            LogicOp::Sub => (x - y).into(),
            LogicOp::Mul => (x * y).into(),
            LogicOp::Div => (x / y).into(),
            LogicOp::Idiv => (x / y).floor().into(),
            LogicOp::Mod => (x % y).into(),
            LogicOp::Emod => (((x % y) + y) % y).into(),
            LogicOp::Pow => x.powf(y).into(),

            LogicOp::Equal => Jump::weak_equal(x_val, y_val).into(),
            LogicOp::NotEqual => (!Jump::weak_equal(x_val, y_val)).into(),
            LogicOp::LessThan => (x < y).into(),
            LogicOp::LessThanEq => (x <= y).into(),
            LogicOp::GreaterThan => (x > y).into(),
            LogicOp::GreaterThanEq => (x >= y).into(),
            LogicOp::StrictEqual => (x_val == y_val).into(),

            LogicOp::Land => (x != 0. && y != 0.).into(),
            LogicOp::Shl => ((x as i64).wrapping_shl(y as i64 as u32)).into(),
            LogicOp::Shr => ((x as i64).wrapping_shr(y as i64 as u32)).into(),
            LogicOp::Ushr => (((x as i64 as u64).wrapping_shr(y as i64 as u32)) as i64).into(),
            LogicOp::Or => ((x as i64) | (y as i64)).into(),
            LogicOp::And => ((x as i64) & (y as i64)).into(),
            LogicOp::Xor => ((x as i64) ^ (y as i64)).into(),
            LogicOp::Not => (!(x as i64)).into(),

            LogicOp::Max => x.max(y).into(),
            LogicOp::Min => x.min(y).into(),
            LogicOp::Angle => wrap_angle((y as f32).atan2(x as f32) * RAD_DEG).into(),
            LogicOp::AngleDiff => {
                let x = (x as f32) % 360.;
                let y = (y as f32) % 360.;
                f32::min(wrap_angle(x - y), wrap_angle(y - x)).into()
            }
            LogicOp::Len => {
                let x = x as f32;
                let y = y as f32;
                (x * x + y * y).sqrt().into()
            }
            LogicOp::Noise => SIMPLEX.get([x, y]).into(),
            LogicOp::Abs => x.abs().into(),
            // https://github.com/rust-lang/rust/issues/57543
            LogicOp::Sign => (if x == 0. { 0. } else { x.signum() }).into(),
            LogicOp::Log => x.ln().into(),
            LogicOp::Logn => x.log(y).into(),
            LogicOp::Log10 => x.log10().into(),
            LogicOp::Floor => x.floor().into(),
            LogicOp::Ceil => x.ceil().into(),
            // java's Math.round rounds toward +inf, but rust's f64::round rounds away from 0
            LogicOp::Round => (x + 0.5).floor().into(),
            LogicOp::Sqrt => x.sqrt().into(),
            LogicOp::Rand => (rand::random::<f64>() * x).into(),

            LogicOp::Sin => (x * F64_DEG_RAD).sin().into(),
            LogicOp::Cos => (x * F64_DEG_RAD).cos().into(),
            LogicOp::Tan => (x * F64_DEG_RAD).tan().into(),

            LogicOp::Asin => (x.asin() * F64_RAD_DEG).into(),
            LogicOp::Acos => (x.acos() * F64_RAD_DEG).into(),
            LogicOp::Atan => (x.atan() * F64_RAD_DEG).into(),
        };

        self.result.set(state, result);
    }
}

pub(super) struct Select {
    result: LVar,
    op: ConditionOp,
    x: LVar,
    y: LVar,
    if_true: LVar,
    if_false: LVar,
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

pub(super) struct Lookup {
    content_type: ContentType,
    result: LVar,
    id: LVar,
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

            _ => LValue::Null,
        };

        self.result.set(state, result);
    }
}

pub(super) struct PackColor {
    result: LVar,
    r: LVar,
    g: LVar,
    b: LVar,
    a: LVar,
}

impl SimpleInstructionTrait for PackColor {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        self.result.set(
            state,
            f32_to_double_bits(
                self.r.get(state).numf().clamp(0., 1.),
                self.g.get(state).numf().clamp(0., 1.),
                self.b.get(state).numf().clamp(0., 1.),
                self.a.get(state).numf().clamp(0., 1.),
            )
            .into(),
        );
    }
}

pub(super) struct UnpackColor {
    r: LVar,
    g: LVar,
    b: LVar,
    a: LVar,
    value: LVar,
}

impl SimpleInstructionTrait for UnpackColor {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        let (r, g, b, a) = f64_from_double_bits(self.value.get(state).num());
        self.r.set(state, r.into());
        self.g.set(state, g.into());
        self.b.set(state, b.into());
        self.a.set(state, a.into());
    }
}

// flow control

pub(super) struct Noop;

impl SimpleInstructionTrait for Noop {
    fn execute(&self, _: &mut ProcessorState, _: &LogicVM) {}
}

pub(super) struct Wait {
    value: LVar,
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

pub(super) struct Stop;

impl InstructionTrait for Stop {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) -> InstructionResult {
        state.counter -= 1;
        state.set_stopped(true);
        InstructionResult::Yield
    }
}

pub(super) struct End;

impl SimpleInstructionTrait for End {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        state.counter = state.num_instructions();
    }
}

pub(super) struct Jump {
    target: usize,
    op: ConditionOp,
    x: LVar,
    y: LVar,
}

impl Jump {
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

    fn weak_equal(x: LValue, y: LValue) -> bool {
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

pub(super) struct GetBlock {
    layer: TileLayer,
    result: LVar,
    x: LVar,
    y: LVar,
}

impl SimpleInstructionTrait for GetBlock {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) {
        let result = match vm.building(Point2 {
            x: self.x.get(state).numf().round() as i32,
            y: self.y.get(state).numf().round() as i32,
        }) {
            Some(building) => match self.layer {
                TileLayer::Floor => Content::Block(&content::blocks::STONE).into(),
                TileLayer::Ore => Content::Block(&content::blocks::AIR).into(),
                TileLayer::Block => Content::Block(building.block).into(),
                TileLayer::Building => building.position.into(),
            },
            None => LValue::Null,
        };
        self.result.set(state, result);
    }
}

pub(super) struct SetRate {
    value: LVar,
}

impl SimpleInstructionTrait for SetRate {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        state.ipt = self.value.get(state).numi().clamp(1, MAX_IPT) as f64;
    }
}
