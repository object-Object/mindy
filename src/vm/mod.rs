use alloc::{rc::Rc, string::String, vec::Vec};
use core::{cell::Cell, time::Duration};
#[cfg(feature = "std")]
use std::time::Instant;

use thiserror::Error;

#[cfg(feature = "embedded_graphics")]
pub use self::draw::embedded::{EmbeddedDisplayData, EmbeddedDisplayInitError};
use self::variables::Constants;
pub use self::{
    buildings::{Building, BuildingData, CustomBuildingData},
    draw::{DrawCommand, TextAlignment},
    instructions::InstructionResult,
    processor::{InstructionHook, Processor, ProcessorBuilder, ProcessorState},
    variables::{Content, LObject, LString, LValue, LVar},
};
#[cfg(feature = "std")]
use crate::types::{Schematic, SchematicTile};
use crate::{
    types::{PackedPoint2, content::Block},
    utils::RapidHashMap,
};

pub mod buildings;
mod draw;
pub mod instructions;
mod processor;
pub mod variables;

const MILLIS_PER_SEC: u64 = 1_000;
const NANOS_PER_MILLI: u32 = 1_000_000;

pub struct LogicVM {
    /// Sorted with all processors in update order first, then all other buildings in arbitrary order.
    buildings: Vec<Building>,
    buildings_map: RapidHashMap<PackedPoint2, usize>,
    total_processors: usize,
    running_processors: Rc<Cell<usize>>,
    time: Rc<Cell<f64>>,
}

impl LogicVM {
    /// Creates a new, empty VM.
    ///
    /// This is usually not the method you want. See [`Self::from_buildings`], [`LogicVMBuilder`], etc.
    pub fn new() -> Self {
        Self {
            buildings: Vec::new(),
            buildings_map: RapidHashMap::default(),
            total_processors: 0,
            running_processors: Rc::new(Cell::new(0)),
            time: Rc::new(Cell::new(0.)),
        }
    }

    #[cfg(feature = "std")]
    pub fn from_schematic(schematic: &Schematic) -> VMLoadResult<Self> {
        Self::from_schematic_tiles(schematic.tiles())
    }

    #[cfg(feature = "std")]
    pub fn from_schematic_tiles(tiles: &[SchematicTile]) -> VMLoadResult<Self> {
        let mut builder = LogicVMBuilder::new();
        builder.add_schematic_tiles(tiles)?;
        builder.build()
    }

    pub fn from_buildings(buildings: impl IntoIterator<Item = Building>) -> VMLoadResult<Self> {
        let mut builder = LogicVMBuilder::new();
        builder.add_buildings(buildings);
        builder.build()
    }

    pub fn building(&self, position: PackedPoint2) -> Option<&Building> {
        self.buildings_map
            .get(&position)
            .map(|&i| &self.buildings[i])
    }

    /// Add a new building to a running VM.
    ///
    /// Processors added using this method will be appended to the end of the update order, shifting all non-processor buildings to the right. To add processors in load order more efficiently, use a [`LogicVMBuilder`].
    pub fn add_building(&mut self, building: Building, globals: &Constants) -> VMLoadResult<()> {
        // check for overlaps first, so that we don't mutate the VM until we know we can do it successfully
        for position in building.iter_positions() {
            if let Some(current) = self.building(position) {
                return Err(VMLoadError::Overlap {
                    position,
                    current: current.block,
                });
            }
        }

        // if it's a processor, run late_init before inserting
        let is_processor = match &mut *building.data.borrow_mut() {
            BuildingData::Processor(processor) => {
                processor.late_init(self, &building, globals)?;
                true
            }
            _ => false,
        };

        // do this here because building is moved into self.buildings
        let all_positions = building.iter_positions();

        // insert the new building into self.buildings
        let index = if is_processor {
            // shift all non-processor indices right by one
            for building in self.buildings.iter().skip(self.total_processors) {
                for position in building.iter_positions() {
                    *self.buildings_map.get_mut(&position).unwrap() += 1;
                }
            }

            // shift the actual non-processor buildings right by one
            self.buildings.insert(self.total_processors, building);
            self.total_processors += 1;
            self.total_processors
        } else {
            self.buildings.push(building);
            self.buildings.len()
        } - 1;

        // finally, insert all of the position lookups
        for position in all_positions {
            self.buildings_map.insert(position, index);
        }

        Ok(())
    }

    /// Remove a building from a running VM.
    ///
    /// ***IMPORTANT:*** This method violates the assumption that buildings are never removed from a LogicVM. The caller must keep track of all processors linked to this building and use [`Processor::update_config`] to remove those links manually - the VM will not (and cannot) do this for you. This also means that "ghost cells" are possible - processors may retain references to the removed building.
    pub fn remove_building(&mut self, position: PackedPoint2) -> Option<Building> {
        let &index = self.buildings_map.get(&position)?;

        // possible cases:
        // index < self.total_processors: processor
        // index + 1 == self.total_processors: *last* processor
        // index >= self.total_processors: not processor
        let building = if index + 1 < self.total_processors {
            // this building is a processor with at least one more processor after it in the update order
            // so we need to shift all subsequent buildings left by one
            // otherwise, either the update order would change or a non-processor would be moved into the processor section

            // update buildings_map for all subsequent buildings
            // eg. if we're removing the 0th building, start at index 1
            for building in self.buildings.iter().skip(index + 1) {
                for position in building.iter_positions() {
                    *self.buildings_map.get_mut(&position).unwrap() -= 1;
                }
            }

            self.buildings.remove(index)
        } else {
            // this building is either not a processor, or it's the very last processor in the update order
            // in both cases, we can safely use swap_remove

            // if this is not the last building, update buildings_map for the building we're swapping into its place
            if let Some(building) = self.buildings.last() {
                for position in building.iter_positions() {
                    self.buildings_map.insert(position, index);
                }
            }

            self.buildings.swap_remove(index)
        };

        // if the building is a processor, decrement total_processors and (maybe) running_processors
        if index < self.total_processors {
            self.total_processors -= 1;
            if building.data.borrow().unwrap_processor().state.enabled() {
                self.running_processors.update(|n| n - 1);
            }
        }

        // remove the building from buildings_map
        for position in building.iter_positions() {
            self.buildings_map.remove(&position);
        }

        Some(building)
    }

    /// Run the simulation until all processors halt, or until a number of ticks are finished.
    /// Returns true if all processors halted, or false if the tick limit was reached.
    #[cfg(feature = "std")]
    pub fn run(&mut self, max_ticks: Option<usize>) -> bool {
        self.run_with_delta(max_ticks, 1.0)
    }

    /// Run the simulation until all processors halt, or until a number of ticks are finished.
    /// Returns true if all processors halted, or false if the tick limit was reached.
    #[cfg(feature = "std")]
    pub fn run_with_delta(&mut self, max_ticks: Option<usize>, delta: f64) -> bool {
        let start = Instant::now();
        let mut tick = 0;

        loop {
            self.do_tick_with_delta(start.elapsed(), delta);

            if self.running_processors.get() == 0 {
                // all processors finished, return true
                return true;
            }

            if let Some(max_ticks) = max_ticks {
                tick += 1;
                if tick >= max_ticks {
                    // hit tick limit, return false
                    return false;
                }
            }
        }
    }

    /// Execute one tick of the simulation with a delta of `1.0`.
    ///
    /// `time` is the time elapsed since the *start* of the simulation.
    pub fn do_tick(&mut self, time: Duration) {
        self.do_tick_with_delta(time, 1.0);
    }

    /// Execute one tick of the simulation.
    ///
    /// `time` is the time elapsed since the *start* of the simulation.
    ///
    /// `delta` is the simulated time delta, eg. `1.0` corresponds to 60 fps.
    pub fn do_tick_with_delta(&mut self, time: Duration, delta: f64) {
        let time = duration_millis_f64(time);
        self.time.set(time);

        for processor in self.iter_processors() {
            processor
                .data
                .borrow_mut()
                .unwrap_processor_mut()
                .do_tick(self, time, delta);
        }
    }

    fn iter_processors(&self) -> impl Iterator<Item = &Building> {
        self.buildings.iter().take(self.total_processors)
    }

    pub fn running_processors(&self) -> usize {
        self.running_processors.get()
    }

    pub fn total_processors(&self) -> usize {
        self.total_processors
    }

    pub fn time(&self) -> Duration {
        Duration::from_secs_f64(self.time.get() / 1000.)
    }
}

impl Default for LogicVM {
    fn default() -> Self {
        Self::new()
    }
}

impl AsRef<LogicVM> for LogicVM {
    fn as_ref(&self) -> &LogicVM {
        self
    }
}

pub struct LogicVMBuilder {
    vm: LogicVM,
    processors: Vec<Building>,
    other_buildings: Vec<Building>,
}

impl LogicVMBuilder {
    pub fn new() -> Self {
        Self {
            vm: LogicVM::new(),
            processors: Vec::new(),
            other_buildings: Vec::new(),
        }
    }

    pub fn add_building(&mut self, building: Building) {
        if matches!(*building.data.borrow(), BuildingData::Processor(_)) {
            self.processors.push(building);
        } else {
            self.other_buildings.push(building);
        };
    }

    pub fn add_buildings(&mut self, buildings: impl IntoIterator<Item = Building>) {
        for building in buildings.into_iter() {
            self.add_building(building);
        }
    }

    #[cfg(feature = "std")]
    pub fn add_schematic_tile(&mut self, tile: &SchematicTile) -> VMLoadResult<()> {
        let building = Building::from_schematic_tile(tile, &*self)?;
        self.add_building(building);
        Ok(())
    }

    #[cfg(feature = "std")]
    pub fn add_schematic_tiles(&mut self, tiles: &[SchematicTile]) -> VMLoadResult<()> {
        for tile in tiles {
            self.add_schematic_tile(tile)?;
        }
        Ok(())
    }

    pub fn vm(&self) -> &LogicVM {
        &self.vm
    }

    pub fn build(self) -> VMLoadResult<LogicVM> {
        self.build_with_globals(&LVar::create_global_constants())
    }

    pub fn build_with_globals(mut self, globals: &Constants) -> VMLoadResult<LogicVM> {
        // sort processors in update order
        // 7 8 9
        // 4 5 6
        // 1 2 3
        // a updates before b if a.y < b.y || a.y == b.y && a.x < b.x
        self.processors
            .sort_unstable_by_key(|p| (p.position.y, p.position.x));

        let mut vm = self.vm;

        vm.total_processors = self.processors.len();

        vm.buildings = core::mem::take(&mut self.processors); // yoink
        vm.buildings.extend(self.other_buildings.drain(0..));

        for (i, building) in vm.buildings.iter().enumerate() {
            for position in building.iter_positions() {
                if let Some(current) = vm.building(position) {
                    return Err(VMLoadError::Overlap {
                        position,
                        current: current.block,
                    });
                }
                vm.buildings_map.insert(position, i);
            }
        }

        for processor in vm.iter_processors() {
            processor
                .data
                .borrow_mut()
                .unwrap_processor_mut()
                .late_init(&vm, processor, globals)?;
        }

        Ok(vm)
    }
}

impl Default for LogicVMBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AsRef<LogicVM> for LogicVMBuilder {
    fn as_ref(&self) -> &LogicVM {
        &self.vm
    }
}

fn duration_millis_f64(d: Duration) -> f64 {
    // reimplementation of the unstable function as_millis_f64
    (d.as_secs() as f64) * (MILLIS_PER_SEC as f64)
        + (d.subsec_nanos() as f64) / (NANOS_PER_MILLI as f64)
}

pub type VMLoadResult<T> = Result<T, VMLoadError>;

#[derive(Error, Debug)]
pub enum VMLoadError {
    #[error("unknown block type: {0}")]
    UnknownBlockType(String),

    #[error("expected {want} block type but got {got}")]
    BadBlockType { want: String, got: String },

    #[cfg(feature = "std")]
    #[error("failed to decode processor config")]
    BadProcessorConfig(#[from] binrw::Error),

    #[error("failed to parse processor code: {0}")]
    BadProcessorCode(String),

    #[error("attempted to call late_init on an already-initialized instruction")]
    AlreadyInitialized,

    #[error("tried to place multiple blocks at {position} (current block: {})", current.name)]
    Overlap {
        position: PackedPoint2,
        current: &'static Block,
    },
}

#[cfg(all(test, not(feature = "std"), feature = "no_std"))]
mod tests {
    use alloc::{boxed::Box, rc::Rc, vec};
    use core::{cell::RefCell, time::Duration};

    use pretty_assertions::assert_eq;
    use widestring::u16str;

    use super::*;
    use crate::{
        parser::ast,
        types::{PackedPoint2, content},
    };

    #[test]
    fn test_load_vm() {
        let gpio_data = Rc::new(RefCell::new(BuildingData::Memory(Box::new([0.; 30]))));
        let gpio_build = Building {
            block: &content::blocks::AIR,
            position: PackedPoint2 { x: 1, y: 0 },
            data: gpio_data.clone(),
        };

        let mut globals = LVar::create_global_constants();
        globals.extend([(
            u16str!("gpio").into(),
            LVar::Constant(gpio_build.clone().into()),
        )]);

        let mut builder = LogicVMBuilder::new();
        builder.add_buildings([Building::from_processor_builder(
            &content::blocks::AIR,
            PackedPoint2 { x: 1, y: 2 },
            ProcessorBuilder {
                ipt: 3.,
                privileged: false,
                code: Box::new([
                    ast::Statement::Instruction(
                        ast::Instruction::Set {
                            to: ast::Value::Variable("ipt".into()),
                            from: ast::Value::Variable("@ipt".into()),
                        },
                        vec![],
                    ),
                    ast::Statement::Instruction(
                        ast::Instruction::Set {
                            to: ast::Value::Variable("this".into()),
                            from: ast::Value::Variable("@this".into()),
                        },
                        vec![],
                    ),
                    ast::Statement::Instruction(
                        ast::Instruction::Write {
                            value: ast::Value::Number(1.),
                            target: ast::Value::Variable("gpio".into()),
                            address: ast::Value::Number(25.),
                        },
                        vec![],
                    ),
                    ast::Statement::Instruction(ast::Instruction::Stop, vec![]),
                ]),
                links: &[],
                instruction_hook: None,
            },
            &builder,
        )]);
        let mut vm = builder.build_with_globals(&globals).unwrap();

        vm.do_tick(Duration::ZERO);

        match &*vm.building((1, 2).into()).unwrap().data.borrow() {
            BuildingData::Processor(p) => {
                assert_eq!(
                    p.state.variable(u16str!("ipt")).map(|v| v.into_owned()),
                    Some(3.into())
                );
                let this = p.state.variable(u16str!("this")).unwrap().into_owned();
                assert!(
                    matches!(
                        this.obj(),
                        Some(LObject::Building(Building {
                            position: PackedPoint2 { x: 1, y: 2 },
                            ..
                        }))
                    ),
                    "{this:?}"
                );
            }
            other => panic!("expected processor, got {other:?}"),
        }
        match &*gpio_data.borrow() {
            BuildingData::Memory(memory) => assert_eq!(memory[25], 1.),
            _ => unreachable!(),
        }
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use std::{format, io::Cursor, prelude::rust_2024::*, thread_local, vec};

    use binrw::{BinRead, BinWrite};
    use itertools::Itertools;
    use pretty_assertions::assert_eq;
    use velcro::{map_iter, map_iter_from};
    use widestring::{U16Str, U16String, u16str};

    use super::{
        buildings::{
            HYPER_PROCESSOR, LOGIC_PROCESSOR, MEMORY_BANK, MEMORY_CELL, MESSAGE, MICRO_PROCESSOR,
            SWITCH, WORLD_CELL, WORLD_PROCESSOR,
        },
        instructions::Instruction,
        variables::Constants,
        *,
    };
    use crate::{
        types::{
            ContentID, ContentType, Object, PackedPoint2, ProcessorConfig, ProcessorLinkConfig,
            Team, colors::COLORS, content,
        },
        utils::u16format,
    };

    fn single_processor_vm(name: &str, code: &str) -> LogicVM {
        let mut builder = LogicVMBuilder::new();
        builder.add_building(
            Building::from_processor_config(
                name,
                PackedPoint2::new(0, 0),
                &ProcessorConfig::from_code(code),
                &builder,
            )
            .unwrap(),
        );
        builder.build().unwrap()
    }

    fn single_processor_vm_with_globals(name: &str, code: &str, globals: &Constants) -> LogicVM {
        let mut builder = LogicVMBuilder::new();
        builder.add_building(
            Building::from_processor_config(
                name,
                PackedPoint2::new(0, 0),
                &ProcessorConfig::from_code(code),
                &builder,
            )
            .unwrap(),
        );
        builder.build_with_globals(globals).unwrap()
    }

    fn run(vm: &mut LogicVM, max_ticks: usize, want: bool) {
        assert_eq!(
            vm.run(Some(max_ticks)),
            want,
            "VM took {cmp}{max_ticks} tick{s} to finish",
            cmp = if want { ">=" } else { "<" },
            s = if max_ticks == 1 { "" } else { "s" },
        );
    }

    fn with_processor<T>(vm: &mut LogicVM, position: T, f: impl FnOnce(&mut Processor))
    where
        T: Into<PackedPoint2>,
    {
        f(vm.building(position.into())
            .unwrap()
            .data
            .borrow_mut()
            .unwrap_processor_mut())
    }

    fn take_processor<T>(vm: &mut LogicVM, position: T) -> Processor
    where
        T: Into<PackedPoint2>,
    {
        vm.building(position.into())
            .unwrap()
            .data
            .replace(BuildingData::Unknown {
                senseable_config: None,
            })
            .into_processor()
    }

    fn assert_variables<'a, T, V>(processor: &Processor, vars: T)
    where
        T: IntoIterator<Item = (&'a U16Str, V)>,
        V: Into<Option<LValue>>,
    {
        for (name, want) in vars {
            match want.into() {
                Some(want) => {
                    assert!(
                        processor.state.variables.contains_key(name),
                        "variable not found: {}",
                        name.display()
                    );
                    assert_eq!(processor.state.variables[name], want, "{}", name.display());
                }
                None => assert!(
                    !processor.state.variables.contains_key(name),
                    "unexpected variable found: {}",
                    name.display()
                ),
            };
        }
    }

    fn assert_locals<'a, T, V>(processor: &Processor, vars: T)
    where
        T: IntoIterator<Item = (&'a U16Str, V)>,
        V: Into<Option<LValue>>,
    {
        for (name, want) in vars {
            match want.into() {
                Some(want) => {
                    assert!(
                        processor.state.locals.contains_key(name),
                        "variable not found: {}",
                        name.display()
                    );
                    assert_eq!(
                        *processor.state.locals[name].get(&processor.state),
                        want,
                        "{}",
                        name.display()
                    );
                }
                None => assert!(
                    !processor.state.locals.contains_key(name),
                    "unexpected variable found: {}",
                    name.display()
                ),
            };
        }
    }

    fn assert_variables_epsilon<'a, T>(processor: &Processor, epsilon: f64, vars: T)
    where
        T: IntoIterator<Item = (&'a U16Str, f64)>,
    {
        for (name, want) in vars {
            match want.into() {
                Some(want) => {
                    assert!(
                        processor.state.variables.contains_key(name),
                        "variable not found: {}",
                        name.display()
                    );
                    let got = processor.state.variables[name].num();
                    assert!(
                        (got - want).abs() <= epsilon,
                        "want {} == {want} +- {epsilon}, got {got}",
                        name.display()
                    );
                }
                None => assert!(
                    !processor.state.variables.contains_key(name),
                    "unexpected variable found: {}",
                    name.display()
                ),
            };
        }
    }

    fn assert_variables_buildings<'a, T, V>(processor: &Processor, vars: T)
    where
        T: IntoIterator<Item = (&'a U16Str, V)>,
        V: Into<Option<PackedPoint2>>,
    {
        for (name, want) in vars {
            match want.into() {
                Some(want) => {
                    assert!(
                        processor.state.variables.contains_key(name),
                        "variable not found: {}",
                        name.display()
                    );
                    match processor.state.variables[name].obj() {
                        Some(LObject::Building(building)) => {
                            assert_eq!(building.position, want, "{}", name.display())
                        }
                        other => panic!("unexpected variable type: {} = {other:?}", name.display()),
                    }
                }
                None => assert!(
                    !processor.state.variables.contains_key(name),
                    "unexpected variable found: {}",
                    name.display()
                ),
            };
        }
    }

    fn assert_locals_buildings<'a, T, V>(processor: &Processor, vars: T)
    where
        T: IntoIterator<Item = (&'a U16Str, V)>,
        V: Into<Option<PackedPoint2>>,
    {
        for (name, want) in vars {
            match want.into() {
                Some(want) => {
                    assert!(
                        processor.state.locals.contains_key(name),
                        "variable not found: {}",
                        name.display()
                    );
                    match processor.state.locals[name].get(&processor.state).obj() {
                        Some(LObject::Building(building)) => {
                            assert_eq!(building.position, want, "{}", name.display())
                        }
                        other => panic!("unexpected variable type: {} = {other:?}", name.display()),
                    }
                }
                None => assert!(
                    !processor.state.locals.contains_key(name),
                    "unexpected variable found: {}",
                    name.display()
                ),
            };
        }
    }

    #[test]
    fn test_empty() {
        let mut vm = LogicVM::from_schematic_tiles(&[]).unwrap();
        run(&mut vm, 1, true);
    }

    #[test]
    fn test_from_schematic_tiles() {
        let mut vm = LogicVM::from_schematic_tiles(&[SchematicTile {
            block: "logic-processor".into(),
            position: PackedPoint2 { x: 0, y: 0 },
            config: {
                let mut cur = Cursor::new(Vec::new());
                ProcessorConfig::from_code("stop").write(&mut cur).unwrap();
                cur.into_inner().into()
            },
            rotation: 0,
        }])
        .unwrap();

        run(&mut vm, 1, true);

        let processor = take_processor(&mut vm, (0, 0));
        assert_eq!(processor.state.counter, 0);
        assert!(processor.state.stopped());
    }

    #[test]
    fn test_instruction_hook() {
        let hits = Rc::new(Cell::new(0));

        let mut builder = LogicVMBuilder::new();
        builder.add_building(Building::from_processor_builder(
            content::blocks::FROM_NAME[HYPER_PROCESSOR],
            PackedPoint2 { x: 0, y: 0 },
            ProcessorBuilder {
                ipt: 25.,
                privileged: false,
                code: ProcessorBuilder::parse_code(
                    "
                    op add a 1 0
                    set b 2
                    op add c 3 0
                    set d 4
                    op add e 5 0
                    ",
                )
                .unwrap(),
                links: &[],
                instruction_hook: {
                    let hits = hits.clone();
                    Some(Box::new(move |instruction, state, _| {
                        if let Instruction::Set(instructions::Set { to, from, .. }) = instruction {
                            hits.update(|v| v + 1);
                            let value = from.get(state).num();
                            to.set(state, (value * 10.).into());
                            Some(if value == 4. {
                                InstructionResult::Yield
                            } else {
                                InstructionResult::Ok
                            })
                        } else {
                            None
                        }
                    }))
                },
            },
            &builder,
        ));
        let mut vm = builder.build().unwrap();

        vm.do_tick(Duration::ZERO);

        let processor = take_processor(&mut vm, (0, 0));
        assert_variables(&processor, map_iter! {
            u16str!("a"): 1.into(),
            u16str!("b"): 20.into(),
            u16str!("c"): 3.into(),
            u16str!("d"): 40.into(),
            u16str!("e"): LValue::NULL,
        });
        assert_eq!(hits.get(), 2);
    }

    #[test]
    fn test_auto_link_names() {
        let mut builder = LogicVMBuilder::new();
        builder.add_buildings(
            [
                Building::from_processor_config(
                    MICRO_PROCESSOR,
                    PackedPoint2 { x: 0, y: 0 },
                    &ProcessorConfig::from_code("stop"),
                    &builder,
                ),
                Building::from_processor_config(
                    MICRO_PROCESSOR,
                    PackedPoint2 { x: 1, y: 0 },
                    &ProcessorConfig::from_code("stop"),
                    &builder,
                ),
                Building::from_config(
                    MEMORY_CELL,
                    PackedPoint2 { x: 2, y: 0 },
                    &Object::Null,
                    &builder,
                ),
                Building::from_processor_config(
                    MICRO_PROCESSOR,
                    PackedPoint2 { x: 3, y: 0 },
                    &ProcessorConfig {
                        code: "
                        set link0 processor0
                        set link1 processor1
                        set link2 processor2
                        set link3 processor3
                        set link4 cell0
                        set link5 cell1
                        set link6 cell2
                        stop
                        "
                        .into(),
                        links: vec![
                            ProcessorLinkConfig {
                                name: "".into(),
                                x: -3,
                                y: 0,
                            },
                            ProcessorLinkConfig {
                                name: "".into(),
                                x: -2,
                                y: 0,
                            },
                            ProcessorLinkConfig {
                                name: "".into(),
                                x: -1,
                                y: 0,
                            },
                        ],
                    },
                    &builder,
                ),
            ]
            .map(|v| v.unwrap()),
        );
        let mut vm = builder.build().unwrap();

        run(&mut vm, 4, true);

        let processor = take_processor(&mut vm, (3, 0));
        assert_variables(&processor, map_iter! {
            u16str!("link0"): LValue::NULL,
            u16str!("link3"): LValue::NULL,
            u16str!("link4"): LValue::NULL,
            u16str!("link6"): LValue::NULL,
        });
        assert_variables_buildings(&processor, map_iter! {
            u16str!("link1"): PackedPoint2 { x: 0, y: 0 },
            u16str!("link2"): PackedPoint2 { x: 1, y: 0 },
            u16str!("link5"): PackedPoint2 { x: 2, y: 0 },
        });
    }

    #[test]
    fn test_set_link_names() {
        let mut builder = LogicVMBuilder::new();
        builder.add_buildings(
            [
                Building::from_processor_config(
                    MICRO_PROCESSOR,
                    PackedPoint2 { x: 0, y: 0 },
                    &ProcessorConfig::from_code("stop"),
                    &builder,
                ),
                Building::from_processor_config(
                    MICRO_PROCESSOR,
                    PackedPoint2 { x: 1, y: 0 },
                    &ProcessorConfig::from_code("stop"),
                    &builder,
                ),
                Building::from_processor_config(
                    MICRO_PROCESSOR,
                    PackedPoint2 { x: 2, y: 0 },
                    &ProcessorConfig::from_code("stop"),
                    &builder,
                ),
                Building::from_processor_config(
                    MICRO_PROCESSOR,
                    PackedPoint2 { x: 3, y: 0 },
                    &ProcessorConfig::from_code("stop"),
                    &builder,
                ),
                Building::from_config(
                    MEMORY_CELL,
                    PackedPoint2 { x: 4, y: 0 },
                    &Object::Null,
                    &builder,
                ),
                Building::from_processor_config(
                    MICRO_PROCESSOR,
                    PackedPoint2 { x: 5, y: 0 },
                    &ProcessorConfig {
                        code: "stop".into(),
                        links: vec![
                            ProcessorLinkConfig {
                                name: "processor1".into(),
                                x: -5,
                                y: 0,
                            },
                            ProcessorLinkConfig {
                                name: "processor1".into(),
                                x: -4,
                                y: 0,
                            },
                            ProcessorLinkConfig {
                                name: "processor10".into(),
                                x: -3,
                                y: 0,
                            },
                            ProcessorLinkConfig {
                                name: "".into(),
                                x: -2,
                                y: 0,
                            },
                            ProcessorLinkConfig {
                                name: "cellFoo".into(),
                                x: -1,
                                y: 0,
                            },
                        ],
                    },
                    &builder,
                ),
            ]
            .map(|v| v.unwrap()),
        );
        let mut vm = builder.build().unwrap();

        let processor = take_processor(&mut vm, (5, 0));
        assert_locals(&processor, map_iter! {
            u16str!("processor3"): None,
            u16str!("cell1"): None,
        });
        assert_locals_buildings(&processor, map_iter! {
            // conflicts should prefer the last building linked
            u16str!("processor1"): PackedPoint2 { x: 1, y: 0 },
            u16str!("processor2"): PackedPoint2 { x: 3, y: 0 },
            u16str!("processor10"): PackedPoint2 { x: 2, y: 0 },
            u16str!("cellFoo"): PackedPoint2 { x: 4, y: 0 },
        });
    }

    #[test]
    fn test_link_max_range() {
        let data = include_bytes!("../../tests/vm/test_link_max_range.msch");
        let schematic = Schematic::read(&mut Cursor::new(data)).unwrap();
        let mut vm = LogicVM::from_schematic(&schematic).unwrap();

        let processor = take_processor(&mut vm, (0, 0));
        assert_locals_buildings(&processor, map_iter! {
            u16str!("cell1"): PackedPoint2 { x: 0, y: 10 },
            u16str!("cell2"): PackedPoint2 { x: 7, y: 7 },
            u16str!("cell3"): PackedPoint2 { x: 9, y: 5 },
            u16str!("bank1"): PackedPoint2 { x: 10, y: 2 },
        });
    }

    #[test]
    fn test_link_out_of_range() {
        let mut builder = LogicVMBuilder::new();
        builder.add_buildings(
            [
                Building::from_processor_config(
                    MICRO_PROCESSOR,
                    PackedPoint2 { x: 1, y: 1 },
                    &ProcessorConfig {
                        code: "stop".into(),
                        links: vec![
                            ProcessorLinkConfig {
                                name: "cell1".into(),
                                x: 0,
                                y: 10,
                            },
                            ProcessorLinkConfig {
                                name: "cell2".into(),
                                x: 0,
                                y: 11,
                            },
                            ProcessorLinkConfig {
                                name: "bank1".into(),
                                x: 6,
                                y: 9,
                            },
                            ProcessorLinkConfig {
                                name: "cell3".into(),
                                x: 7,
                                y: 7,
                            },
                            ProcessorLinkConfig {
                                name: "cell4".into(),
                                x: 8,
                                y: 8,
                            },
                            ProcessorLinkConfig {
                                name: "cell5".into(),
                                x: 8,
                                y: 7,
                            },
                            ProcessorLinkConfig {
                                name: "cell6".into(),
                                x: 9,
                                y: 5,
                            },
                            ProcessorLinkConfig {
                                name: "bank2".into(),
                                x: 10,
                                y: 2,
                            },
                        ],
                    },
                    &builder,
                ),
                Building::from_config(
                    MEMORY_CELL,
                    PackedPoint2 { x: 1, y: 11 },
                    &Object::Null,
                    &builder,
                ),
                Building::from_config(
                    MEMORY_CELL,
                    PackedPoint2 { x: 1, y: 12 },
                    &Object::Null,
                    &builder,
                ),
                Building::from_config(
                    MEMORY_BANK,
                    PackedPoint2 { x: 7, y: 10 },
                    &Object::Null,
                    &builder,
                ),
                Building::from_config(
                    MEMORY_CELL,
                    PackedPoint2 { x: 8, y: 8 },
                    &Object::Null,
                    &builder,
                ),
                Building::from_config(
                    MEMORY_CELL,
                    PackedPoint2 { x: 9, y: 9 },
                    &Object::Null,
                    &builder,
                ),
                Building::from_config(
                    MEMORY_CELL,
                    PackedPoint2 { x: 9, y: 8 },
                    &Object::Null,
                    &builder,
                ),
                Building::from_config(
                    MEMORY_CELL,
                    PackedPoint2 { x: 10, y: 6 },
                    &Object::Null,
                    &builder,
                ),
                Building::from_config(
                    MEMORY_BANK,
                    PackedPoint2 { x: 11, y: 3 },
                    &Object::Null,
                    &builder,
                ),
            ]
            .map(|v| v.unwrap()),
        );
        let mut vm = builder.build().unwrap();

        let processor = take_processor(&mut vm, (1, 1));
        assert_locals(&processor, map_iter! {
            u16str!("cell2"): None,
            u16str!("bank1"): None,
            u16str!("cell4"): None,
            u16str!("cell5"): None,
        });
        assert_locals_buildings(&processor, map_iter! {
            u16str!("cell1"): PackedPoint2 { x: 1, y: 11 },
            u16str!("cell3"): PackedPoint2 { x: 8, y: 8 },
            u16str!("cell6"): PackedPoint2 { x: 10, y: 6 },
            u16str!("bank2"): PackedPoint2 { x: 11, y: 3 },
        });
    }

    #[test]
    fn test_printflush() {
        let mut builder = LogicVMBuilder::new();
        builder.add_buildings(
            [
                Building::from_processor_config(
                    MICRO_PROCESSOR,
                    PackedPoint2 { x: 0, y: 0 },
                    &ProcessorConfig {
                        code: format!(
                            r#"
                            print "bar"
                            printflush message1

                            print "baz"
                            printflush message1

                            print "{max_length}"
                            printflush message1

                            printflush message1
                            noop

                            print "{too_long}"
                            printflush message1

                            print "discarded"
                            printflush null

                            print "discarded"
                            printflush @this

                            stop
                            "#,
                            max_length = "a".repeat(400),
                            too_long = "b".repeat(401),
                        ),
                        links: vec![ProcessorLinkConfig::unnamed(1, 0)],
                    },
                    &builder,
                ),
                Building::from_config(
                    MESSAGE,
                    PackedPoint2 { x: 1, y: 0 },
                    &Object::String(Some("foo".into())),
                    &builder,
                ),
            ]
            .map(|v| v.unwrap()),
        );
        let mut vm = builder.build().unwrap();

        fn with_message(vm: &LogicVM, f: impl FnOnce(&U16Str)) {
            let data = vm.building((1, 0).into()).unwrap().data.borrow();
            let BuildingData::Message(buf) = &*data else {
                panic!("expected Message, got {}", <&str>::from(&*data));
            };
            f(buf);
        }

        // initial state
        with_message(&vm, |buf| {
            assert_eq!(buf, u16str!("foo"));
        });

        // print "bar"
        vm.do_tick(Duration::ZERO);
        with_message(&vm, |buf| {
            assert_eq!(buf, u16str!("bar"));
        });
        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.printbuffer, u16str!(""));
        });

        // print "baz"
        vm.do_tick(Duration::ZERO);
        with_message(&vm, |buf| {
            assert_eq!(buf, u16str!("baz"));
        });
        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.printbuffer, u16str!(""));
        });

        // print "{max_length}"
        vm.do_tick(Duration::ZERO);
        with_message(&vm, |buf| {
            assert_eq!(buf.len(), 400);
            assert_eq!(buf[0..1], u16str!("a"));
        });
        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.printbuffer, u16str!(""));
        });

        // empty printflush
        vm.do_tick(Duration::ZERO);
        with_message(&vm, |buf| {
            assert_eq!(buf, u16str!(""));
        });
        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.printbuffer, u16str!(""));
        });

        // print "{too_long}"
        vm.do_tick(Duration::ZERO);
        with_message(&vm, |buf| {
            assert_eq!(buf.len(), 400);
            assert_eq!(buf[0..1], u16str!("b"));
        });
        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.printbuffer, u16str!(""));
        });

        // printflush null
        vm.do_tick(Duration::ZERO);
        with_message(&vm, |buf| {
            assert_eq!(buf.len(), 400);
            assert_eq!(buf[0..1], u16str!("b"));
        });
        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.printbuffer, u16str!(""));
        });

        // printflush @this
        vm.do_tick(Duration::ZERO);
        with_message(&vm, |buf| {
            assert_eq!(buf.len(), 400);
            assert_eq!(buf[0..1], u16str!("b"));
        });
        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.printbuffer, u16str!(""));
        });
    }

    #[test]
    fn test_getlink() {
        let mut builder = LogicVMBuilder::new();
        builder.add_buildings(
            [
                Building::from_processor_config(
                    HYPER_PROCESSOR,
                    PackedPoint2 { x: 0, y: 0 },
                    &ProcessorConfig {
                        code: "
                        getlink link_-1 -1
                        getlink link_null null
                        getlink link_0 0
                        getlink link_1 1
                        getlink link_2 2
                        getlink link_3 3
                        stop
                        "
                        .into(),
                        links: vec![
                            ProcessorLinkConfig::unnamed(3, 0),
                            ProcessorLinkConfig::unnamed(4, 0),
                            ProcessorLinkConfig::unnamed(5, 0),
                        ],
                    },
                    &builder,
                ),
                Building::from_config(SWITCH, PackedPoint2 { x: 3, y: 0 }, &Object::Null, &builder),
                Building::from_config(SWITCH, PackedPoint2 { x: 4, y: 0 }, &Object::Null, &builder),
                Building::from_config(SWITCH, PackedPoint2 { x: 5, y: 0 }, &Object::Null, &builder),
            ]
            .map(|v| v.unwrap()),
        );
        let mut vm = builder.build().unwrap();

        run(&mut vm, 1, true);

        let processor = take_processor(&mut vm, (0, 0));
        assert_variables(&processor, map_iter! {
            u16str!("link_-1"): LValue::NULL,
            u16str!("link_3"): LValue::NULL,
        });
        assert_variables_buildings(&processor, map_iter! {
            u16str!("link_null"): PackedPoint2 { x: 3, y: 0 },
            u16str!("link_0"): PackedPoint2 { x: 3, y: 0 },
            u16str!("link_1"): PackedPoint2 { x: 4, y: 0 },
            u16str!("link_2"): PackedPoint2 { x: 5, y: 0 },
        });
    }

    #[test]
    fn test_overlap() {
        let mut builder = LogicVMBuilder::new();
        builder.add_buildings(
            [
                Building::from_processor_config(
                    HYPER_PROCESSOR,
                    PackedPoint2 { x: 0, y: 0 },
                    &ProcessorConfig::default(),
                    &builder,
                ),
                Building::from_config(SWITCH, PackedPoint2 { x: 2, y: 2 }, &Object::Null, &builder),
            ]
            .map(|v| v.unwrap()),
        );

        let Err(err) = builder.build() else {
            panic!("did not return error");
        };

        assert!(
            matches!(err, VMLoadError::Overlap {
                position: PackedPoint2 { x: 2, y: 2 },
                current,
            } if current == content::blocks::FROM_NAME["hyper-processor"]),
            "{err:?}"
        );
    }

    #[test]
    fn test_getblock() {
        let mut builder = LogicVMBuilder::new();
        builder.add_buildings(
            [
                Building::from_processor_config(
                    WORLD_PROCESSOR,
                    PackedPoint2 { x: 1, y: 2 },
                    &ProcessorConfig::from_code(
                        "
                        getblock floor floor1 @thisx @thisy
                        getblock ore ore1 @thisx @thisy
                        getblock block block1 @thisx @thisy
                        getblock building building1 @thisx @thisy

                        getblock floor floor2 1 3
                        getblock ore ore2 1 3
                        getblock block block2 1 3
                        getblock building building2 1 3

                        getblock floor floor3 1 1
                        getblock ore ore3 1 1
                        getblock block block3 1 1
                        getblock building building3 1 1

                        stop
                        ",
                    ),
                    &builder,
                ),
                Building::from_config(SWITCH, PackedPoint2 { x: 1, y: 3 }, &Object::Null, &builder),
            ]
            .map(|v| v.unwrap()),
        );
        let mut vm = builder.build().unwrap();

        run(&mut vm, 2, true);

        let processor = take_processor(&mut vm, (1, 2));
        assert_variables(&processor, map_iter! {
            u16str!("floor1"): LValue::from(Content::Block(&content::blocks::STONE)),
            u16str!("ore1"): LValue::from(Content::Block(&content::blocks::AIR)),
            u16str!("block1"): LValue::from(Content::Block(content::blocks::FROM_NAME["world-processor"])),

            u16str!("floor2"): LValue::from(Content::Block(&content::blocks::STONE)),
            u16str!("ore2"): LValue::from(Content::Block(&content::blocks::AIR)),
            u16str!("block2"): LValue::from(Content::Block(content::blocks::FROM_NAME["switch"])),

            u16str!("floor3"): LValue::NULL,
            u16str!("ore3"): LValue::NULL,
            u16str!("block3"): LValue::NULL,
            u16str!("building3"): LValue::NULL,
        });
        assert_variables_buildings(&processor, map_iter! {
            u16str!("building1"): PackedPoint2 { x: 1, y: 2 },
            u16str!("building2"): PackedPoint2 { x: 1, y: 3 },
        });
    }

    thread_local! {
        static SENSOR_TESTS: Vec<(U16String, &'static str, &'static str, LValue)> = map_iter_from![
            ("null", "@dead"): true,

            ("@hyper-processor", "@name"): u16str!("hyper-processor"),
            ("@hyper-processor", "@id"): 141,
            ("@hyper-processor", "@size"): 3,

            ("@titanium", "@name"): u16str!("titanium"),
            ("@titanium", "@id"): 6,

            ("@cryofluid", "@name"): u16str!("cryofluid"),
            ("@cryofluid", "@id"): 3,

            ("@flare", "@name"): u16str!("flare"),
            ("@flare", "@id"): 15,

            (r#""123456789""#, "@size"): 9,

            ("@sharded", "@id"): 1,

            ("1", "@dead"): LValue::NULL,

            ("@this", "@enabled"): true,
            ("@this", "@config"): LValue::NULL,
            ("@this", "@dead"): false,
            ("@this", "@x"): 0,
            ("@this", "@y"): 0,
            ("@this", "@size"): 1,
            ("@this", "@type"): Content::Block(content::blocks::FROM_NAME[WORLD_PROCESSOR]),

            ("processor1", "@enabled"): false,
            ("processor1", "@config"): LValue::NULL,
            ("processor1", "@dead"): false,
            ("processor1", "@x"): 1,
            ("processor1", "@y"): 0,
            ("processor1", "@size"): 1,
            ("processor1", "@type"): Content::Block(content::blocks::FROM_NAME[MICRO_PROCESSOR]),

            ("processor2", "@enabled"): true,

            ("cell1", "@enabled"): true,
            ("cell1", "@config"): LValue::NULL,
            ("cell1", "@memoryCapacity"): 64,

            ("cell2", "@enabled"): true,
            ("cell2", "@memoryCapacity"): 512,

            ("message1", "@enabled"): true,
            ("message1", "@config"): LValue::NULL,
            ("message1", "@bufferSize"): 3,

            ("switch1", "@enabled"): false,
            ("switch1", "@config"): LValue::NULL,

            ("switch2", "@enabled"): true,

            ("sorter1", "@enabled"): true,
            ("sorter1", "@config"): Content::Item(content::items::FROM_NAME["graphite"]),
            ("sorter1", "@graphite"): LValue::NULL,
        ]
        .map(|((target, sensor), want)| (u16format!("_{target}_{sensor}"), target, sensor, want))
        .collect();
    }

    #[test]
    fn test_sensor() {
        SENSOR_TESTS.with(|tests| {
            let code = tests
                .iter()
                .map(|(var, target, sensor, _)| {
                    format!("sensor {} {target} {sensor}", var.display())
                })
                .join("\n");
            let code = format!("setrate 1000\n{code}\nstop");

            let mut builder = LogicVMBuilder::new();
            builder.add_buildings(
                [
                    Building::from_processor_config(
                        WORLD_PROCESSOR,
                        PackedPoint2 { x: 0, y: 0 },
                        &ProcessorConfig {
                            code,
                            links: vec![
                                ProcessorLinkConfig::unnamed(1, 0),
                                ProcessorLinkConfig::unnamed(1, 1),
                                ProcessorLinkConfig::unnamed(2, 0),
                                ProcessorLinkConfig::unnamed(2, 1),
                                ProcessorLinkConfig::unnamed(3, 0),
                                ProcessorLinkConfig::unnamed(4, 0),
                                ProcessorLinkConfig::unnamed(4, 1),
                                ProcessorLinkConfig::unnamed(5, 0),
                            ],
                        },
                        &builder,
                    ),
                    Building::from_processor_config(
                        MICRO_PROCESSOR,
                        PackedPoint2 { x: 1, y: 0 },
                        &ProcessorConfig::default(),
                        &builder,
                    ),
                    Building::from_processor_config(
                        MICRO_PROCESSOR,
                        PackedPoint2 { x: 1, y: 1 },
                        &ProcessorConfig::from_code("wait 0; wait 0; stop"),
                        &builder,
                    ),
                    Building::from_config(
                        MEMORY_CELL,
                        PackedPoint2 { x: 2, y: 0 },
                        &Object::Null,
                        &builder,
                    ),
                    Building::from_config(
                        WORLD_CELL,
                        PackedPoint2 { x: 2, y: 1 },
                        &Object::Null,
                        &builder,
                    ),
                    Building::from_config(
                        MESSAGE,
                        PackedPoint2 { x: 3, y: 0 },
                        &Object::String(Some("foo".into())),
                        &builder,
                    ),
                    Building::from_config(
                        SWITCH,
                        PackedPoint2 { x: 4, y: 0 },
                        &false.into(),
                        &builder,
                    ),
                    Building::from_config(
                        SWITCH,
                        PackedPoint2 { x: 4, y: 1 },
                        &true.into(),
                        &builder,
                    ),
                    Building::from_config(
                        "sorter",
                        PackedPoint2 { x: 5, y: 0 },
                        &Object::Content(ContentID {
                            type_: ContentType::Item,
                            id: content::items::FROM_NAME["graphite"].id as i16,
                        }),
                        &builder,
                    ),
                ]
                .map(|v| v.unwrap()),
            );
            let mut vm = builder.build().unwrap();

            run(&mut vm, 2, true);

            let processor = take_processor(&mut vm, (0, 0));
            assert_variables(
                &processor,
                tests
                    .iter()
                    .map(|(var, _, _, want)| (var.as_ustr(), want.clone())),
            );
        });
    }

    #[test]
    fn test_sensor_schematic() {
        SENSOR_TESTS.with(|tests| {
            let code = tests
                .iter()
                .map(|(var, target, sensor, _)| {
                    format!("sensor {} {target} {sensor}", var.display())
                })
                .join("\n");
            let code = format!("setrate 1000\n{code}\nstop");

            let data = include_bytes!("../../tests/vm/test_sensor_schematic.msch");
            let mut schematic = Schematic::read(&mut Cursor::new(data)).unwrap();

            // replace main processor code
            let tile = schematic.tile_mut(0).unwrap();
            assert_eq!(tile.block, "world-processor");
            let mut config = ProcessorConfig::parse(&tile.config).unwrap();
            config.code = code;
            match &mut tile.config {
                Object::ByteArray { values } => {
                    values.clear();
                    config.write(&mut Cursor::new(values)).unwrap();
                }
                _ => unreachable!(),
            };

            let mut vm = LogicVM::from_schematic(&schematic).unwrap();

            run(&mut vm, 2, true);

            let processor = take_processor(&mut vm, (0, 0));
            assert_variables(
                &processor,
                tests
                    .iter()
                    .map(|(var, _, _, want)| (var.as_ustr(), want.clone())),
            );
        });
    }

    #[test]
    fn test_sensor_invalid() {
        let mut vm = single_processor_vm(
            LOGIC_PROCESSOR,
            r#"
            set canary1 0xdeadbeef
            sensor canary1 @this 1

            set canary2 0xdeadbeef
            sensor canary2 "foo" 1

            stop
            "#,
        );

        run(&mut vm, 1, true);

        let processor = take_processor(&mut vm, (0, 0));
        assert_variables(&processor, map_iter! {
            u16str!("canary1"): LValue::from(0xdeadbeefu32 as f64),
            u16str!("canary2"): LValue::NULL,
        });
    }

    #[test]
    fn test_control() {
        let mut builder = LogicVMBuilder::new();
        builder.add_buildings(
            [
                Building::from_processor_config(
                    WORLD_PROCESSOR,
                    PackedPoint2 { x: 0, y: 0 },
                    &ProcessorConfig {
                        code: "
                        getblock building unlinked 4 0

                        control enabled processor1 false
                        control enabled switch1 true
                        control enabled cell1 false
                        control enabled unlinked false

                        sensor got1 processor1 @enabled
                        sensor got2 switch1 @enabled
                        sensor got3 cell1 @enabled
                        sensor got4 unlinked @enabled

                        stop
                        "
                        .into(),
                        links: vec![
                            ProcessorLinkConfig::unnamed(1, 0),
                            ProcessorLinkConfig::unnamed(2, 0),
                            ProcessorLinkConfig::unnamed(3, 0),
                        ],
                    },
                    &builder,
                ),
                Building::from_processor_config(
                    MICRO_PROCESSOR,
                    PackedPoint2 { x: 1, y: 0 },
                    &ProcessorConfig::from_code("noop"),
                    &builder,
                ),
                Building::from_config(SWITCH, PackedPoint2 { x: 2, y: 0 }, &false.into(), &builder),
                Building::from_config(
                    MEMORY_CELL,
                    PackedPoint2 { x: 3, y: 0 },
                    &Object::Null,
                    &builder,
                ),
                Building::from_config(SWITCH, PackedPoint2 { x: 4, y: 0 }, &true.into(), &builder),
            ]
            .map(|v| v.unwrap()),
        );
        let mut vm = builder.build().unwrap();

        run(&mut vm, 2, true);

        let processor = take_processor(&mut vm, (0, 0));
        assert_variables(&processor, map_iter! {
            u16str!("got1"): LValue::from(0.),
            u16str!("got2"): LValue::from(1.),
            u16str!("got3"): LValue::from(1.),
            u16str!("got4"): LValue::from(0.),
        });
    }

    #[test]
    fn test_read() {
        let mut builder = LogicVMBuilder::new();
        builder.add_buildings(
            [
                Building::from_processor_config(
                    LOGIC_PROCESSOR,
                    PackedPoint2 { x: 0, y: 0 },
                    &ProcessorConfig {
                        code: r#"
                        set number 10
                        set string "abc"
                        set building cell1
                        stop
                        "#
                        .into(),
                        links: vec![ProcessorLinkConfig::unnamed(0, 2)],
                    },
                    &builder,
                ),
                Building::from_config(
                    MEMORY_CELL,
                    PackedPoint2 { x: 0, y: 2 },
                    &Object::Null,
                    &builder,
                ),
                Building::from_processor_config(
                    HYPER_PROCESSOR,
                    PackedPoint2 { x: 2, y: 0 },
                    &ProcessorConfig {
                        code: r#"
                        read processor_number processor1 "number"
                        read processor_string processor1 "string"
                        read processor_building processor1 "building"
                        read processor_counter processor1 "@counter"
                        set processor_ipt "overwritten"
                        read processor_ipt processor1 "@ipt"
                        set processor_this "overwritten"
                        read processor_this processor1 "@this"
                        set processor_undefined "overwritten"
                        read processor_undefined processor1 "undefined"
                        set processor_0 "preserved"
                        read processor_0 processor1 0

                        read processor_building_0 processor_building 0
                        read processor_building_63 processor_building 63

                        set cell_-1 "overwritten"
                        read cell_-1 cell1 -1
                        read cell_0 cell1 0
                        set cell_str "overwritten"
                        read cell_str cell1 "a"
                        read cell_63 cell1 63
                        set cell_64 "overwritten"
                        read cell_64 cell1 64
                        
                        set message_-1 "overwritten"
                        read message_-1 message1 -1
                        read message_0 message1 0
                        set message_str "overwritten"
                        read message_str message1 "a"
                        read message_2 message1 2
                        set message_3 "overwritten"
                        read message_3 message1 3

                        set switch "preserved"
                        read switch switch1 0

                        stop
                        "#
                        .into(),
                        links: vec![
                            ProcessorLinkConfig::unnamed(-1, 0),
                            ProcessorLinkConfig::unnamed(3, 0),
                            ProcessorLinkConfig::unnamed(4, 0),
                            ProcessorLinkConfig::unnamed(5, 0),
                        ],
                    },
                    &builder,
                ),
                Building::from_config(
                    MEMORY_CELL,
                    PackedPoint2 { x: 5, y: 0 },
                    &Object::Null,
                    &builder,
                ),
                Building::from_config(
                    MESSAGE,
                    PackedPoint2 { x: 6, y: 0 },
                    &Object::String(Some("def".into())),
                    &builder,
                ),
                Building::from_config(SWITCH, PackedPoint2 { x: 7, y: 0 }, &true.into(), &builder),
            ]
            .map(|v| v.unwrap()),
        );
        let mut vm = builder.build().unwrap();

        if let Some(building) = vm.building(PackedPoint2 { x: 0, y: 2 })
            && let BuildingData::Memory(memory) = &mut *building.data.borrow_mut()
        {
            memory[63] = 20.;
        } else {
            panic!("unexpected building");
        }

        if let Some(building) = vm.building(PackedPoint2 { x: 5, y: 0 })
            && let BuildingData::Memory(memory) = &mut *building.data.borrow_mut()
        {
            memory[0] = 30.;
            memory[1] = 40.;
            memory[63] = 50.;
        } else {
            panic!("unexpected building");
        }

        run(&mut vm, 2, true);

        let processor = take_processor(&mut vm, (2, 0));
        assert_variables(&processor, map_iter! {
            u16str!("processor_number"): LValue::from(10.),
            u16str!("processor_string"): u16str!("abc").into(),
            u16str!("processor_counter"): LValue::from(3.),
            u16str!("processor_ipt"): LValue::NULL,
            u16str!("processor_this"): LValue::NULL,
            u16str!("processor_undefined"): LValue::NULL,
            u16str!("processor_0"): u16str!("preserved").into(),

            u16str!("processor_building_0"): LValue::from(0.),
            u16str!("processor_building_63"): LValue::from(20.),

            u16str!("cell_-1"): LValue::NULL,
            u16str!("cell_0"): LValue::from(30.),
            u16str!("cell_str"): LValue::from(40.),
            u16str!("cell_63"): LValue::from(50.),
            u16str!("cell_64"): LValue::NULL,

            u16str!("message_-1"): LValue::NULL,
            u16str!("message_0"): LValue::from(b'd' as f64),
            u16str!("message_str"): LValue::from(b'e' as f64),
            u16str!("message_2"): LValue::from(b'f' as f64),
            u16str!("message_3"): LValue::NULL,

            u16str!("switch"): u16str!("preserved").into(),
        });
        assert_variables_buildings(&processor, map_iter! {
            u16str!("processor_building"): PackedPoint2 { x: 0, y: 2 },
        });
    }

    #[test]
    fn test_write() {
        let mut builder = LogicVMBuilder::new();
        builder.add_buildings(
            [
                Building::from_processor_config(
                    LOGIC_PROCESSOR,
                    PackedPoint2 { x: 0, y: 0 },
                    &ProcessorConfig {
                        code: r#"
                        # if the world proc runs before this one, canary won't be set
                        set canary 0xdeadbeef
                        set var 0
                        stop

                        set jumped true
                        stop
                        "#
                        .into(),
                        links: vec![ProcessorLinkConfig::unnamed(0, 2)],
                    },
                    &builder,
                ),
                Building::from_config(
                    MEMORY_CELL,
                    PackedPoint2 { x: 0, y: 2 },
                    &Object::Null,
                    &builder,
                ),
                Building::from_config(
                    WORLD_CELL,
                    PackedPoint2 { x: 0, y: 3 },
                    &Object::Null,
                    &builder,
                ),
                Building::from_processor_config(
                    WORLD_PROCESSOR,
                    PackedPoint2 { x: 2, y: 0 },
                    &ProcessorConfig {
                        code: r#"
                        setrate 1000

                        write 10 processor1 "var"
                        write 3 processor1 "@counter"
                        write null processor1 0
                        write "discarded1" processor1 "undefined"
                        
                        set var 0
                        write 20 @this "var"
                        write "discarded2" @this "undefined"

                        write 30 cell1 0
                        write 40 cell1 "a"
                        write "b" cell1 2
                        write 50 cell1 63
                        write -1 cell1 -1
                        write -1 cell1 64

                        write 60 cell2 0
                        write 70 cell2 511

                        stop
                        "#
                        .into(),
                        links: vec![
                            ProcessorLinkConfig::unnamed(-2, 0),
                            ProcessorLinkConfig::unnamed(-2, 2),
                            ProcessorLinkConfig::unnamed(-2, 3),
                        ],
                    },
                    &builder,
                ),
            ]
            .map(|v| v.unwrap()),
        );
        let mut vm = builder.build().unwrap();

        run(&mut vm, 2, true);

        let processor = take_processor(&mut vm, (0, 0));
        assert_eq!(processor.state.counter, 4);
        assert_variables(&processor, map_iter! {
            u16str!("canary"): Some(LValue::from(0xdeadbeefu32 as f64)),
            u16str!("var"): Some(LValue::from(10.)),
            u16str!("jumped"): Some(LValue::from(1.)),
            u16str!("undefined"): None,
        });

        let processor = take_processor(&mut vm, (2, 0));
        assert_variables(&processor, map_iter! {
            u16str!("var"): Some(LValue::from(20.)),
            u16str!("undefined"): None,
        });

        if let Some(building) = vm.building(PackedPoint2 { x: 0, y: 2 })
            && let BuildingData::Memory(memory) = &mut *building.data.borrow_mut()
        {
            assert_eq!(memory[0], 30.);
            assert_eq!(memory[1], 40.);
            assert_eq!(memory[2], 1.);
            assert_eq!(memory[63], 50.);
        } else {
            panic!("unexpected building");
        }

        if let Some(building) = vm.building(PackedPoint2 { x: 0, y: 3 })
            && let BuildingData::Memory(memory) = &mut *building.data.borrow_mut()
        {
            assert_eq!(memory[0], 60.);
            assert_eq!(memory[511], 70.);
        } else {
            panic!("unexpected building");
        }
    }

    #[test]
    fn test_max_ticks() {
        let mut vm = single_processor_vm(
            MICRO_PROCESSOR,
            "
            noop
            noop
            stop
            ",
        );

        run(&mut vm, 1, false);

        let processor = take_processor(&mut vm, (0, 0));
        assert_eq!(processor.state.counter, 2);
        assert!(!processor.state.stopped());
    }

    #[test]
    fn test_print() {
        let mut vm = single_processor_vm(
            HYPER_PROCESSOR,
            r#"
            print "foo"
            print "bar\n"
            print 10
            print "\n"
            print 1.5
            print null
            print foo
            print "♥"
            stop
            "#,
        );

        run(&mut vm, 1, true);

        let processor = take_processor(&mut vm, (0, 0));
        assert_eq!(
            processor.state.printbuffer,
            u16str!("foobar\n10\n1.5nullnull♥")
        );
        assert_eq!(
            *processor.state.printbuffer.as_vec(),
            "foobar\n10\n1.5nullnull"
                .bytes()
                .map(|b| b as u16)
                .chain([0x2665])
                .collect_vec()
        );
    }

    #[test]
    fn test_end() {
        let mut vm = single_processor_vm(
            MICRO_PROCESSOR,
            "
            print 1
            end
            print 2
            ",
        );

        run(&mut vm, 2, false);

        let processor = take_processor(&mut vm, (0, 0));
        assert_eq!(processor.state.counter, 3);
        assert!(!processor.state.stopped());
        assert_eq!(processor.state.printbuffer, u16str!("11"));
    }

    #[test]
    fn test_set() {
        let mut vm = single_processor_vm(
            MICRO_PROCESSOR,
            r#"
            set foo 1
            noop
            set foo 2
            set @counter 6
            stop
            stop
            set @counter null
            set @counter "foo"
            set @ipt 10
            set true 0
            set pi @pi
            set pi_fancy π
            set e @e
            noop
            set a 1e308
            set b 1e309
            set c -1e308
            set d -1e309
            "#,
        );

        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.variables[u16str!("foo")], LValue::NULL);
        });

        vm.do_tick(Duration::ZERO);

        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.variables[u16str!("foo")], LValue::from(1.));
        });

        vm.do_tick(Duration::ZERO);

        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.variables[u16str!("foo")], LValue::from(2.));
            assert_eq!(p.state.counter, 6);
        });

        vm.do_tick(Duration::ZERO);

        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.counter, 8);
        });

        vm.do_tick(Duration::ZERO);
        vm.do_tick(Duration::ZERO);
        vm.do_tick(Duration::ZERO);

        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.variables.get(u16str!("@ipt")), None);
            assert_eq!(p.state.variables.get(u16str!("true")), None);
            assert_eq!(
                p.state.variables[u16str!("pi")],
                LValue::from(variables::PI)
            );
            assert_eq!(
                p.state.variables[u16str!("pi_fancy")],
                LValue::from(variables::PI)
            );
            assert_eq!(p.state.variables[u16str!("e")], LValue::from(variables::E));
        });

        vm.do_tick(Duration::ZERO);
        vm.do_tick(Duration::ZERO);

        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.variables[u16str!("a")], LValue::from(1e308));
            assert_eq!(p.state.variables[u16str!("b")], LValue::NULL);
            assert_eq!(p.state.variables[u16str!("c")], LValue::from(-1e308));
            assert_eq!(p.state.variables[u16str!("d")], LValue::NULL);
        });
    }

    #[test]
    fn test_setrate() {
        let mut vm = single_processor_vm(
            WORLD_PROCESSOR,
            "
            noop
            setrate 0
            stop
            setrate 10
            stop
            setrate 1
            stop
            setrate 1001
            stop
            setrate 500
            stop
            setrate 1000
            stop
            setrate 5.5
            stop
            ",
        );

        for ipt in [8, 1, 10, 1, 1000, 500, 1000, 5] {
            with_processor(&mut vm, (0, 0), |p| {
                assert_eq!(
                    p.state.ipt, ipt as f64,
                    "incorrect ipt at counter {}",
                    p.state.counter
                );
                p.state.counter += 1;
                p.state.set_stopped(false);
            });

            vm.do_tick(Duration::ZERO);
        }
    }

    #[test]
    fn test_setrate_unpriv() {
        let mut vm = single_processor_vm(MICRO_PROCESSOR, "setrate 10; stop");
        run(&mut vm, 1, true);
        let processor = take_processor(&mut vm, (0, 0));
        assert_eq!(processor.state.ipt, 2.0);
    }

    const CONDITION_TESTS: &[(&str, &str, &str, bool)] = &[
        // equal
        ("equal", "0", "0", true),
        ("equal", "0", "null", true),
        ("equal", "1", r#""""#, true),
        ("equal", "1", r#""foo""#, true),
        ("equal", r#""""#, r#""""#, true),
        ("equal", r#""abc""#, r#""abc""#, true),
        ("equal", "null", "null", true),
        ("equal", "0", "0.0000009", true),
        ("equal", "@pi", "3.1415927", true),
        ("equal", "π", "3.1415927", true),
        ("equal", "@e", "2.7182818", true),
        ("equal", "0", "0.000001", false),
        ("equal", "0", "1", false),
        ("equal", "1", "null", false),
        ("equal", r#""abc""#, r#""def""#, false),
        // notEqual
        ("notEqual", "0", "0", false),
        ("notEqual", "0", "null", false),
        ("notEqual", "null", "null", false),
        ("notEqual", "0", "0.0000009", false),
        ("notEqual", "0", "0.000001", true),
        ("notEqual", "0", "1", true),
        ("notEqual", "1", "null", true),
        // lessThan
        ("lessThan", "0", "1", true),
        ("lessThan", "0", "0", false),
        ("lessThan", "1", "0", false),
        // lessThanEq
        ("lessThanEq", "0", "1", true),
        ("lessThanEq", "0", "0", true),
        ("lessThanEq", "1", "0", false),
        // greaterThan
        ("greaterThan", "0", "1", false),
        ("greaterThan", "0", "0", false),
        ("greaterThan", "1", "0", true),
        // greaterThanEq
        ("greaterThanEq", "0", "1", false),
        ("greaterThanEq", "0", "0", true),
        ("greaterThanEq", "1", "0", true),
        // strictEqual
        ("strictEqual", "0", "0", true),
        ("strictEqual", "0.5", "0.5", true),
        ("strictEqual", "null", "null", true),
        ("strictEqual", r#""""#, r#""""#, true),
        ("strictEqual", r#""abc""#, r#""abc""#, true),
        ("strictEqual", "0", "null", false),
        ("strictEqual", "1", r#""""#, false),
        ("strictEqual", "1", r#""foo""#, false),
        ("strictEqual", r#""abc""#, r#""def""#, false),
        ("strictEqual", "0", "0.0000009", false),
        ("strictEqual", "0", "0.000001", false),
        ("strictEqual", "0", "1", false),
        ("strictEqual", "1", "null", false),
        // always
        ("always", "0", "0", true),
        ("always", "0", "1", true),
        ("always", "1", "0", true),
        ("always", "1", "1", true),
    ];

    #[test]
    fn test_jump() {
        let mut code = r#"
        setrate 1000

        # detect reentry
        set source "reentered init"
        jump oops notEqual canary null
        set source null

        set canary 0xdeadbeef

        # test 'always' manually

        jump test_always_0 always
            set canary "test_always_0 not taken: always"
            stop
        test_always_0:

        jump test_always_1 always 1
            set canary "test_always_1 not taken: always 1"
            stop
        test_always_1:
        "#
        .to_string();

        for (i, &(cond, x, y, want_jump)) in CONDITION_TESTS.iter().enumerate() {
            let err = format!("{cond} {x} {y}").replace('"', "'");
            if want_jump {
                code.push_str(&format!(
                    "
                    jump test{i}_0 {cond} {x} {y}
                        set canary \"test{i}_0 not taken: {err}\"
                        stop
                    test{i}_0:

                    set x {x}
                    set y {y}
                    jump test{i}_1 {cond} x y
                        set canary \"test{i}_1 not taken: {err}\"
                        stop
                    test{i}_1:
                    "
                ));
            } else {
                code.push_str(&format!(
                    "
                    set source \"test{i}_0 taken: {err}\"
                    jump oops {cond} {x} {y}
                    set x {x}
                    set y {y}
                    set source \"test{i}_1 taken: {err}\"
                    jump oops {cond} x y
                    set source null
                    "
                ));
            }
        }

        code += "
        stop

        oops:
        set canary source
        stop
        ";

        let mut vm = single_processor_vm(WORLD_PROCESSOR, &code);

        run(&mut vm, 2, true);

        let processor = take_processor(&mut vm, (0, 0));
        assert_eq!(
            processor.state.variables[u16str!("canary")],
            LValue::from(0xdeadbeefu64 as f64)
        );
    }

    #[test]
    fn test_wait() {
        let mut vm = single_processor_vm(
            HYPER_PROCESSOR,
            "
            print 1
            wait -1

            print 2
            wait 0

            print 3
            wait 1e-5

            print 4
            wait 1

            print 5
            stop
            ",
        );

        let mut time = Duration::ZERO;
        vm.do_tick(time);

        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.printbuffer, u16str!("123"));
        });

        time += Duration::from_secs_f64(1. / 60.);
        vm.do_tick(time);

        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.printbuffer, u16str!("1234"));
        });

        time += Duration::from_millis(500);
        vm.do_tick(time);

        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.printbuffer, u16str!("1234"));
        });

        time += Duration::from_millis(500);
        vm.do_tick(time);

        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.printbuffer, u16str!("12345"));
        });
    }

    #[test]
    fn test_printchar() {
        // TODO: test full range (requires op add)
        let mut vm = single_processor_vm(
            HYPER_PROCESSOR,
            "
            printchar 0
            printchar 10
            printchar 0x41
            printchar 0xc0
            printchar 0x2665
            printchar 0xd799
            printchar 0xd800
            printchar 0xdfff
            printchar 0x8000
            printchar 0xffff
            printchar 0x10000
            printchar 0x10001
            stop
            ",
        );

        run(&mut vm, 1, true);

        let state = take_processor(&mut vm, (0, 0)).state;
        assert_eq!(state.printbuffer.as_vec(), &[
            0, 10, 0x41, 0xc0, 0x2665, 0xd799, 0xd800, 0xdfff, 0x8000, 0xffff, 0, 1
        ]);
    }

    #[test]
    fn test_format() {
        let mut vm = single_processor_vm(
            MICRO_PROCESSOR,
            r#"
            print "{0} {1} {/} {9} {:} {10} {0}"
            noop

            format 4
            noop

            format "abcde"
            noop

            format "aa"
            noop

            format ""
            noop

            format "ignored"
            stop
            "#,
        );

        vm.do_tick(Duration::ZERO);

        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.printbuffer, u16str!("{0} {1} {/} {9} {:} {10} {0}"));
        });

        vm.do_tick(Duration::ZERO);

        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.printbuffer, u16str!("4 {1} {/} {9} {:} {10} {0}"));
        });

        vm.do_tick(Duration::ZERO);

        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.printbuffer, u16str!("4 {1} {/} {9} {:} {10} abcde"));
        });

        vm.do_tick(Duration::ZERO);

        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.printbuffer, u16str!("4 aa {/} {9} {:} {10} abcde"));
        });

        vm.do_tick(Duration::ZERO);

        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.printbuffer, u16str!("4 aa {/}  {:} {10} abcde"));
        });

        vm.do_tick(Duration::ZERO);

        with_processor(&mut vm, (0, 0), |p| {
            assert_eq!(p.state.printbuffer, u16str!("4 aa {/}  {:} {10} abcde"));
        });
    }

    #[test]
    fn test_pack_unpack_color() {
        let mut vm = single_processor_vm(
            HYPER_PROCESSOR,
            "
            packcolor packed1 0 0.5 0.75 1
            unpackcolor r1 g1 b1 a1 packed1
            unpackcolor r2 g2 b2 a2 %[royal]
            packcolor packed2 r2 g2 b2 a2
            stop
            ",
        );

        run(&mut vm, 1, true);

        let variables = take_processor(&mut vm, (0, 0)).state.variables;

        assert_eq!(
            variables[u16str!("packed1")],
            LValue::from(f64::from_bits(0x00_7f_bf_ffu64))
        );

        assert_eq!(variables[u16str!("r1")], LValue::from(0.));
        assert_eq!(variables[u16str!("g1")], LValue::from(127. / 255.));
        assert_eq!(variables[u16str!("b1")], LValue::from(191. / 255.));
        assert_eq!(variables[u16str!("a1")], LValue::from(1.));

        assert_eq!(variables[u16str!("r2")], LValue::from((0x41 as f64) / 255.));
        assert_eq!(variables[u16str!("g2")], LValue::from((0x69 as f64) / 255.));
        assert_eq!(variables[u16str!("b2")], LValue::from((0xe1 as f64) / 255.));
        assert_eq!(variables[u16str!("a2")], LValue::from((0xff as f64) / 255.));

        assert_eq!(variables[u16str!("packed2")], LValue::from(COLORS["royal"]));
    }

    #[test]
    fn test_select() {
        let globals = LVar::create_global_constants();

        for &(cond, x, y, want_true) in CONDITION_TESTS {
            let mut vm = single_processor_vm_with_globals(
                HYPER_PROCESSOR,
                &format!(
                    "
                    set x {x}
                    set y {y}
                    set if_true 0xdeadbeef
                    set if_false 0xbabecafe
                    select got1 {cond} x y if_true if_false
                    select got2 {cond} {x} {y} 0xdeadbeef 0xbabecafe
                    stop
                    "
                ),
                &globals,
            );

            run(&mut vm, 1, true);

            let processor = take_processor(&mut vm, (0, 0));
            let want_value = if want_true {
                0xdeadbeefu64
            } else {
                0xbabecafeu64
            }
            .into();
            assert_eq!(
                processor.state.variables[u16str!("got1")],
                want_value,
                "{cond} {x} {y} (variables)"
            );
            assert_eq!(
                processor.state.variables[u16str!("got2")],
                want_value,
                "{cond} {x} {y} (constants)"
            );
        }
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn test_op_unary() {
        let globals = LVar::create_global_constants();

        for (op, x, want, epsilon) in [
            // not
            ("not", "0b00", (-1).into(), None),
            ("not", "0b01", (-2).into(), None),
            ("not", "0b10", (-3).into(), None),
            ("not", "0b11", (-4).into(), None),
            ("not", "-1", 0.into(), None),
            ("not", "-2", 1.into(), None),
            ("not", "-3", 2.into(), None),
            ("not", "-4", 3.into(), None),
            // abs
            ("abs", "0", 0.into(), None),
            ("abs", "-0", 0.into(), None),
            ("abs", "1", 1.into(), None),
            ("abs", "-1", 1.into(), None),
            ("abs", "1e308", 1e308.into(), None),
            ("abs", "1e309", 0.into(), None),
            ("abs", "-1e308", 1e308.into(), None),
            ("abs", "-1e309", 0.into(), None),
            // sign
            ("sign", "0", 0.into(), None),
            ("sign", "-0", 0.into(), None),
            ("sign", "1", 1.into(), None),
            ("sign", "-1", (-1).into(), None),
            ("sign", "1e308", 1.into(), None),
            ("sign", "1e309", 0.into(), None),
            ("sign", "-1e308", (-1).into(), None),
            ("sign", "-1e309", 0.into(), None),
            // log
            ("log", "-1", LValue::NULL, None),
            ("log", "0", LValue::NULL, None),
            ("log", "@e", 0.99999996963214.into(), None),
            ("log", "2", 0.6931471805599453.into(), None),
            // log10
            ("log10", "-1", LValue::NULL, None),
            ("log10", "0", LValue::NULL, None),
            ("log10", "100", 2.into(), None),
            ("log10", "101", 2.0043213737826426.into(), None),
            // floor
            ("floor", "0", 0.into(), None),
            ("floor", "1", 1.into(), None),
            ("floor", "1.5", 1.into(), None),
            ("floor", "-1", (-1).into(), None),
            ("floor", "-1.5", (-2).into(), None),
            // ceil
            ("ceil", "0", 0.into(), None),
            ("ceil", "1", 1.into(), None),
            ("ceil", "1.5", 2.into(), None),
            ("ceil", "-1", (-1).into(), None),
            ("ceil", "-1.5", (-1).into(), None),
            // round
            ("round", "0", 0.into(), None),
            ("round", "1", 1.into(), None),
            ("round", "1.1", 1.into(), None),
            ("round", "1.49", 1.into(), None),
            ("round", "1.5", 2.into(), None),
            ("round", "1.51", 2.into(), None),
            ("round", "1.9", 2.into(), None),
            ("round", "-1", (-1).into(), None),
            ("round", "-1.1", (-1).into(), None),
            ("round", "-1.49", (-1).into(), None),
            ("round", "-1.5", (-1).into(), None),
            ("round", "-1.51", (-2).into(), None),
            ("round", "-1.9", (-2).into(), None),
            // sqrt
            ("sqrt", "-1", LValue::NULL, None),
            ("sqrt", "-0.25", LValue::NULL, None),
            ("sqrt", "0", 0.into(), None),
            ("sqrt", "0.25", 0.5.into(), None),
            ("sqrt", "1", 1.into(), None),
            ("sqrt", "4", 2.into(), None),
            // sin
            ("sin", "-30", (-0.5).into(), Some(1e-15)),
            ("sin", "0", 0.into(), None),
            ("sin", "30", 0.5.into(), Some(1e-15)),
            ("sin", "45", 0.7071067811865476.into(), Some(1e-15)),
            ("sin", "60", 0.8660254037844386.into(), Some(1e-15)),
            ("sin", "90", 1.into(), Some(1e-15)),
            ("sin", "180", 0.into(), Some(1e-15)),
            // cos
            ("cos", "-30", 0.8660254037844387.into(), Some(1e-15)),
            ("cos", "0", 1.into(), None),
            ("cos", "30", 0.8660254037844387.into(), Some(1e-15)),
            ("cos", "45", 0.7071067811865476.into(), Some(1e-15)),
            ("cos", "60", 0.5.into(), Some(1e-15)),
            ("cos", "90", 6.123233995736766e-17.into(), Some(1e-15)),
            ("cos", "180", (-1).into(), Some(1e-15)),
            // tan
            ("tan", "0", 0.into(), None),
            ("tan", "45", 1.into(), Some(1e-15)),
            ("tan", "90", 16331239353195370i64.into(), Some(1e-15)),
            ("tan", "135", (-1).into(), Some(1e-15)),
            // asin
            ("asin", "-0.5", (-30).into(), Some(1e-13)),
            ("asin", "0", 0.into(), None),
            ("asin", "0.5", 30.into(), Some(1e-13)),
            // acos
            ("acos", "-0.5", 120.into(), Some(1e-13)),
            ("acos", "0", 90.into(), None),
            ("acos", "0.5", 60.into(), Some(1e-13)),
            // atan
            ("atan", "-0.5", (-26.56505117707799).into(), Some(1e-13)),
            ("atan", "0", 0.into(), None),
            ("atan", "0.5", 26.56505117707799.into(), Some(1e-13)),
        ] {
            let mut vm = single_processor_vm_with_globals(
                HYPER_PROCESSOR,
                &format!(
                    "
                    op {op} got {x}
                    stop
                    "
                ),
                &globals,
            );

            run(&mut vm, 1, true);

            let processor = take_processor(&mut vm, (0, 0));
            let got = &processor.state.variables[u16str!("got")];
            if let Some(epsilon) = epsilon {
                assert!(
                    (got.num() - want.num()).abs() <= epsilon,
                    "{op} {x} (got {got:?}, want {want:?} ± {epsilon:?})"
                );
            } else {
                assert_eq!(*got, want, "{op} {x}");
            }
        }
    }

    #[test]
    fn test_op_binary() {
        let globals = LVar::create_global_constants();

        for (op, x, y, want) in [
            // add
            ("add", "0", "0", 0.into()),
            ("add", "0", "1", 1.into()),
            ("add", "1.5", "0.25", 1.75.into()),
            ("add", "1.0", "-2.0", (-1).into()),
            // sub
            ("sub", "3", "1", 2.into()),
            ("sub", "3", "-1", 4.into()),
            // mul
            ("mul", "1", "0", 0.into()),
            ("mul", "1", "1", 1.into()),
            ("mul", "3", "-4.5", (-13.5).into()),
            // div
            ("div", "5", "2", 2.5.into()),
            ("div", "-5", "2", (-2.5).into()),
            ("div", "5", "-2", (-2.5).into()),
            ("div", "-5", "-2", 2.5.into()),
            ("div", "1", "0", LValue::NULL),
            ("div", "-1", "0", LValue::NULL),
            ("div", "0", "0", LValue::NULL),
            ("div", "0", "1", 0.into()),
            ("div", "0", "-1", 0.into()),
            // idiv
            ("idiv", "5", "2", 2.into()),
            ("idiv", "-5", "2", (-3).into()),
            ("idiv", "5", "-2", (-3).into()),
            ("idiv", "-5", "-2", 2.into()),
            ("idiv", "1", "0", LValue::NULL),
            ("idiv", "-1", "0", LValue::NULL),
            ("idiv", "0", "0", LValue::NULL),
            ("idiv", "0", "1", 0.into()),
            ("idiv", "0", "-1", 0.into()),
            // mod
            ("mod", "5", "2", 1.into()),
            ("mod", "-5", "2", (-1).into()),
            ("mod", "5", "-2", 1.into()),
            ("mod", "-5", "-2", (-1).into()),
            // emod
            ("emod", "5", "2", 1.into()),
            ("emod", "-5", "2", 1.into()),
            ("emod", "5", "-2", (-1).into()),
            ("emod", "-5", "-2", (-1).into()),
            // pow
            ("pow", "3", "2", 9.into()),
            ("pow", "9", "0.5", 3.into()),
            ("pow", "16", "-0.5", 0.25.into()),
            ("pow", "-3", "2", 9.into()),
            ("pow", "-9", "0.5", LValue::NULL),
            ("pow", "-16", "-0.5", LValue::NULL),
            // land
            ("land", "false", "false", 0.into()),
            ("land", "false", "true", 0.into()),
            ("land", "true", "false", 0.into()),
            ("land", "true", "true", 1.into()),
            ("land", "\"foo\"", "null", 0.into()),
            ("land", "\"foo\"", "\"bar\"", 1.into()),
            // shl
            ("shl", "2", "0", 2.into()),
            ("shl", "2", "1", 4.into()),
            ("shl", "2", "62", (-9223372036854775808i64).into()),
            ("shl", "2", "63", 0.into()),
            ("shl", "2", "64", 2.into()),
            ("shl", "2", "-1", 0.into()),
            ("shl", "2", "-60", 32.into()),
            ("shl", "-2", "0", (-2).into()),
            ("shl", "-2", "1", (-4).into()),
            ("shl", "-2", "62", (-9223372036854775808i64).into()),
            ("shl", "-2", "63", 0.into()),
            ("shl", "-2", "64", (-2).into()),
            ("shl", "-2", "-1", 0.into()),
            ("shl", "-2", "-60", (-32).into()),
            // shr
            ("shr", "2", "0", 2.into()),
            ("shr", "2", "1", 1.into()),
            ("shr", "2", "62", 0.into()),
            ("shr", "2", "63", 0.into()),
            ("shr", "2", "64", 2.into()),
            ("shr", "2", "-1", 0.into()),
            ("shr", "2", "-60", 0.into()),
            ("shr", "-2", "0", (-2).into()),
            ("shr", "-2", "1", (-1).into()),
            ("shr", "-2", "62", (-1).into()),
            ("shr", "-2", "63", (-1).into()),
            ("shr", "-2", "64", (-2).into()),
            ("shr", "-2", "-1", (-1).into()),
            ("shr", "-2", "-60", (-1).into()),
            // ushr
            ("ushr", "2", "0", 2.into()),
            ("ushr", "2", "1", 1.into()),
            ("ushr", "2", "62", 0.into()),
            ("ushr", "2", "63", 0.into()),
            ("ushr", "2", "64", 2.into()),
            ("ushr", "2", "-1", 0.into()),
            ("ushr", "2", "-60", 0.into()),
            ("ushr", "-2", "0", (-2).into()),
            ("ushr", "-2", "1", 9223372036854775807i64.into()),
            ("ushr", "-2", "62", 3.into()),
            ("ushr", "-2", "63", 1.into()),
            ("ushr", "-2", "64", (-2).into()),
            ("ushr", "-2", "-1", 1.into()),
            ("ushr", "-2", "-60", 1152921504606846976i64.into()),
            // or
            ("or", "0b10", "0b10", 0b10.into()),
            ("or", "0b10", "0b11", 0b11.into()),
            ("or", "0b11", "0b10", 0b11.into()),
            ("or", "0b11", "0b11", 0b11.into()),
            ("or", "-1", "0", (-1).into()),
            ("or", "-1", "1", (-1).into()),
            // and
            ("and", "0b10", "0b10", 0b10.into()),
            ("and", "0b10", "0b11", 0b10.into()),
            ("and", "0b11", "0b10", 0b10.into()),
            ("and", "0b11", "0b11", 0b11.into()),
            ("and", "-1", "0", 0.into()),
            ("and", "-1", "1", 1.into()),
            // xor
            ("xor", "0b10", "0b10", 0b00.into()),
            ("xor", "0b10", "0b11", 0b01.into()),
            ("xor", "0b11", "0b10", 0b01.into()),
            ("xor", "0b11", "0b11", 0b00.into()),
            ("xor", "-1", "0", (-1).into()),
            ("xor", "-1", "1", (-2).into()),
            // max
            ("max", "-1", "1", 1.into()),
            ("max", "1", "-1", 1.into()),
            ("max", "1", "2", 2.into()),
            ("max", "2", "1", 2.into()),
            // min
            ("min", "-1", "1", (-1).into()),
            ("min", "1", "-1", (-1).into()),
            ("min", "1", "2", 1.into()),
            ("min", "2", "1", 1.into()),
            // angle
            // mindustry apparently uses a different algorithm that gives 29.999948501586914 instead of 30
            // but this is probably close enough
            ("angle", "0.8660254038", "0.5", 30.into()),
            // angleDiff
            ("angleDiff", "10", "10", 0.into()),
            ("angleDiff", "10", "20", 10.into()),
            ("angleDiff", "10", "-10", 20.into()),
            ("angleDiff", "10", "350", 20.into()),
            // len
            ("len", "3", "4", 5.into()),
            ("len", "1", "1", 2f32.sqrt().into()),
            // noise
            ("noise", "0", "0", 0.into()),
            // i'm not porting mindustry's noise algorithm. this is not the value you would get ingame
            ("noise", "0", "1", (-0.7139277281035279).into()),
            ("noise", "1", "0", (-0.3646124062135936).into()),
            // logn
            ("logn", "-1", "2", LValue::NULL),
            ("logn", "0", "2", LValue::NULL),
            ("logn", "0b1000", "2", 3.into()),
            ("logn", "0b1010", "2", 3.3219280948873626.into()),
            ("logn", "-1", "10", LValue::NULL),
            ("logn", "0", "10", LValue::NULL),
            ("logn", "100", "10", 2.into()),
            ("logn", "101", "10", 2.0043213737826426.into()),
        ]
        .into_iter()
        .chain(
            CONDITION_TESTS
                .iter()
                .filter(|&(op, ..)| *op != "always")
                .map(|&(op, x, y, want)| (op, x, y, want.into())),
        ) {
            let mut vm = single_processor_vm_with_globals(
                HYPER_PROCESSOR,
                &format!(
                    "
                    op {op} got {x} {y}
                    stop
                    "
                ),
                &globals,
            );

            run(&mut vm, 1, true);

            let processor = take_processor(&mut vm, (0, 0));
            assert_eq!(
                processor.state.variables[u16str!("got")],
                want,
                "{op} {x} {y}"
            );
        }
    }

    #[test]
    pub fn test_lookup() {
        let mut vm = single_processor_vm(
            HYPER_PROCESSOR,
            "
            set blocks @blockCount
            set items @itemCount
            set liquids @liquidCount
            set units @unitCount

            lookup block block1 -1
            lookup block block2 0
            lookup block block3 259
            lookup block block4 260

            lookup item item1 -1
            lookup item item2 0
            lookup item item3 19
            lookup item item4 20

            lookup liquid liquid1 -1
            lookup liquid liquid2 0
            lookup liquid liquid3 10
            lookup liquid liquid4 11

            lookup unit unit1 -1
            lookup unit unit2 0
            lookup unit unit3 55
            lookup unit unit4 56

            lookup team team1 -1
            lookup team team2 0
            lookup team team3 255
            lookup team team4 256

            stop
            ",
        );

        run(&mut vm, 1, true);

        let variables = take_processor(&mut vm, (0, 0)).state.variables;

        assert_eq!(variables[u16str!("blocks")], LValue::from(260.));
        assert_eq!(variables[u16str!("items")], LValue::from(20.));
        assert_eq!(variables[u16str!("liquids")], LValue::from(11.));
        assert_eq!(variables[u16str!("units")], LValue::from(56.));

        // blocks

        assert_eq!(variables[u16str!("block1")], LValue::NULL);
        let block2 = &variables[u16str!("block2")];
        assert!(
            matches!(block2.obj(), Some(LObject::Content(Content::Block(b))) if b.name.as_str() == "graphite-press"),
            "{block2:?}"
        );
        let block3 = &variables[u16str!("block3")];
        assert!(
            matches!(block3.obj(), Some(LObject::Content(Content::Block(b))) if b.name.as_str() == "tile-logic-display"),
            "{block3:?}"
        );
        assert_eq!(variables[u16str!("block4")], LValue::NULL);

        // items

        assert_eq!(variables[u16str!("item1")], LValue::NULL);
        let item2 = &variables[u16str!("item2")];
        assert!(
            matches!(item2.obj(), Some(LObject::Content(Content::Item(b))) if b.name.as_str() == "copper"),
            "{item2:?}"
        );
        let item3 = &variables[u16str!("item3")];
        assert!(
            matches!(item3.obj(), Some(LObject::Content(Content::Item(b))) if b.name.as_str() == "carbide"),
            "{item3:?}"
        );
        assert_eq!(variables[u16str!("item4")], LValue::NULL);

        // liquids

        assert_eq!(variables[u16str!("liquid1")], LValue::NULL);
        let liquid2 = &variables[u16str!("liquid2")];
        assert!(
            matches!(liquid2.obj(), Some(LObject::Content(Content::Liquid(b))) if b.name.as_str() == "water"),
            "{liquid2:?}"
        );
        let liquid3 = &variables[u16str!("liquid3")];
        assert!(
            matches!(liquid3.obj(), Some(LObject::Content(Content::Liquid(b))) if b.name.as_str() == "arkycite"),
            "{liquid3:?}"
        );
        assert_eq!(variables[u16str!("liquid4")], LValue::NULL);

        // units

        assert_eq!(variables[u16str!("unit1")], LValue::NULL);
        let unit2 = &variables[u16str!("unit2")];
        assert!(
            matches!(unit2.obj(), Some(LObject::Content(Content::Unit(b))) if b.name.as_str() == "dagger"),
            "{unit2:?}"
        );
        let unit3 = &variables[u16str!("unit3")];
        assert!(
            matches!(unit3.obj(), Some(LObject::Content(Content::Unit(b))) if b.name.as_str() == "emanate"),
            "{unit3:?}"
        );
        assert_eq!(variables[u16str!("unit4")], LValue::NULL);

        // teams

        assert_eq!(variables[u16str!("team1")], LValue::NULL);
        assert_eq!(variables[u16str!("team2")], LValue::from(Team::DERELICT));
        assert_eq!(variables[u16str!("team3")], LValue::from(Team(255)));
        assert_eq!(variables[u16str!("team4")], LValue::NULL);
    }

    #[test]
    fn test_draw() {
        let tests = [
            ("draw clear 10 20 30", DrawCommand::Clear {
                r: 10,
                g: 20,
                b: 30,
            }),
            ("draw color 10 20 30 40", DrawCommand::Color {
                r: 10,
                g: 20,
                b: 30,
                a: 40,
            }),
            ("draw col %[black]", DrawCommand::Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }),
            ("draw col %[royal]", DrawCommand::Color {
                r: 0x41,
                g: 0x69,
                b: 0xe1,
                a: 0xff,
            }),
            ("draw stroke 5", DrawCommand::Stroke { width: 5 }),
            ("draw line 10 20 30 40", DrawCommand::Line {
                x1: 10,
                y1: 20,
                x2: 30,
                y2: 40,
            }),
            ("draw rect 10 20 30 40", DrawCommand::Rect {
                x: 10,
                y: 20,
                width: 30,
                height: 40,
                fill: true,
            }),
            ("draw lineRect 10 20 30 40", DrawCommand::Rect {
                x: 10,
                y: 20,
                width: 30,
                height: 40,
                fill: false,
            }),
            ("draw poly 10 20 30 40 359", DrawCommand::Poly {
                x: 10,
                y: 20,
                sides: 30,
                radius: 40,
                rotation: 359,
                fill: true,
            }),
            ("draw linePoly 10 20 30 40 359", DrawCommand::Poly {
                x: 10,
                y: 20,
                sides: 30,
                radius: 40,
                rotation: 359,
                fill: false,
            }),
            ("draw triangle 10 20 30 40 50 60", DrawCommand::Triangle {
                x1: 10,
                y1: 20,
                x2: 30,
                y2: 40,
                x3: 50,
                y3: 60,
            }),
            ("draw image 10 20 @copper 30 359", DrawCommand::Image {
                x: 10,
                y: 20,
                image: Some(Content::Item(content::items::FROM_NAME["copper"])),
                size: 30,
                rotation: 359,
            }),
            ("draw print 10 20 @bottomLeft", DrawCommand::Print {
                x: 10,
                y: 20,
                alignment: TextAlignment::BOTTOM_LEFT,
                text: u16str!("a\nb").into(),
            }),
            ("draw translate 10 20", DrawCommand::Translate {
                x: 10,
                y: 20,
            }),
            ("draw scale 10 20", DrawCommand::Scale {
                x: 10 * 20,
                y: 20 * 20,
            }),
            ("draw rotate 359", DrawCommand::Rotate { degrees: 359 }),
            ("draw reset", DrawCommand::Reset),
        ];

        let mut vm = single_processor_vm(
            HYPER_PROCESSOR,
            &format!(
                r#"
                print "a\nb"

                {}

                wait 0.5

                draw triangle var1 var2 var3 var4 var5 var6

                drawflush display1
                stop
                "#,
                tests.iter().map(|v| v.0).join("\n"),
            ),
        );

        vm.do_tick(Duration::ZERO);

        with_processor(&mut vm, (0, 0), |processor| {
            assert_eq!(processor.state.drawbuffer_len, tests.len() + 1);
            assert_eq!(
                processor.state.drawbuffer,
                tests.into_iter().map(|v| v.1).collect_vec()
            );
            assert_eq!(processor.state.printbuffer, U16String::new());
        });

        vm.do_tick(Duration::from_secs(1));

        let processor = take_processor(&mut vm, (0, 0));
        assert_eq!(processor.state.drawbuffer_len, 0);
        assert_eq!(processor.state.drawbuffer, vec![]);
        for name in [
            u16str!("var1"),
            u16str!("var2"),
            u16str!("var3"),
            u16str!("var4"),
            u16str!("var5"),
            u16str!("var6"),
        ] {
            assert!(
                processor.state.variables.contains_key(name),
                "variable not found: {}",
                name.display()
            );
        }
    }

    #[test]
    fn test_time() {
        let mut vm = single_processor_vm(
            MICRO_PROCESSOR,
            "
            set tick1 @tick
            set time1 @time
            set second1 @second
            set minute1 @minute

            set tick2 @tick
            set time2 @time
            set second2 @second
            set minute2 @minute

            set tick3 @tick
            set time3 @time
            set second3 @second
            set minute3 @minute

            set tick4 @tick
            set time4 @time
            set second4 @second
            set minute4 @minute
            ",
        );

        let mut time = Duration::ZERO;

        vm.do_tick(time);
        vm.do_tick(time);

        time += Duration::from_secs(1);
        vm.do_tick(time);
        vm.do_tick(time);

        time += Duration::from_millis(1);
        vm.do_tick(time);
        vm.do_tick(time);

        time += Duration::from_secs(60);
        vm.do_tick(time);
        vm.do_tick(time);

        let processor = take_processor(&mut vm, (0, 0));
        assert_variables_epsilon(&processor, 1e-8, map_iter! {
            u16str!("tick1"): 0.,
            u16str!("time1"): 0.,
            u16str!("second1"): 0.,
            u16str!("minute1"): 0.,

            u16str!("tick2"): 1. * 60.,
            u16str!("time2"): 1000.,
            u16str!("second2"): 1.,
            u16str!("minute2"): 1. / 60.,

            u16str!("tick3"): 1.001 * 60.,
            u16str!("time3"): 1001.,
            u16str!("second3"): 1.001,
            u16str!("minute3"): 1.001 / 60.,

            u16str!("tick4"): 61.001 * 60.,
            u16str!("time4"): 61001.,
            u16str!("second4"): 61.001,
            u16str!("minute4"): 1.001 / 60. + 1.,
        });
    }
}
