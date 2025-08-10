use alloc::{
    borrow::Cow,
    boxed::Box,
    format,
    rc::Rc,
    string::{String, ToString},
    vec::Vec,
};
use core::cell::{Cell, RefCell};
use derivative::Derivative;

use itertools::Itertools;
#[allow(unused_imports)]
use num_traits::float::FloatCore;
use replace_with::replace_with_or_default_and_return;
use thiserror::Error;
use widestring::{U16Str, U16String};

use super::{
    BuildingData, Constants, LValue, LogicVM, LogicVMBuilder, VMLoadError, VMLoadResult,
    buildings::Building,
    instructions::{Instruction, InstructionBuilder, InstructionResult, InstructionTrait, Noop},
    variables::{LVar, Variables},
};
#[cfg(feature = "std")]
use crate::logic::LogicParser;
use crate::{
    logic::ast,
    types::{PackedPoint2, ProcessorLinkConfig, content},
    utils::{RapidHashMap, RapidHashSet},
};

pub(super) const MAX_TEXT_BUFFER: usize = 400;
const MAX_INSTRUCTION_SCALE: f64 = 5.0;

pub type InstructionHook =
    dyn FnMut(&Instruction, &mut ProcessorState, &LogicVM) -> Option<InstructionResult>;

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Processor {
    instructions: Box<[Instruction]>,
    #[derivative(Debug = "ignore")]
    instruction_hook: Option<Box<InstructionHook>>,
    pub state: ProcessorState,
}

impl Processor {
    pub fn late_init(
        &mut self,
        vm: &LogicVM,
        building: &Building,
        globals: &Constants,
    ) -> VMLoadResult<()> {
        // init links
        // this is the only reason for the late init logic to exist
        // TODO: this may produce different link names than mindustry in specific cases
        // ie. if a custom link name is specified for a building that would be built after this processor
        let mut taken_names = RapidHashMap::default();

        let mut links = core::mem::take(&mut self.state.links).into_vec();
        links.retain_mut(|link| {
            // resolve the actual building at the link position
            // before this, link.building is just air

            let Some(other) = vm.building(link.building.position) else {
                return false;
            };
            link.building = other.clone();

            // check range

            let self_size = (building.block.size as f64) / 2.;
            let other_size = (other.block.size as f64) / 2.;

            let here = (
                building.position.x as f64 + self_size,
                building.position.y as f64 + self_size,
            );
            let there = (
                other.position.x as f64 + other_size,
                other.position.y as f64 + other_size,
            );

            let dist_sq = (here.0 - there.0).powi(2) + (here.1 - there.1).powi(2);
            let range = building.block.range + other_size;
            if dist_sq > range * range {
                return false;
            }

            // finally, get the link name

            // link name prefix, eg "processor"
            let split = other.block.name.split('-').collect_vec();
            let name_prefix = if split.len() >= 2
                && (split[split.len() - 1] == "large"
                    || split[split.len() - 1].parse::<f64>().is_ok())
            {
                split[split.len() - 2]
            } else {
                split[split.len() - 1]
            };

            // link indices that are already in use for this prefix
            if !taken_names.contains_key(name_prefix) {
                taken_names.insert(name_prefix.to_string(), RapidHashSet::default());
            }
            let taken = taken_names.get_mut(name_prefix).unwrap();

            // if the name from the config has the correct prefix, use it
            // this means multiple buildings may be configured with the same link name
            // but this is how mindustry behaves, so we should allow it too
            if let Some(num) = link.name.strip_prefix(name_prefix) {
                if let Ok(num) = num.parse() {
                    taken.insert(num);
                }
                return true;
            }

            // otherwise, take the first available index
            for i in 1usize.. {
                if taken.insert(i) {
                    link.name = format!("{name_prefix}{i}");
                    return true;
                }
            }
            false // should never happen
        });
        self.state.links = links.into();

        self.state
            .linked_positions
            .extend(self.state.links.iter().map(|l| l.building.position));

        // now that we know which links are valid, set up the per-processor constants
        LVar::create_local_constants(&mut self.state.locals, building, &self.state.links);

        // finally, finish parsing the instructions
        // this must only be done after the link variables have been added
        for instruction in self.instructions.iter_mut() {
            // at this point, all instructions should be InstructionBuilders
            // for each instruction, we take ownership of the builder and convert it to the final instruction
            replace_with_or_default_and_return(
                instruction,
                |instruction| -> (VMLoadResult<()>, _) {
                    let result = match instruction {
                        Instruction::InstructionBuilder(builder) => {
                            builder.late_init(globals, &mut self.state)
                        }
                        _ => Err(VMLoadError::AlreadyInitialized),
                    };
                    match result {
                        Ok(instruction) => (Ok(()), instruction),
                        Err(err) => (Err(err), Noop.into()),
                    }
                },
            )?;
        }

        Ok(())
    }

    pub fn do_tick(&mut self, vm: &LogicVM, time: f64, delta: f64) {
        if !self.state.enabled {
            return;
        }

        self.state.accumulator = f64::min(
            self.state.accumulator + self.state.ipt * delta,
            MAX_INSTRUCTION_SCALE * self.state.ipt,
        );

        if self.state.wait_end_time > time {
            return;
        }

        // casting to usize truncates the fractional part
        // so this is equivalent to `while self.state.accumulator >= 1.`
        for i in 0..(self.state.accumulator as usize) {
            if let InstructionResult::Yield = self.step(vm) {
                self.state.accumulator -= (i + 1) as f64;
                return;
            }
        }
        // if we didn't yield, then we consumed all integer steps in the accumulator
        // so leave only the fractional part
        self.state.accumulator = self.state.accumulator.fract();
    }

    /// Do not call if the processor is disabled.
    fn step(&mut self, vm: &LogicVM) -> InstructionResult {
        let mut counter = self.state.counter;
        if counter >= self.instructions.len() {
            counter = 0;
        }

        self.state.counter = counter + 1;

        // SAFETY: we checked the upper bound already
        // and processors without instructions are always disabled, so self.instructions cannot be empty here
        let instruction = unsafe { self.instructions.get_unchecked(counter) };

        // if we have a hook and it returns a result, skip executing the instruction
        if let Some(hook) = &mut self.instruction_hook
            && let Some(result) = hook(instruction, &mut self.state, vm)
        {
            result
        } else {
            instruction.execute(&mut self.state, vm)
        }
    }
}

#[derive(Debug, Error)]
pub enum SetVariableError {
    #[error("Variable not found.")]
    NotFound,
}

#[derive(Debug, Clone)]
pub struct ProcessorState {
    enabled: bool,
    /// True if we're currently at a `stop` instruction.
    stopped: bool,
    pub(super) wait_end_time: f64,

    privileged: bool,
    num_instructions: usize,
    links: Box<[ProcessorLink]>,
    linked_positions: RapidHashSet<PackedPoint2>,

    pub counter: usize,
    accumulator: f64,
    pub ipt: f64,

    running_processors: Rc<Cell<usize>>,
    pub(super) time: Rc<Cell<f64>>,
    // we use U16String instead of Utf16String or String because Java strings allow invalid UTF-16
    // this behaviour is user-visible with printchar and when reading from a message
    // https://users.rust-lang.org/t/why-is-a-char-valid-in-jvm-but-invalid-in-rust/73524
    pub printbuffer: U16String,

    pub(super) locals: Constants,
    pub variables: Variables,
}

impl ProcessorState {
    #[inline(always)]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    #[inline(always)]
    pub fn set_enabled(&mut self, enabled: bool) {
        match (self.enabled, enabled) {
            // if transitioning from enabled to disabled, decrement running_processors
            (true, false) => {
                self.running_processors.update(|n| n - 1);
            }
            // if transitioning from disabled to enabled, increment running_processors
            // but don't enable if we don't have any instructions to execute
            // or if we would just stop again immediately
            (false, true) if self.num_instructions > 0 && !self.stopped => {
                self.running_processors.update(|n| n + 1);
            }
            _ => return,
        }
        self.enabled = enabled;
    }

    #[inline(always)]
    pub fn stopped(&self) -> bool {
        self.stopped
    }

    /// For use by the `stop` and `write` instructions only.
    #[inline(always)]
    pub fn set_stopped(&mut self, stopped: bool) {
        self.stopped = stopped;
        self.set_enabled(!stopped);
    }

    #[inline(always)]
    pub fn tick(&self) -> f64 {
        self.time.get() * 60. / 1000.
    }

    #[inline(always)]
    pub fn link(&self, index: usize) -> Option<&Building> {
        self.links.get(index).map(|l| &l.building)
    }

    #[inline(always)]
    pub(super) fn links(&self) -> &[ProcessorLink] {
        &self.links
    }

    #[inline(always)]
    pub fn linked_positions(&self) -> &RapidHashSet<PackedPoint2> {
        &self.linked_positions
    }

    #[inline(always)]
    pub fn privileged(&self) -> bool {
        self.privileged
    }

    #[inline(always)]
    pub fn num_instructions(&self) -> usize {
        self.num_instructions
    }

    #[inline(always)]
    pub fn try_set_counter(counter: &mut usize, value: &LValue) {
        if value.isnum() {
            // we do a bounds check in the exec loop, so don't bother here
            *counter = value.num() as usize;
        }
    }

    #[inline(always)]
    pub fn locals(&self) -> &Constants {
        &self.locals
    }

    // these aren't used internally, so don't bother inlining

    /// Checks if a variable or local constant exists.
    pub fn has_variable(&self, name: &U16Str) -> bool {
        self.variables.contains_key(name) || self.locals.contains_key(name)
    }

    /// Looks up a variable or local constant by name.
    pub fn variable<'a>(&'a self, name: &U16Str) -> Option<Cow<'a, LValue>> {
        self.variables
            .get(name)
            .map(Cow::Borrowed)
            .or_else(|| self.locals.get(name).map(|v| v.get(self)))
    }

    pub fn set_variable(&mut self, name: &U16Str, value: LValue) -> Result<(), SetVariableError> {
        if self.variables.contains_key(name) {
            self.variables[name] = value;
            Ok(())
        } else {
            Err(SetVariableError::NotFound)
        }
    }
}

/// A representation of a link from this processor to a building.
#[derive(Debug, Clone)]
pub(super) struct ProcessorLink {
    pub name: String,
    pub building: Building,
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct ProcessorBuilder<'a> {
    pub ipt: f64,
    pub privileged: bool,
    pub code: Box<[ast::Statement]>,
    pub links: &'a [ProcessorLinkConfig],
    /// If provided, this is called just before this processor executes each instruction.
    /// The intercepted instruction is skipped if this hook returns `Some`.
    #[derivative(Debug = "ignore")]
    pub instruction_hook: Option<Box<InstructionHook>>,
}

impl ProcessorBuilder<'_> {
    #[cfg(feature = "std")]
    pub fn parse_code(code: &str) -> VMLoadResult<Box<[ast::Statement]>> {
        match LogicParser::new().parse(code) {
            Ok(value) => Ok(value.into_boxed_slice()),
            // FIXME: hack
            Err(e) => Err(VMLoadError::BadProcessorCode(e.to_string())),
        }
    }

    pub fn build(self, position: PackedPoint2, builder: &LogicVMBuilder) -> Box<Processor> {
        let ProcessorBuilder {
            ipt,
            privileged,
            code,
            links,
            instruction_hook,
        } = self;

        // TODO: this could be more efficient
        let mut num_instructions = 0;
        let labels = {
            let mut labels = RapidHashMap::default();
            for statement in &code {
                match statement {
                    ast::Statement::Label(label) => {
                        labels.insert(label.clone(), num_instructions);
                    }
                    ast::Statement::Instruction(_, _) => {
                        num_instructions += 1;
                    }
                }
            }
            Rc::new(labels)
        };

        let mut instructions: Vec<Instruction> = Vec::with_capacity(num_instructions);
        for statement in code.into_iter() {
            if let ast::Statement::Instruction(instruction, _) = statement {
                instructions.push(
                    InstructionBuilder {
                        instruction,
                        labels: labels.clone(),
                    }
                    .into(),
                );
            }
        }

        let enabled = !instructions.is_empty();
        if enabled {
            builder.vm.running_processors.update(|n| n + 1);
        }

        let fake_data = Rc::new(RefCell::new(BuildingData::Unknown {
            senseable_config: None,
        }));

        let links = links
            .iter()
            .map(|link| ProcessorLink {
                name: link.name.to_string(),
                building: Building {
                    block: &content::blocks::AIR,
                    position: PackedPoint2 {
                        x: position.x + link.x,
                        y: position.y + link.y,
                    },
                    data: fake_data.clone(),
                },
            })
            .collect();

        Box::new(Processor {
            instructions: instructions.into(),
            instruction_hook,
            state: ProcessorState {
                enabled,
                stopped: false,
                wait_end_time: -1.,
                privileged,
                num_instructions,
                links,
                linked_positions: RapidHashSet::default(),
                counter: 0,
                accumulator: 0.,
                ipt,
                running_processors: builder.vm.running_processors.clone(),
                time: builder.vm.time.clone(),
                printbuffer: U16String::new(),
                locals: Constants::default(),
                variables: Variables::default(),
            },
        })
    }
}
