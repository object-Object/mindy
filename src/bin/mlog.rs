use std::{error::Error, time::Instant};

use clap::Parser;
use clap_stdin::FileOrStdin;
use mindy::{
    types::{Object, ProcessorConfig, ProcessorLinkConfig},
    vm::{
        Building, BuildingData, LogicVMBuilder,
        buildings::{
            HYPER_PROCESSOR, LOGIC_PROCESSOR, MEMORY_BANK, MEMORY_CELL, MESSAGE, MICRO_PROCESSOR,
            WORLD_PROCESSOR,
        },
    },
};
use strum::EnumString;
use widestring::U16String;

#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Mlog code to load and run
    code: FileOrStdin,

    /// Processor type to use (micro, logic, hyper, world)
    #[arg(long, short, default_value_t = ProcessorType::World)]
    processor: ProcessorType,

    /// Simulated time delta (0.5 = 120 fps, 1 = 60 fps, 2 = 30 fps)
    #[arg(long, default_value_t = 1.0, value_parser = time_delta_parser)]
    delta: f64,

    /// Maximum number of ticks to run the simulation for
    #[arg(long)]
    max_ticks: Option<u32>,
}

fn time_delta_parser(s: &str) -> Result<f64, String> {
    match s.parse() {
        Ok(value) if value > 0. && value <= 6. => Ok(value),
        Ok(_) => Err(format!("{s} is not in range (0, 6]")),
        Err(_) => Err(format!("{s} is not a valid number")),
    }
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, EnumString, strum::Display)]
#[strum(serialize_all = "lowercase")]
enum ProcessorType {
    Micro,
    Logic,
    Hyper,
    World,
}

impl ProcessorType {
    fn name(&self) -> &str {
        match self {
            Self::Micro => MICRO_PROCESSOR,
            Self::Logic => LOGIC_PROCESSOR,
            Self::Hyper => HYPER_PROCESSOR,
            Self::World => WORLD_PROCESSOR,
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let mut builder = LogicVMBuilder::new();
    builder.add_buildings([
        Building::from_processor_config(
            cli.processor.name(),
            (0, 0).into(),
            &ProcessorConfig {
                code: cli.code.contents()?,
                links: vec![
                    ProcessorLinkConfig::unnamed(3, 0),
                    ProcessorLinkConfig::unnamed(4, 0),
                    ProcessorLinkConfig::unnamed(5, 0),
                ],
            },
            &builder,
        )?,
        Building::from_config(MESSAGE, (3, 0).into(), &Object::Null, &builder)?,
        Building::from_config(MEMORY_CELL, (4, 0).into(), &Object::Null, &builder)?,
        Building::from_config(MEMORY_BANK, (5, 0).into(), &Object::Null, &builder)?,
    ]);
    let mut vm = builder.build()?;

    let processor = vm.building((0, 0).into()).unwrap().clone();
    assert_eq!(processor.block.name.as_str(), cli.processor.name());

    let message = vm.building((3, 0).into()).unwrap().clone();
    assert_eq!(message.block.name.as_str(), MESSAGE);

    let start = Instant::now();
    let mut ticks = 0u32;
    let mut prev_message = U16String::new();

    let all_stopped = loop {
        vm.do_tick_with_delta(start.elapsed(), cli.delta);
        ticks += 1;

        if let BuildingData::Message(message) = &*message.data.borrow()
            && !message.is_empty()
            && *message != prev_message
        {
            println!("{}", message.display());
            prev_message = message.clone();
        }

        if vm.running_processors() == 0 {
            break true;
        }

        if let Some(max_ticks) = cli.max_ticks
            && ticks >= max_ticks
        {
            break false;
        }
    };

    if let BuildingData::Processor(processor) = &*processor.data.borrow()
        && !processor.state.printbuffer.is_empty()
    {
        println!("{}", processor.state.printbuffer.display());
    }

    if all_stopped {
        println!("--------\nAll processors stopped, halting.");
    } else {
        println!("--------\nTick limit reached, halting.");
    }

    let time = start.elapsed();
    println!("Runtime: {time:?}");
    println!("Ticks completed: {ticks}");
    println!("Average time per tick: {:?}", time / ticks);
    println!(
        "Average ticks per second: {:.1}",
        (ticks as f64) / time.as_secs_f64()
    );

    Ok(())
}
