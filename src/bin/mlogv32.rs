#![allow(dead_code)]

use std::io::{Cursor, Read, Write};
use std::time::Duration;
use std::{error::Error, fmt::Display, fs::File, path::PathBuf, time::Instant};

use binrw::{BinRead, BinWrite};
use clap::Parser;
use indicatif::ProgressIterator;
use itertools::Itertools;
use mindustry_rs::logic::vm::{
    Building, LogicVM, MEMORY_BANK, MEMORY_CELL, MESSAGE, MICRO_PROCESSOR, Processor, SWITCH,
    WORLD_PROCESSOR, decode_utf16,
};
use mindustry_rs::types::{Object, ProcessorConfig};
use mindustry_rs::{
    logic::vm::{BuildingData, LogicVMBuilder},
    types::{Point2, Schematic},
};
use prompted::input;
use serde::Deserialize;

#[derive(Parser)]
#[command(version)]
struct Cli {
    /// The mlogv32 schematic to run
    schematic: PathBuf,

    /// Binary file to flash to the processor's ROM before running
    #[arg(long)]
    bin: Option<PathBuf>,

    /// Enable single-stepping mode when the processor starts
    #[arg(long)]
    step: bool,
}

#[derive(Deserialize)]
struct Metadata {
    uarts: Vec<MetaPoint2>,

    registers: MetaPoint2,
    csrs: MetaPoint2,
    config: MetaPoint2,
    uart_fifo_capacity: usize,

    error_output: MetaPoint2,
    power_switch: MetaPoint2,
    pause_switch: MetaPoint2,
    single_step_switch: MetaPoint2,

    cpu: MetaPoint2,
    cpu_width: usize,
    cpu_height: usize,

    memory: MetaPoint2,
    memory_width: usize,
    memory_height: usize,

    rom_processors: usize,
    ram_processors: usize,
    icache_processors: usize,
}

#[derive(Deserialize, Clone, Copy)]
struct MetaPoint2(i32, i32);

impl MetaPoint2 {
    fn x(&self) -> i32 {
        self.0
    }
    fn y(&self) -> i32 {
        self.1
    }
}

impl Display for MetaPoint2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {})", self.0, self.1)
    }
}

impl From<MetaPoint2> for Point2 {
    fn from(MetaPoint2(x, y): MetaPoint2) -> Self {
        Self { x, y }
    }
}

fn get_building<'a>(vm: &'a LogicVM, position: MetaPoint2, name: &str) -> &'a Building {
    let Some(building) = vm.building(position.into()) else {
        panic!("{name} not found at {position}");
    };
    assert_eq!(building.block.name, name);
    building
}

fn print_var(processor: &Processor, name: &str) {
    match processor.variable(name) {
        Some(value) => println!("{name} = {value:?}"),
        None => println!("{name} = <undefined>"),
    }
}

// tx/rx are from our perspective, not the processor's
const UART_TX_READ: usize = 254;
const UART_TX_WRITE: usize = 255;
const UART_RX_START: usize = 256;
const UART_RX_READ: usize = 510;
const UART_RX_WRITE: usize = 511;

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    println!("Loading schematic...");

    let mut schematic = {
        let mut file = File::open(cli.schematic)?;
        Schematic::read(&mut file)?
    };

    println!("Loading metadata...");

    let meta: Metadata = serde_json::from_str(&schematic.tags["mlogv32_metadata"])?;

    if let Some(bin_path) = cli.bin {
        println!("Flashing ROM...");

        let mut data = Vec::new();
        {
            let mut file = File::open(bin_path)?;
            file.read_to_end(&mut data)?;
        }
        while data.len() % 4 != 0 {
            data.push(0);
        }

        schematic
            .tiles_mut()
            .sort_by_key(|t| (t.position.y, t.position.x));

        let mut i = 0;
        let mut code = String::new();
        let bar_count = ((data.len() as f64) / 16384.) as u64;

        for (chunk, tile) in data
            .into_iter()
            .chunks(16384)
            .into_iter()
            .zip(schematic.tiles_mut().iter_mut().filter(|t| {
                t.block == MICRO_PROCESSOR
                    && (t.position.x as i32) >= meta.memory.x()
                    && (t.position.x as i32) < meta.memory.x() + (meta.memory_width as i32)
                    && (t.position.y as i32) >= meta.memory.y()
                    && (t.position.y as i32) < meta.memory.y() + (meta.memory_height as i32)
            }))
            .progress_count(bar_count)
        {
            i += 1;
            if i > meta.rom_processors {
                panic!("ROM overflowed!");
            }

            code.clear();
            code.push_str("set v \"");
            for b in chunk {
                code.push(char::from_u32((b as u32) + 174).unwrap());
            }
            code.push_str("\"; stop");

            let mut cur = Cursor::new(Vec::new());
            ProcessorConfig::from_code(&code).write(&mut cur)?;
            tile.config = Object::ByteArray {
                values: cur.into_inner(),
            };
        }
    }

    println!("Loading VM...");

    let mut builder = LogicVMBuilder::new();
    for tile in schematic.tiles().iter().progress() {
        builder.add_schematic_tile(tile)?;
    }
    let vm = builder.build()?;

    let uart_fifo_modulo = meta.uart_fifo_capacity + 1;

    let controller = get_building(&vm, meta.cpu, WORLD_PROCESSOR);
    let registers = get_building(&vm, meta.registers, MEMORY_CELL);
    let uart0 = get_building(&vm, meta.uarts[0], MEMORY_BANK);
    let error_output = get_building(&vm, meta.error_output, MESSAGE);
    let power_switch = get_building(&vm, meta.power_switch, SWITCH);
    let pause_switch = get_building(&vm, meta.pause_switch, SWITCH);
    let single_step_switch = get_building(&vm, meta.single_step_switch, SWITCH);

    println!("Initializing processors...");

    for _ in 0..500 {
        vm.do_tick(Duration::from_secs_f64(1. / 60.));
    }

    println!("Starting.\n--------");

    if let BuildingData::Switch(enabled) = &mut *power_switch.data.borrow_mut() {
        *enabled = true;
    }
    if let BuildingData::Switch(enabled) = &mut *single_step_switch.data.borrow_mut() {
        *enabled = cli.step;
    }

    let mut ticks = 0;
    let mut uart_print = String::new();
    let mut now = Instant::now() - Duration::from_secs_f64(1. / 60.);
    let start = Instant::now();

    loop {
        vm.do_tick(now.elapsed());
        ticks += 1;
        now = Instant::now();

        if let BuildingData::Switch(paused) = &mut *pause_switch.data.borrow_mut()
            && *paused
            && let BuildingData::Switch(single_step) = &mut *single_step_switch.data.borrow_mut()
            && let BuildingData::Processor(ctrl) = &*controller.data.borrow()
            && let BuildingData::Memory(mem) = &*registers.data.borrow()
        {
            println!("\ntime: {:.3?}", vm.time());
            println!("pc: {:#010x}", ctrl.variable("pc").unwrap().num() as u32);
            for i in 0..16 {
                println!(
                    "x{i:<2} = {:#010x}  x{:<2} = {:#010x}",
                    mem[i] as u32,
                    i + 16,
                    mem[i + 16] as u32
                );
            }

            loop {
                let cmd = input!("> ");
                let cmd = cmd.split(' ').collect_vec();
                match cmd[0] {
                    "s" | "step" => {
                        *single_step = true;
                        break;
                    }
                    "c" | "continue" => {
                        *single_step = false;
                        break;
                    }
                    "p" | "print" | "v" | "var" if cmd.len() >= 2 => print_var(ctrl, cmd[1]),
                    "i" | "inspect" if cmd.len() >= 3 => {
                        let Ok(x) = cmd[1].parse() else {
                            println!("Invalid x.");
                            continue;
                        };
                        let Ok(y) = cmd[2].parse() else {
                            println!("Invalid y.");
                            continue;
                        };
                        match vm.building((x, y).into()) {
                            Some(b) => match b.data.try_borrow() {
                                Ok(data) => match &*data {
                                    BuildingData::Processor(p) => match cmd.get(3) {
                                        Some(&"*") => {
                                            for name in p.variables.keys() {
                                                print_var(p, name);
                                            }
                                        }
                                        Some(addr) => print_var(p, addr),
                                        None => println!("{}", b.block.name),
                                    },
                                    BuildingData::Memory(mem) => {
                                        let i =
                                            cmd.get(3).copied().unwrap_or("").parse().unwrap_or(0);
                                        println!("{}[{i}] = {:?}", b.block.name, mem.get(i));
                                    }
                                    BuildingData::Message(msg) => {
                                        println!("{} = {}", b.block.name, decode_utf16(msg))
                                    }
                                    BuildingData::Switch(on) => {
                                        println!("{} = {on}", b.block.name)
                                    }
                                    _ => println!("{}", b.block.name),
                                },
                                Err(_) => println!("Already borrowed."),
                            },
                            None => println!("No building found at ({x}, {y})."),
                        }
                    }
                    _ => {}
                }
            }

            *paused = false;
        }

        if vm.running_processors() == 0 {
            println!("\n--------\nAll processors halted, exiting.");
            break;
        }

        if let BuildingData::Switch(enabled) = &*power_switch.data.borrow()
            && !*enabled
        {
            println!("\n--------\nPower switch disabled, exiting.");
            break;
        }

        if let BuildingData::Memory(memory) = &mut *uart0.data.borrow_mut() {
            let mut rx_read = (memory[UART_RX_READ] as usize) % uart_fifo_modulo;
            let rx_write = (memory[UART_RX_WRITE] as usize) % uart_fifo_modulo;
            if rx_read != rx_write {
                uart_print.clear();
                while rx_read != rx_write {
                    let c = memory[UART_RX_START + rx_read] as u8;
                    uart_print.push(c as char);
                    rx_read = (rx_read + 1) % uart_fifo_modulo;
                }
                memory[UART_RX_READ] = rx_write as f64;
                print!("{uart_print}");
                std::io::stdout().flush()?;
            }
        }
    }

    let elapsed = start.elapsed();

    if let BuildingData::Message(buf) = &*error_output.data.borrow()
        && !buf.is_empty()
    {
        println!("Error output: {}", decode_utf16(buf));
    }

    println!("Runtime: {elapsed:?}");
    println!("Ticks completed: {ticks}");
    println!("Average time per tick: {:?}", elapsed / ticks);
    println!(
        "Average ticks per second: {:.1}",
        (ticks as f64) / elapsed.as_secs_f64()
    );

    Ok(())
}
