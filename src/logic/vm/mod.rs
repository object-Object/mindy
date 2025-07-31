#![allow(unused)]

mod blocks;
mod instructions;
mod processor;
mod variables;

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};

use thiserror::Error;

use self::blocks::{Block, BlockType};
use crate::types::{Point2, Schematic, SchematicTile};

const MILLIS_PER_SEC: u64 = 1_000;
const NANOS_PER_MILLI: u32 = 1_000_000;

type BlockPtr = Rc<RefCell<Block>>;

pub struct LogicVM {
    blocks: HashMap<Point2, BlockPtr>,
    processors: Vec<BlockPtr>,
    running_processors: Rc<Cell<usize>>,
    time: Rc<Cell<f64>>,
}

impl LogicVM {
    pub fn new() -> Self {
        Self {
            blocks: HashMap::new(),
            processors: Vec::new(),
            running_processors: Rc::new(Cell::new(0)),
            time: Rc::new(Cell::new(0.)),
        }
    }

    pub fn from_schematic(schematic: &Schematic) -> VMLoadResult<Self> {
        Self::from_schematic_tiles(schematic.tiles())
    }

    pub fn from_schematic_tiles(tiles: &[SchematicTile]) -> VMLoadResult<Self> {
        let mut vm = Self::new();
        vm.add_schematic_tiles(tiles)?;
        Ok(vm)
    }

    pub fn add_block(&mut self, block: Block, size: i32, position: Point2) -> VMLoadResult<()> {
        let block = Rc::new(RefCell::new(block));

        for x in position.x..position.x + size {
            for y in position.y..position.y + size {
                self.blocks.insert(Point2 { x, y }, block.clone());
            }
        }

        if let Block::Processor(processor) = &*block.borrow() {
            self.processors.push(block.clone());
            if processor.state.enabled() {
                self.running_processors.update(|n| n + 1);
            }
        }

        Ok(())
    }

    pub fn add_blocks<T>(&mut self, blocks: T) -> VMLoadResult<()>
    where
        T: IntoIterator<Item = ((Block, i32), Point2)>,
    {
        for ((block, size), position) in blocks.into_iter() {
            self.add_block(block, size, position)?;
        }
        Ok(())
    }

    pub fn add_schematic_tile(&mut self, tile: &SchematicTile) -> VMLoadResult<()> {
        let (block, size) = Block::from_schematic_tile(tile, self)?;
        let position = Point2::from(tile.position);
        self.add_block(block, size, position)
    }

    pub fn add_schematic_tiles(&mut self, tiles: &[SchematicTile]) -> VMLoadResult<()> {
        for tile in tiles {
            self.add_schematic_tile(tile)?;
        }
        Ok(())
    }

    /// Run the simulation until all processors halt, or until a number of ticks are finished.
    /// Returns true if all processors halted, or false if the tick limit was reached.
    pub fn run(&mut self, max_ticks: Option<usize>) -> bool {
        let mut now = Instant::now();
        let mut tick = 0;

        loop {
            self.do_tick(now.elapsed());
            now = Instant::now();

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

    /// Execute one tick of the simulation.
    pub fn do_tick(&mut self, delta: Duration) {
        self.time.update(|t| t + duration_millis_f64(delta));

        for processor in &self.processors {
            processor.borrow_mut().unwrap_processor_mut().do_tick(self);
        }
    }
}

impl Default for LogicVM {
    fn default() -> Self {
        Self::new()
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
    #[error("expected {want} block type but got {got}")]
    BadBlockType { want: String, got: BlockType },

    #[error("failed to decode processor config")]
    BadProcessorConfig(#[from] binrw::Error),

    #[error("failed to parse processor code: {0}")]
    BadProcessorCode(String),

    #[error("tried to place multiple blocks at {0}")]
    Overlap(Point2),
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use binrw::BinWrite;

    use crate::types::{Object, PackedPoint2, ProcessorConfig};

    use super::{processor::Processor, *};

    fn single_processor_vm(block: BlockType, code: &str) -> LogicVM {
        let mut vm = LogicVM::new();
        vm.add_blocks([(
            Block::new_processor(block, &ProcessorConfig::from_code(code), &vm).unwrap(),
            Point2 { x: 0, y: 0 },
        )])
        .unwrap();
        vm
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

    fn take_processor(vm: &mut LogicVM, idx: usize) -> Processor {
        vm.processors[0]
            .replace(Block::Unknown {
                block: "".into(),
                config: Object::Null,
            })
            .into_processor()
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
                ProcessorConfig::from_code("stop").write(&mut cur);
                cur.into_inner().into()
            },
            rotation: 0,
        }])
        .unwrap();

        run(&mut vm, 1, true);

        let processor = take_processor(&mut vm, 0);
        assert_eq!(processor.state.counter, 0);
        assert!(processor.state.stopped());
    }

    #[test]
    fn test_max_ticks() {
        let mut vm = single_processor_vm(
            BlockType::MicroProcessor,
            "
            noop
            noop
            stop
            ",
        );

        run(&mut vm, 1, false);

        let processor = take_processor(&mut vm, 0);
        assert_eq!(processor.state.counter, 2);
        assert!(!processor.state.stopped());
    }

    #[test]
    fn test_print() {
        let mut vm = single_processor_vm(
            BlockType::HyperProcessor,
            r#"
            print "foo"
            print "bar\n"
            print 10
            print "\n"
            print 1.5
            stop
            "#,
        );

        run(&mut vm, 1, true);

        let processor = take_processor(&mut vm, 0);
        assert_eq!(processor.state.printbuffer, "foobar\n10\n1.5")
    }
}
