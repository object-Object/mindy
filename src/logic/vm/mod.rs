#![allow(dead_code)]

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

    use crate::{
        logic::vm::variables::{LValue, LVar},
        types::{Object, PackedPoint2, ProcessorConfig},
    };

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

    fn with_processor(vm: &mut LogicVM, idx: usize, f: impl FnOnce(&mut Processor)) {
        f(vm.processors[idx].borrow_mut().unwrap_processor_mut())
    }

    fn take_processor(vm: &mut LogicVM, idx: usize) -> Processor {
        vm.processors[idx]
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
                ProcessorConfig::from_code("stop").write(&mut cur).unwrap();
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
            print null
            print foo
            stop
            "#,
        );

        run(&mut vm, 1, true);

        let processor = take_processor(&mut vm, 0);
        assert_eq!(processor.state.printbuffer, "foobar\n10\n1.5nullnull");
    }

    #[test]
    fn test_end() {
        let mut vm = single_processor_vm(
            BlockType::MicroProcessor,
            "
            print 1
            end
            print 2
            ",
        );

        run(&mut vm, 2, false);

        let processor = take_processor(&mut vm, 0);
        assert_eq!(processor.state.counter, 3);
        assert!(!processor.state.stopped());
        assert_eq!(processor.state.printbuffer, "11");
    }

    #[test]
    fn test_set() {
        let mut vm = single_processor_vm(
            BlockType::MicroProcessor,
            "
            set foo 1
            noop
            set foo 2
            set @counter 6
            stop
            stop
            set @ipt 10
            set true 0
            ",
        );

        with_processor(&mut vm, 0, |p| {
            assert_eq!(p.state.variables["foo"].get(&p.state), LValue::Null);
        });

        vm.do_tick(Duration::ZERO);

        with_processor(&mut vm, 0, |p| {
            assert_eq!(p.state.variables["foo"].get(&p.state), LValue::Number(1.));
        });

        vm.do_tick(Duration::ZERO);

        with_processor(&mut vm, 0, |p| {
            assert_eq!(p.state.variables["foo"].get(&p.state), LValue::Number(2.));
            assert_eq!(p.state.counter, 6);
        });

        vm.do_tick(Duration::ZERO);

        with_processor(&mut vm, 0, |p| {
            assert_eq!(p.state.variables["@ipt"], LVar::Ipt);
            assert_eq!(p.state.variables["true"].get(&p.state), LValue::Number(1.));
        });
    }

    #[test]
    fn test_setrate() {
        let mut vm = single_processor_vm(
            BlockType::WorldProcessor,
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
            with_processor(&mut vm, 0, |p| {
                assert_eq!(
                    p.state.ipt, ipt,
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
        let mut vm = single_processor_vm(BlockType::MicroProcessor, "setrate 10; stop");
        run(&mut vm, 1, true);
        let processor = take_processor(&mut vm, 0);
        assert_eq!(processor.state.ipt, 2);
    }

    #[test]
    fn test_jump() {
        let mut code = r#"
        setrate 1000

        # detect reentry
        set source "reentered init"
        jump oops notEqual canary null
        set source null

        set canary 0xdeadbeef
        "#
        .to_string();

        for (i, (x, y, cond, want_jump)) in [
            // equal
            ("0", "0", "equal", true),
            ("0", "null", "equal", true),
            ("1", r#""""#, "equal", true),
            ("1", r#""foo""#, "equal", true),
            (r#""""#, r#""""#, "equal", true),
            (r#""abc""#, r#""abc""#, "equal", true),
            ("null", "null", "equal", true),
            ("0", "0.0000009", "equal", true),
            ("0", "0.000001", "equal", false),
            ("0", "1", "equal", false),
            ("1", "null", "equal", false),
            (r#""abc""#, r#""def""#, "equal", false),
            // notEqual
            ("0", "0", "notEqual", false),
            ("0", "null", "notEqual", false),
            ("null", "null", "notEqual", false),
            ("0", "0.0000009", "notEqual", false),
            ("0", "0.000001", "notEqual", true),
            ("0", "1", "notEqual", true),
            ("1", "null", "notEqual", true),
            // lessThan
            ("0", "1", "lessThan", true),
            ("0", "0", "lessThan", false),
            ("1", "0", "lessThan", false),
            // lessThanEq
            ("0", "1", "lessThanEq", true),
            ("0", "0", "lessThanEq", true),
            ("1", "0", "lessThanEq", false),
            // greaterThan
            ("0", "1", "greaterThan", false),
            ("0", "0", "greaterThan", false),
            ("1", "0", "greaterThan", true),
            // greaterThanEq
            ("0", "1", "greaterThanEq", false),
            ("0", "0", "greaterThanEq", true),
            ("1", "0", "greaterThanEq", true),
            // strictEqual
            ("0", "0", "strictEqual", true),
            ("null", "null", "strictEqual", true),
            (r#""""#, r#""""#, "strictEqual", true),
            (r#""abc""#, r#""abc""#, "strictEqual", true),
            ("0", "null", "strictEqual", false),
            ("1", r#""""#, "strictEqual", false),
            ("1", r#""foo""#, "strictEqual", false),
            (r#""abc""#, r#""def""#, "strictEqual", false),
            ("0", "0.0000009", "strictEqual", false),
            ("0", "0.000001", "strictEqual", false),
            ("0", "1", "strictEqual", false),
            ("1", "null", "strictEqual", false),
        ]
        .into_iter()
        .enumerate()
        {
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

        let mut vm = single_processor_vm(BlockType::WorldProcessor, &code);

        run(&mut vm, 2, true);

        let state = take_processor(&mut vm, 0).state;
        assert_eq!(
            state.variables["canary"].get(&state),
            LValue::Number(0xdeadbeefu64 as f64)
        );
    }
}
