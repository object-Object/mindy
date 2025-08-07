use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    io::Cursor,
    rc::Rc,
};

use binrw::BinRead;
use itertools::Itertools;
use rapidhash::fast::RapidHashSet;
use replace_with::replace_with_or_default_and_return;
use thiserror::Error;
use widestring::{U16Str, U16String};

use super::{
    Constants, LValue, LogicVM, VMLoadError, VMLoadResult,
    buildings::Building,
    instructions::{Instruction, InstructionBuilder, InstructionResult, InstructionTrait, Noop},
    variables::{LVar, Variables},
};
use crate::{
    logic::{LogicParser, ast},
    types::{Object, Point2, ProcessorConfig},
};

pub(super) const MAX_TEXT_BUFFER: usize = 400;
const MAX_INSTRUCTION_SCALE: f64 = 5.0;

pub struct Processor {
    instructions: Vec<Instruction>,
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
        let mut taken_names = HashMap::new();
        self.state.links.retain_mut(|link| {
            // check if the other building exists

            let Some(other) = vm.building(link.position) else {
                return false;
            };

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

            let dist_sq = (here.0 - there.0).powf(2.) + (here.1 - there.1).powf(2.);
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
                taken_names.insert(name_prefix.to_string(), HashSet::new());
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
            .extend(self.state.links.iter().map(|l| l.position));

        // now that we know which links are valid, set up the per-processor constants
        LVar::create_local_constants(&mut self.state.locals, building.position, &self.state.links);

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
        if !self.state.enabled || self.state.wait_end_time > time {
            return;
        }

        self.state.accumulator = f64::min(
            self.state.accumulator + self.state.ipt * delta,
            MAX_INSTRUCTION_SCALE * self.state.ipt,
        );

        while self.state.accumulator >= 1. {
            let res = self.step(vm);
            self.state.accumulator -= 1.;
            if let InstructionResult::Yield = res {
                break;
            }
        }
    }

    /// Do not call if the processor is disabled.
    fn step(&mut self, vm: &LogicVM) -> InstructionResult {
        let mut counter = self.state.counter;
        if counter >= self.instructions.len() {
            counter = 0;
        }

        self.state.counter = counter + 1;
        self.instructions[counter].execute(&mut self.state, vm)
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
    links: Vec<ProcessorLink>,
    linked_positions: RapidHashSet<Point2>,

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
    pub fn enabled(&self) -> bool {
        self.enabled
    }

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

    pub fn stopped(&self) -> bool {
        self.stopped
    }

    /// For use by the `stop` and `write` instructions only.
    pub fn set_stopped(&mut self, stopped: bool) {
        self.stopped = stopped;
        self.set_enabled(!stopped);
    }

    pub fn tick(&self) -> f64 {
        self.time.get() * 60. / 1000.
    }

    pub fn link(&self, index: usize) -> Option<Point2> {
        self.links.get(index).map(|l| l.position)
    }

    pub fn linked_positions(&self) -> &RapidHashSet<Point2> {
        &self.linked_positions
    }

    pub fn privileged(&self) -> bool {
        self.privileged
    }

    pub fn num_instructions(&self) -> usize {
        self.num_instructions
    }

    pub fn try_set_counter(&mut self, value: LValue) -> bool {
        if let LValue::Number(n) = value {
            let counter = n as usize;
            self.counter = if (0..self.num_instructions).contains(&counter) {
                counter
            } else {
                0
            };
            true
        } else {
            false
        }
    }

    pub fn locals(&self) -> &Constants {
        &self.locals
    }

    /// Checks if a variable or local constant exists.
    pub fn has_variable(&self, name: &U16Str) -> bool {
        self.variables.contains_key(name) || self.locals.contains_key(name)
    }

    /// Looks up a variable or local constant by name.
    pub fn variable(&self, name: &U16Str) -> Option<LValue> {
        self.variables
            .get(name)
            .cloned()
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
    pub position: Point2,
}

#[derive(Debug)]
pub(super) struct ProcessorBuilder<'a> {
    pub ipt: f64,
    pub privileged: bool,
    pub running_processors: Rc<Cell<usize>>,
    pub time: Rc<Cell<f64>>,
    pub position: Point2,
    pub config: &'a ProcessorConfig,
}

impl ProcessorBuilder<'_> {
    pub fn parse_config(config: &Object) -> VMLoadResult<ProcessorConfig> {
        let data = match config {
            Object::ByteArray { values } => values,
            _ => {
                return Err(binrw::Error::Custom {
                    pos: 0,
                    err: Box::new(format!("incorrect config type: {config:?}")),
                }
                .into());
            }
        };
        Ok(ProcessorConfig::read(&mut Cursor::new(data))?)
    }

    pub fn build(self) -> VMLoadResult<Box<Processor>> {
        let ProcessorBuilder {
            ipt,
            privileged,
            running_processors,
            time,
            position,
            config,
        } = self;

        let code = LogicParser::new()
            .parse(&config.code)
            // FIXME: hack
            .map_err(|e| VMLoadError::BadProcessorCode(e.to_string()))?;

        // TODO: this could be more efficient
        let mut num_instructions = 0;
        let labels = {
            let mut labels = HashMap::new();
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
            running_processors.update(|n| n + 1);
        }

        let links = config
            .links
            .iter()
            .map(|link| ProcessorLink {
                name: link.name.to_string(),
                position: Point2 {
                    x: position.x + link.x as i32,
                    y: position.y + link.y as i32,
                },
            })
            .collect();

        Ok(Box::new(Processor {
            instructions,
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
                running_processors,
                time,
                printbuffer: U16String::new(),
                locals: Constants::default(),
                variables: Variables::default(),
            },
        }))
    }
}
