use std::{borrow::Cow, collections::HashMap, rc::Rc};

use lazy_static::lazy_static;
use noise::{NoiseFn, Simplex};

use super::{
    LogicVM, VMLoadError, VMLoadResult,
    buildings::BuildingData,
    processor::{MAX_TEXT_BUFFER, ProcessorState},
    variables::{Content, LValue, LVar, RAD_DEG},
};
use crate::{
    logic::{
        ast::{self, ConditionOp, LogicOp, TileLayer},
        vm::variables::{F64_DEG_RAD, F64_RAD_DEG},
    },
    types::{
        ContentType, Point2, Team,
        colors::{f32_to_double_bits, f64_from_double_bits},
        content,
    },
};

const MAX_IPT: usize = 1000;
const EQUALITY_EPSILON: f64 = 0.000001;
const PRINT_EPSILON: f64 = 0.00001;

lazy_static! {
    static ref SIMPLEX: Simplex = Simplex::new(0);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionResult {
    Ok,
    Yield,
}

pub trait Instruction {
    fn late_init(
        self: Box<Self>,
        _variables: &mut HashMap<String, LVar>,
        _globals: &HashMap<String, LVar>,
        _privileged: bool,
        _num_instructions: usize,
    ) -> VMLoadResult<Box<dyn Instruction>> {
        Err(VMLoadError::AlreadyInitialized)
    }

    /// Returns true if more instructions can be executed,
    /// or false if the processor should yield for the rest of this tick.
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) -> InstructionResult;
}

trait SimpleInstruction {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM);
}

impl<T: SimpleInstruction> Instruction for T {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) -> InstructionResult {
        self.execute(state, vm);
        InstructionResult::Ok
    }
}

pub(super) struct InstructionBuilder {
    pub instruction: ast::Instruction,
    pub labels: Rc<HashMap<String, usize>>,
}

impl Instruction for InstructionBuilder {
    fn late_init(
        self: Box<Self>,
        variables: &mut HashMap<String, LVar>,
        globals: &HashMap<String, LVar>,
        privileged: bool,
        num_instructions: usize,
    ) -> VMLoadResult<Box<dyn Instruction>> {
        // helpers

        let mut lvar = |value| match value {
            ast::Value::Variable(name) => {
                if let Some(var) = globals.get(&name).or_else(|| variables.get(&name)) {
                    var.clone()
                } else {
                    let var = LVar::new_variable();
                    variables.insert(name, LVar::clone(&var));
                    var
                }
            }
            ast::Value::String(value) => LVar::Constant(LValue::String(value.into())),
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
            // TODO: implement draw?
            ast::Instruction::Draw { .. } => Box::new(Noop),
            ast::Instruction::Print { value } => Box::new(Print { value: lvar(value) }),
            ast::Instruction::PrintChar { value } => Box::new(PrintChar { value: lvar(value) }),
            ast::Instruction::Format { value } => Box::new(Format { value: lvar(value) }),

            // block control
            ast::Instruction::DrawFlush { .. } => Box::new(Noop),
            ast::Instruction::PrintFlush { target } => Box::new(PrintFlush {
                target: lvar(target),
            }),
            ast::Instruction::GetLink { result, index } => Box::new(GetLink {
                result: lvar(result),
                index: lvar(index),
            }),

            // operations
            ast::Instruction::Set { to, from } => Box::new(Set {
                to: lvar(to),
                from: lvar(from),
            }),
            ast::Instruction::Op { op, result, x, y } => Box::new(Op {
                op,
                result: lvar(result),
                x: lvar(x),
                y: lvar(y),
            }),
            ast::Instruction::Select {
                result,
                op,
                x,
                y,
                if_true,
                if_false,
            } => Box::new(Select {
                result: lvar(result),
                op,
                x: lvar(x),
                y: lvar(y),
                if_true: lvar(if_true),
                if_false: lvar(if_false),
            }),
            ast::Instruction::Lookup {
                content_type,
                result,
                id,
            } => Box::new(Lookup {
                content_type,
                result: lvar(result),
                id: lvar(id),
            }),
            ast::Instruction::PackColor { result, r, g, b, a } => Box::new(PackColor {
                result: lvar(result),
                r: lvar(r),
                g: lvar(g),
                b: lvar(b),
                a: lvar(a),
            }),
            ast::Instruction::UnpackColor { r, g, b, a, value } => Box::new(UnpackColor {
                r: lvar(r),
                g: lvar(g),
                b: lvar(b),
                a: lvar(a),
                value: lvar(value),
            }),

            // flow control
            ast::Instruction::Noop => Box::new(Noop),
            ast::Instruction::Wait { value } => Box::new(Wait { value: lvar(value) }),
            ast::Instruction::Stop => Box::new(Stop),
            ast::Instruction::End => Box::new(End),
            ast::Instruction::Jump { target, op, x, y } => Box::new(Jump {
                target: jump_target(target)?,
                op,
                x: lvar(x),
                y: lvar(y),
            }),

            // unknown
            // do this here so it isn't ignored for unprivileged procs
            ast::Instruction::Unknown(name) => {
                return Err(VMLoadError::BadProcessorCode(format!(
                    "unknown instruction: {name}"
                )));
            }

            // convert privileged instructions to noops if the proc is unprivileged
            _ if !privileged => Box::new(Noop),

            // privileged
            ast::Instruction::GetBlock {
                layer,
                result,
                x,
                y,
            } => Box::new(GetBlock {
                layer,
                result: lvar(result),
                x: lvar(x),
                y: lvar(y),
            }),
            ast::Instruction::SetRate { value } => Box::new(SetRate { value: lvar(value) }),
        })
    }

    fn execute(&self, _: &mut ProcessorState, _: &LogicVM) -> InstructionResult {
        unreachable!("InstructionBuilder should always be replaced during late init")
    }
}

// input/output

struct Print {
    value: LVar,
}

impl Print {
    fn to_string<'a>(value: &'a LValue, vm: &LogicVM) -> Cow<'a, str> {
        match value {
            LValue::Null => Cow::from("null"),
            LValue::Number(n) => {
                let rounded = n.round() as u64;
                Cow::from(if (n - (rounded as f64)).abs() < PRINT_EPSILON {
                    rounded.to_string()
                } else {
                    n.to_string()
                })
            }
            LValue::String(s) => Cow::Borrowed(s),
            LValue::Content(content) => Cow::Borrowed(content.name()),
            LValue::Team(team) => team.name(),
            LValue::Building(position) => vm
                .building(*position)
                .map(|b| Cow::Borrowed(b.block.name.as_str()))
                .unwrap_or(Cow::from("null")),
        }
    }
}

impl SimpleInstruction for Print {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) {
        if state.printbuffer.len() >= MAX_TEXT_BUFFER {
            return;
        }

        let value = self.value.get(state);
        state.append_printbuffer(&Print::to_string(&value, vm));
    }
}

struct PrintChar {
    value: LVar,
}

impl SimpleInstruction for PrintChar {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        if state.printbuffer.len() >= MAX_TEXT_BUFFER {
            return;
        }

        // TODO: content emojis
        if let LValue::Number(c) = self.value.get(state) {
            // Java converts from float to char via int, not directly
            state.printbuffer.push(c.floor() as u32 as u16);
        }
    }
}

struct Format {
    value: LVar,
}

impl SimpleInstruction for Format {
    fn execute(&self, state: &mut ProcessorState, vm: &LogicVM) {
        if state.printbuffer.len() >= MAX_TEXT_BUFFER {
            return;
        }

        let mut placeholder_index = MAX_TEXT_BUFFER;
        let mut placeholder_number = 10;

        for (i, vals) in state.printbuffer.windows(3).enumerate() {
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
        state.printbuffer.splice(
            placeholder_index..placeholder_index + 3,
            ProcessorState::encode_utf16(&Print::to_string(&value, vm)),
        );
    }
}

// block control

struct PrintFlush {
    target: LVar,
}

impl SimpleInstruction for PrintFlush {
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

struct GetLink {
    result: LVar,
    index: LVar,
}

impl SimpleInstruction for GetLink {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        let result = match self.index.get(state).num_usize() {
            Ok(index) => state.link(index).into(),
            Err(_) => LValue::Null,
        };
        self.result.set(state, result);
    }
}

// operations

struct Set {
    to: LVar,
    from: LVar,
}

impl SimpleInstruction for Set {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        self.to.set_from(state, &self.from);
    }
}

struct Op {
    op: LogicOp,
    result: LVar,
    x: LVar,
    y: LVar,
}

impl SimpleInstruction for Op {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        // TODO: this seems inefficient for unary and condition ops
        let x = self.x.get(state).num();
        let y = self.y.get(state).num();

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

            LogicOp::Land => (x != 0. && y != 0.).into(),
            LogicOp::Condition(op) => Jump::test(op, &self.x, &self.y, state).into(),

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

struct Select {
    result: LVar,
    op: ConditionOp,
    x: LVar,
    y: LVar,
    if_true: LVar,
    if_false: LVar,
}

impl SimpleInstruction for Select {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        let result = if Jump::test(self.op, &self.x, &self.y, state) {
            &self.if_true
        } else {
            &self.if_false
        };
        self.result.set_from(state, result);
    }
}

struct Lookup {
    content_type: ContentType,
    result: LVar,
    id: LVar,
}

impl SimpleInstruction for Lookup {
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

            ContentType::Team => id.try_into().ok().map(Team::from_id).into(),

            _ => LValue::Null,
        };

        self.result.set(state, result);
    }
}

struct PackColor {
    result: LVar,
    r: LVar,
    g: LVar,
    b: LVar,
    a: LVar,
}

impl SimpleInstruction for PackColor {
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
        )
    }
}

struct UnpackColor {
    r: LVar,
    g: LVar,
    b: LVar,
    a: LVar,
    value: LVar,
}

impl SimpleInstruction for UnpackColor {
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

impl SimpleInstruction for Noop {
    fn execute(&self, _: &mut ProcessorState, _: &LogicVM) {}
}

struct Wait {
    value: LVar,
}

impl Instruction for Wait {
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

struct Stop;

impl Instruction for Stop {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) -> InstructionResult {
        state.counter -= 1;
        state.set_stopped(true);
        InstructionResult::Yield
    }
}

struct End;

impl SimpleInstruction for End {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        state.counter = state.num_instructions;
    }
}

struct Jump {
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
            ConditionOp::Equal => {
                if x.isobj() && y.isobj() {
                    x == y
                } else {
                    (x.num() - y.num()).abs() < EQUALITY_EPSILON
                }
            }
            ConditionOp::NotEqual => {
                if x.isobj() && y.isobj() {
                    x != y
                } else {
                    (x.num() - y.num()).abs() >= EQUALITY_EPSILON
                }
            }
            ConditionOp::LessThan => x.num() < y.num(),
            ConditionOp::LessThanEq => x.num() <= y.num(),
            ConditionOp::GreaterThan => x.num() > y.num(),
            ConditionOp::GreaterThanEq => x.num() >= y.num(),
            ConditionOp::StrictEqual => x == y,
            ConditionOp::Always => unreachable!(),
        }
    }
}

impl SimpleInstruction for Jump {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        if Self::test(self.op, &self.x, &self.y, state) {
            // we do the bounds check while parsing
            state.counter = self.target;
        }
    }
}

// privileged

struct GetBlock {
    layer: TileLayer,
    result: LVar,
    x: LVar,
    y: LVar,
}

impl SimpleInstruction for GetBlock {
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

struct SetRate {
    value: LVar,
}

impl SimpleInstruction for SetRate {
    fn execute(&self, state: &mut ProcessorState, _: &LogicVM) {
        state.ipt = (self.value.get(state).num() as usize).clamp(1, MAX_IPT);
    }
}
