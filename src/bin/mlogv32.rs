#![allow(dead_code)]

use std::borrow::Cow;
use std::io::{Cursor, Read};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;
use std::{error::Error, fmt::Display, fs::File, path::PathBuf, time::Instant};

use binrw::{BinRead, BinWrite};
use clap::Parser;
use cursive::theme::Theme;
use cursive::view::{Nameable, Resizable, ScrollStrategy};
use cursive::views::{
    Checkbox, EditView, LinearLayout, ListView, Panel, ScrollView, TextContent, TextView,
};
use indicatif::ProgressIterator;
use itertools::Itertools;
use mindustry_rs::logic::vm::LVar;
use mindustry_rs::{
    logic::vm::{
        Building, BuildingData, LValue, LogicVM, LogicVMBuilder, MEMORY_BANK, MESSAGE,
        MICRO_PROCESSOR, Processor, SWITCH, WORLD_PROCESSOR, decode_utf16,
    },
    types::{Object, Point2, ProcessorConfig, Schematic},
};
use serde::Deserialize;

// tx/rx are from our perspective, not the processor's
const UART_TX_READ: usize = 254;
const UART_TX_WRITE: usize = 255;
const UART_RX_START: usize = 256;
const UART_RX_READ: usize = 510;
const UART_RX_WRITE: usize = 511;

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

    #[arg(long, short)]
    verbose: bool,
}

#[derive(Debug, Deserialize)]
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

    mtime_frequency: usize,
}

#[derive(Debug, Clone, Copy, Deserialize)]
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

macro_rules! tui_println {
    ($text:ident) => {
        $text.append("\n")
    };
    ($text:ident, $($arg:tt)*) => {
        {
            $text.append(format!($($arg)*));
            $text.append("\n");
        }
    };
}

enum VMCommand {
    Exit,
    Freeze,
    Pause,
    Step,
    Continue,
    Restart,
    SetBreakpoint(Option<u32>),
    PrintVar(String, Option<String>),
}

struct VMState {
    power: bool,
    pause: bool,
    single_step: bool,
    state: Option<String>,
    pc: u32,
    mtime: u32,
    mcycle: u32,
    minstret: u32,
}

fn tui(stdout: TextContent, debug: TextContent, tx: Sender<VMCommand>, rx: Receiver<VMState>) {
    let mut siv = cursive::crossterm().into_runner();

    siv.set_fps(20);
    siv.set_theme(Theme::terminal_default());

    siv.add_fullscreen_layer(
        LinearLayout::vertical()
            .child(
                Panel::new(
                    ScrollView::new(TextView::new_with_content(stdout))
                        .scroll_strategy(ScrollStrategy::StickToBottom),
                )
                .title("UART0")
                .full_height(),
            )
            .child(
                LinearLayout::horizontal()
                    .child(
                        Panel::new(
                            LinearLayout::vertical()
                                .child(
                                    ScrollView::new(TextView::new_with_content(debug.clone()))
                                        .scroll_strategy(ScrollStrategy::StickToBottom)
                                        .min_height(16),
                                )
                                .child(
                                    EditView::new()
                                        .on_submit({
                                            let tx = tx.clone();
                                            move |siv, cmd| {
                                                if let Some(msg) = process_cmd(&debug, cmd) {
                                                    tui_println!(debug, "> {cmd}");
                                                    tx.send(msg).unwrap();
                                                }
                                                siv.call_on_name("debug", |view: &mut EditView| {
                                                    view.set_content("");
                                                });
                                            }
                                        })
                                        .with_name("debug"),
                                )
                                .full_width(),
                        )
                        .title("Debug"),
                    )
                    .child(Panel::new(
                        ListView::new()
                            .child("Power", Checkbox::new().disabled().with_name("power"))
                            .child("Pause", Checkbox::new().disabled().with_name("pause"))
                            .child("Step", Checkbox::new().disabled().with_name("single_step"))
                            .child("State", TextView::new("???").with_name("state"))
                            .child("PC", TextView::new("0x00000000").with_name("pc"))
                            .child("time", TextView::new("0").with_name("mtime"))
                            .child("cycle", TextView::new("0").with_name("mcycle"))
                            .child("instret", TextView::new("0").with_name("minstret"))
                            .min_width("State trap_breakpoint".len()),
                    )),
            )
            .full_screen(),
    );

    siv.refresh();
    while siv.is_running() {
        siv.step();
        for VMState {
            power,
            pause,
            single_step,
            state,
            pc,
            mtime,
            mcycle,
            minstret,
        } in rx.try_iter()
        {
            siv.call_on_name("power", |v: &mut Checkbox| v.set_checked(power));
            siv.call_on_name("pause", |v: &mut Checkbox| v.set_checked(pause));
            siv.call_on_name("single_step", |v: &mut Checkbox| v.set_checked(single_step));
            siv.call_on_name("state", |v: &mut TextView| match state {
                Some(state) => v.set_content(state),
                None => v.set_content("???"),
            });
            siv.call_on_name("pc", |v: &mut TextView| {
                v.set_content(format!("{pc:#010x}"))
            });
            siv.call_on_name("mtime", |v: &mut TextView| v.set_content(mtime.to_string()));
            siv.call_on_name("mcycle", |v: &mut TextView| {
                v.set_content(mcycle.to_string())
            });
            siv.call_on_name("minstret", |v: &mut TextView| {
                v.set_content(minstret.to_string())
            });
        }
    }
    tx.send(VMCommand::Exit).unwrap();
}

fn process_cmd(out: &TextContent, cmd: &str) -> Option<VMCommand> {
    let cmd = cmd.split(' ').collect_vec();
    Some(match cmd[0] {
        "freeze" => VMCommand::Freeze,
        "pause" => VMCommand::Pause,
        "s" | "step" => VMCommand::Step,
        "c" | "continue" => VMCommand::Continue,
        "rs" | "restart" => VMCommand::Restart,
        "b" | "break" if cmd.len() >= 2 => match cmd[1] {
            "clear" => VMCommand::SetBreakpoint(None),
            value => match u32::from_str_radix(value.trim_start_matches("0x"), 16) {
                Ok(value) => VMCommand::SetBreakpoint(Some(value)),
                Err(_) => {
                    tui_println!(out, "Invalid address.");
                    return None;
                }
            },
        },
        "p" | "print" | "v" | "var" if cmd.len() >= 2 => {
            VMCommand::PrintVar(cmd[1].to_string(), cmd.get(2).map(|s| s.to_string()))
        }
        /*
        "i" | "inspect" if cmd.len() >= 3 => {
            let Ok(x) = cmd[1].parse() else {
                println!("Invalid x.");
                return None;
            };
            let Ok(y) = cmd[2].parse() else {
                println!("Invalid y.");
                return None;
            };
            match vm.building((x, y).into()) {
                Some(b) => match b.data.try_borrow() {
                    Ok(data) => match &*data {
                        BuildingData::Processor(p) => match cmd.get(3) {
                            Some(&"*") => {
                                for name in p.variables.keys().sorted() {
                                    print_var(p, name, cmd.get(4));
                                }
                            }
                            Some(addr) => print_var(p, addr, cmd.get(4)),
                            None => println!("{}", b.block.name),
                        },
                        BuildingData::Memory(mem) => {
                            let i = cmd.get(3).copied().unwrap_or("").parse().unwrap_or(0);
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
        */
        _ => return None,
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    println!("Loading schematic...");

    let mut schematic = {
        let mut file = File::open(cli.schematic)?;
        Schematic::read(&mut file)?
    };

    println!("Loading metadata...");

    let meta: Metadata = serde_json::from_str(&schematic.tags["mlogv32_metadata"])?;

    if cli.verbose {
        println!("{meta:#?}");
    }

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
    let globals = LVar::create_globals();
    let vm = builder.build_with_globals(Cow::Borrowed(&globals))?;

    let uart_fifo_modulo = meta.uart_fifo_capacity + 1;

    let controller = get_building(&vm, meta.cpu, WORLD_PROCESSOR);
    let config = get_building(&vm, meta.config, MICRO_PROCESSOR);
    let uart0 = get_building(&vm, meta.uarts[0], MEMORY_BANK);
    let error_output = get_building(&vm, meta.error_output, MESSAGE);
    let power_switch = get_building(&vm, meta.power_switch, SWITCH);
    let pause_switch = get_building(&vm, meta.pause_switch, SWITCH);
    let single_step_switch = get_building(&vm, meta.single_step_switch, SWITCH);

    println!("Initializing processors...");

    let mut time = Duration::ZERO;
    for _ in 0..500 {
        vm.do_tick(time);
        time += Duration::from_secs_f64(1. / 60.);
    }

    // switch to TUI

    let stdout = TextContent::new("");
    let debug = TextContent::new("");

    let (tx_state, rx_state) = mpsc::channel();
    let (tx_cmd, rx_cmd) = mpsc::channel();

    {
        let stdout = stdout.clone();
        let debug = debug.clone();
        thread::spawn(move || {
            tui(stdout, debug, tx_cmd, rx_state);
        });
    }

    let print_var = |processor: &Processor, name: String, radix: Option<String>| {
        match processor
            .variable(&name)
            .or_else(|| globals.get(&name).map(|v| v.get(&processor.state)))
        {
            Some(value) => match radix.as_deref() {
                Some("x") => tui_println!(debug, "{name} = {:#010x}", value.num() as u32),
                Some("b") => tui_println!(debug, "{name} = {:#034b}", value.num() as u32),
                _ => tui_println!(debug, "{name} = {value:?}"),
            },
            None => tui_println!(debug, "{name} = <undefined>"),
        };
    };

    // start CPU

    if let BuildingData::Switch(power) = &mut *power_switch.data.borrow_mut()
        && let BuildingData::Switch(single_step) = &mut *single_step_switch.data.borrow_mut()
    {
        *power = true;
        *single_step = cli.step;
    }

    let mut prev_power = false;
    let mut frozen = false;
    let mut ticks = 0;
    let mut start = Instant::now();
    let mut next_state_update = Duration::ZERO;
    let state_update_interval = Duration::from_secs_f64(1. / 8.);

    loop {
        let time = start.elapsed();
        if !frozen {
            vm.do_tick(time);
            ticks += 1;
        }

        // UART0 terminal
        if let BuildingData::Memory(uart0) = &mut *uart0.data.borrow_mut() {
            let mut rx_read = (uart0[UART_RX_READ] as usize) % uart_fifo_modulo;
            let rx_write = (uart0[UART_RX_WRITE] as usize) % uart_fifo_modulo;
            if rx_read != rx_write {
                while rx_read != rx_write {
                    let c = uart0[UART_RX_START + rx_read] as u8;
                    stdout.append(c as char);
                    rx_read = (rx_read + 1) % uart_fifo_modulo;
                }
                uart0[UART_RX_READ] = rx_write as f64;
            }
        }

        if time >= next_state_update
            && let BuildingData::Switch(power) = &mut *power_switch.data.borrow_mut()
            && let BuildingData::Switch(pause) = &mut *pause_switch.data.borrow_mut()
            && let BuildingData::Switch(single_step) = &mut *single_step_switch.data.borrow_mut()
            && let BuildingData::Processor(controller) = &mut *controller.data.borrow_mut()
            && let BuildingData::Processor(config) = &mut *config.data.borrow_mut()
            && let BuildingData::Message(error_output) = &*error_output.data.borrow()
        {
            next_state_update = time + state_update_interval;

            // handle commands
            for cmd in rx_cmd.try_iter() {
                match cmd {
                    VMCommand::Exit => return Ok(()),
                    VMCommand::Freeze => {
                        frozen = true;
                    }
                    VMCommand::Pause => {
                        *pause = true;
                    }
                    VMCommand::Step => {
                        if *pause {
                            *pause = false;
                            *single_step = true;
                        }
                    }
                    VMCommand::Continue => {
                        frozen = false;
                        *pause = false;
                        *single_step = false;
                    }
                    VMCommand::Restart => {
                        controller.set_variable("pc", 0.into())?;
                        *power = true;
                        *pause = false;
                        *single_step = false;
                        start = Instant::now();
                    }
                    VMCommand::SetBreakpoint(Some(value)) => {
                        config.set_variable("BREAKPOINT_ADDRESS", value.into())?;
                        tui_println!(debug, "Breakpoint set: {value:#010x}");
                    }
                    VMCommand::SetBreakpoint(None) => {
                        config.set_variable("BREAKPOINT_ADDRESS", LValue::Null)?;
                        tui_println!(debug, "Breakpoint cleared.");
                    }
                    VMCommand::PrintVar(name, radix) => print_var(controller, name, radix),
                }
            }

            // send state change event
            tx_state.send(VMState {
                power: *power,
                pause: *pause,
                single_step: *single_step,
                state: match controller.variable("state").unwrap() {
                    LValue::String(state) => Some(state.to_string()),
                    _ => None,
                },
                pc: controller.variable("pc").unwrap().numu(),
                mcycle: controller.variable("csr_mcycle").unwrap().numu(),
                mtime: controller.variable("csr_mtime").unwrap().numu(),
                minstret: controller.variable("csr_minstret").unwrap().numu(),
            })?;

            if *power != prev_power {
                prev_power = *power;
                if *power {
                    tui_println!(debug, "Starting.");
                } else {
                    tui_println!(debug, "Processor halted.");
                    if !error_output.is_empty() {
                        tui_println!(debug, "Error output: {}", decode_utf16(error_output));
                    }
                    tui_println!(debug, "Runtime: {time:?}");
                    tui_println!(debug, "Ticks completed: {ticks}");
                    tui_println!(debug, "Average time per tick: {:?}", time / ticks);
                    tui_println!(
                        debug,
                        "Average ticks per second: {:.1}",
                        (ticks as f64) / time.as_secs_f64()
                    );
                }
            }
        }
    }
}
