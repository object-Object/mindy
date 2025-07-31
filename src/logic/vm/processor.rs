use std::{cell::Cell, cmp::min, collections::HashMap, io::Cursor, rc::Rc};

use binrw::BinRead;

use super::{
    LogicVM, VMLoadError, VMLoadResult,
    instructions::{Instruction, InstructionResult, parse_instruction},
    variables::LVar,
};
use crate::{
    logic::{LogicParser, ast},
    types::{Object, ProcessorConfig},
};

const MAX_INSTRUCTION_SCALE: usize = 5;

pub struct Processor {
    range: f32,
    privileged: bool,
    instructions: Vec<Box<dyn Instruction>>,
    links: Vec<ProcessorLink>,
    pub(super) state: ProcessorState,
}

impl Processor {
    pub fn do_tick(&mut self, vm: &LogicVM) {
        if !self.state.enabled {
            return;
        }

        self.state.accumulator = min(
            self.state.accumulator + self.state.ipt,
            MAX_INSTRUCTION_SCALE * self.state.ipt,
        );

        while self.state.accumulator >= 1 {
            let res = self.step(vm);
            self.state.accumulator -= 1;
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

#[derive(Debug, Clone)]
pub struct ProcessorState {
    enabled: bool,
    /// True if we're currently at a `stop` instruction.
    stopped: bool,
    pub(super) num_instructions: usize,

    pub(super) counter: usize,
    accumulator: usize,
    pub(super) ipt: usize,

    running_processors: Rc<Cell<usize>>,
    pub(super) time: Rc<Cell<f64>>,
    pub(super) printbuffer: String,
    pub(super) variables: HashMap<String, LVar>,
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

    /// For use by the `stop` instruction only.
    pub fn set_stopped(&mut self, stopped: bool) {
        self.stopped = stopped;
        self.set_enabled(!stopped);
    }

    pub fn tick(&self) -> f64 {
        self.time.get() * 60. / 1000.
    }
}

#[derive(Debug, Clone)]
struct ProcessorLink {}

#[derive(Debug)]
pub struct ProcessorBuilder<'a> {
    pub ipt: usize,
    pub range: f32,
    pub privileged: bool,
    pub running_processors: Rc<Cell<usize>>,
    pub time: Rc<Cell<f64>>,
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

    pub fn build(self) -> VMLoadResult<Processor> {
        let ProcessorBuilder {
            ipt,
            range,
            privileged,
            running_processors,
            time,
            config,
        } = self;

        let code = LogicParser::new()
            .parse(&config.code)
            // FIXME: hack
            .map_err(|e| VMLoadError::BadProcessorCode(e.to_string()))?;

        // TODO: this could be more efficient
        let mut labels = HashMap::new();
        let mut num_instructions = 0;
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

        let mut variables = HashMap::new();
        LVar::init_globals(&mut variables);

        let mut instructions = Vec::with_capacity(num_instructions);
        for statement in code.into_iter() {
            if let ast::Statement::Instruction(instruction, _) = statement {
                instructions.push(parse_instruction(
                    instruction,
                    &mut variables,
                    &labels,
                    privileged,
                    num_instructions,
                )?);
            }
        }

        // TODO: implement, late-init after adding all blocks
        let links = Vec::new();

        Ok(Processor {
            range,
            privileged,
            links,
            state: ProcessorState {
                enabled: !instructions.is_empty(),
                stopped: false,
                num_instructions,
                counter: 0,
                accumulator: 0,
                ipt,
                running_processors,
                time,
                printbuffer: String::new(),
                variables,
            },
            instructions,
        })
    }
}
