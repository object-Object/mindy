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
#[allow(unused_imports)]
use num_traits::float::FloatCore;
use replace_with::replace_with_or_default_and_return;
use widestring::{U16Str, U16String};

use super::{
    Building, BuildingData, DrawCommand, InstructionResult, LValue, LVar, LogicVM, VMLoadError,
    VMLoadResult,
    instructions::{Instruction, InstructionBuilder, InstructionTrait, Noop},
    variables::{Constants, Variables},
};
#[cfg(feature = "std")]
use crate::parser::LogicParser;
use crate::{
    parser::ast,
    types::{PackedPoint2, ProcessorLinkConfig, content},
    utils::{RapidHashMap, RapidHashSet},
};

pub(super) const MAX_TEXT_BUFFER: usize = 400;
pub(super) const MAX_DRAW_BUFFER: usize = 400;
const MAX_INSTRUCTION_SCALE: f64 = 5.0;

pub type InstructionHook =
    dyn FnMut(&Instruction, &mut ProcessorState, &LogicVM) -> Option<InstructionResult>;

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Processor {
    instructions: Vec<Instruction>,
    #[derivative(Debug = "ignore")]
    instruction_hook: Option<Box<InstructionHook>>,
    pub state: ProcessorState,
}

impl Processor {
    pub(super) fn late_init(
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

        self.state.links.retain_mut(|link| {
            // resolve the actual building at the link position
            // before this, link.building is just air

            let Some(other) = vm.building(link.building.position) else {
                return false;
            };
            link.building = other.clone();

            // check range

            #[cfg(feature = "enforce_processor_range")]
            {
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
            }

            // finally, get the link name

            // link name prefix, eg "processor"
            let mut parts = other.block.name.rsplit('-');
            let last_part = parts.next().unwrap_or("");
            let name_prefix = if let Some(second_last_part) = parts.next()
                && (last_part == "large" || last_part.parse::<f64>().is_ok())
            {
                second_last_part
            } else {
                last_part
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

        self.state
            .linked_positions
            .extend(self.state.links.iter().map(|l| l.building.position));

        // now that we know which links are valid, set up the per-processor constants
        LVar::create_local_constants(&mut self.state.locals, building, &self.state.links);

        // finish parsing the instructions
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

        // finally, now that we know everything has succeeded, tell the VM if this processor is running
        if self.state.enabled {
            vm.running_processors.update(|n| n + 1);
        }

        Ok(())
    }

    /// Overwrites the code (and optionally the links) of this processor, resetting most internal state.
    ///
    /// This method is meant to be used for moving a processor from one VM to another. If you want to modify the code/links of a processor while it's still in a VM, use [`Self::update_config`] instead.
    pub fn reset_config<T>(
        mut self,
        code: T,
        links: Option<&[ProcessorLinkConfig]>,
        vm: impl AsRef<LogicVM>,
        position: PackedPoint2,
    ) -> Self
    where
        T: IntoIterator<Item = ast::Statement>,
        for<'a> &'a T: IntoIterator<Item = &'a ast::Statement>,
    {
        self.instructions.clear();
        self.state = ProcessorState::new(self.state.privileged, self.state.ipt, vm.as_ref());
        self.set_initial_config(code, links, position);
        self
    }

    /// Overwrites the code (and optionally the links) of this processor in-place, resetting most internal state.
    ///
    /// If an error occurs, all changes will be rolled back.
    pub fn update_config<T>(
        &mut self,
        code: T,
        links: Option<&[ProcessorLinkConfig]>,
        vm: &LogicVM,
        building: &Building,
        globals: &Constants,
    ) -> VMLoadResult<()>
    where
        T: IntoIterator<Item = ast::Statement>,
        for<'a> &'a T: IntoIterator<Item = &'a ast::Statement>,
    {
        let prev_instructions = core::mem::take(&mut self.instructions);

        // late_init assumes the processor is disabled and increments running_processors if it becomes enabled
        // so decrement running_processors if the processor is currently enabled to avoid double-counting
        let prev_running_processors = vm.running_processors.get();
        if self.state.enabled {
            vm.running_processors.update(|n| n - 1);
        }

        // this preserves any previous setrate calls, which matches Mindustry's behaviour
        let new_state = ProcessorState::new(self.state.privileged, self.state.ipt, vm);
        let prev_state = core::mem::replace(&mut self.state, new_state);

        // this assumes self.state is newly initialized
        self.set_initial_config(code, links, building.position);

        // if the initialization fails, roll back the changes
        let result = self.late_init(vm, building, globals);
        if result.is_err() {
            let _ = core::mem::replace(&mut self.instructions, prev_instructions);
            vm.running_processors.set(prev_running_processors);
            let _ = core::mem::replace(&mut self.state, prev_state);
        }
        result
    }

    /// Overwrites the code/links of this processor **without** fully initializing them. Assumes the processor is currently in its default state.
    fn set_initial_config<T>(
        &mut self,
        code: T,
        links: Option<&[ProcessorLinkConfig]>,
        position: PackedPoint2,
    ) where
        T: IntoIterator<Item = ast::Statement>,
        for<'a> &'a T: IntoIterator<Item = &'a ast::Statement>,
    {
        let labels = {
            let mut labels = RapidHashMap::default();
            for statement in (&code).into_iter() {
                match statement {
                    ast::Statement::Label(label) => {
                        labels.insert(label.clone(), self.state.num_instructions);
                    }
                    ast::Statement::Instruction(_, _) => {
                        self.state.num_instructions += 1;
                    }
                }
            }
            Rc::new(labels)
        };

        self.instructions.reserve_exact(self.state.num_instructions);
        for statement in code.into_iter() {
            if let ast::Statement::Instruction(instruction, _) = statement {
                self.instructions.push(
                    InstructionBuilder {
                        instruction,
                        labels: labels.clone(),
                    }
                    .into(),
                );
            }
        }

        self.state.enabled = !self.instructions.is_empty();

        let fake_data = Rc::new(RefCell::new(BuildingData::Unknown {
            senseable_config: None,
        }));

        if let Some(links) = links {
            self.state
                .links
                .extend(links.iter().map(|link| ProcessorLink {
                    name: link.name.to_string(),
                    building: Building {
                        block: &content::blocks::AIR,
                        position: PackedPoint2 {
                            x: position.x + link.x,
                            y: position.y + link.y,
                        },
                        data: fake_data.clone(),
                    },
                }));
        }
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
            // SAFETY: self.state.enabled is always false if self.instructions is empty
            if let InstructionResult::Yield = unsafe { self.step(vm) } {
                self.state.accumulator -= (i + 1) as f64;
                return;
            }
        }
        // if we didn't yield, then we consumed all integer steps in the accumulator
        // so leave only the fractional part
        self.state.accumulator = self.state.accumulator.fract();
    }

    /// Executes a single instruction.
    ///
    /// # Safety
    ///
    /// Calling this method on a processor with no instructions is undefined behavior.
    #[inline(always)]
    pub unsafe fn step(&mut self, vm: &LogicVM) -> InstructionResult {
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

#[derive(Debug, Clone)]
pub struct ProcessorState {
    enabled: bool,
    /// True if we're currently at a `stop` instruction.
    stopped: bool,
    pub(super) wait_end_time: f64,

    privileged: bool,
    num_instructions: usize,
    links: Vec<ProcessorLink>,
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
    pub drawbuffer: Vec<DrawCommand>,
    pub(super) drawbuffer_len: usize,

    pub(super) locals: Constants,
    pub(super) variables: Variables,
}

impl ProcessorState {
    fn new(privileged: bool, ipt: f64, vm: &LogicVM) -> Self {
        Self {
            enabled: false,
            stopped: false,
            wait_end_time: -1.,

            privileged,
            num_instructions: 0,
            links: Vec::new(),
            linked_positions: RapidHashSet::default(),

            counter: 0,
            accumulator: 0.,
            ipt,

            running_processors: vm.running_processors.clone(),
            time: vm.time.clone(),
            printbuffer: U16String::new(),
            drawbuffer: Vec::new(),
            drawbuffer_len: 0,

            locals: Constants::default(),
            variables: Variables::default(),
        }
    }

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
    pub fn links(&self) -> &[ProcessorLink] {
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

    /// Checks if a variable exists in this processor.
    pub fn has_variable(&self, name: &U16Str) -> bool {
        self.variables.contains_key(name) || self.locals.contains_key(name)
    }

    /// Looks up a variable by name in this processor.
    pub fn variable<'a>(&'a self, name: &U16Str) -> Option<Cow<'a, LValue>> {
        self.variables.get(name).map(Cow::Borrowed)
    }

    /// Sets the value of an existing variable in this processor.
    ///
    /// ***Panics*** if the variable does not exist.
    pub fn set_variable(&mut self, name: &U16Str, value: LValue) {
        self.variables[name] = value;
    }
}

/// A representation of a link from this processor to a building.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ProcessorLink {
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

    pub fn build(self, position: PackedPoint2, vm: impl AsRef<LogicVM>) -> Box<Processor> {
        let ProcessorBuilder {
            ipt,
            privileged,
            code,
            links,
            instruction_hook,
        } = self;

        let mut processor = Processor {
            instructions: Vec::new(),
            instruction_hook,
            state: ProcessorState::new(privileged, ipt, vm.as_ref()),
        };

        processor.set_initial_config(code, Some(links), position);

        Box::new(processor)
    }
}
