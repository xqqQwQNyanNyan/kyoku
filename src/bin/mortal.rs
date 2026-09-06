use std::env;
use std::error::Error;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

use convlog::{tenhou::Log, tenhou_to_mjai};
use kyoku::mahjong::player_index::PlayerIndex;
use kyoku::mortal::{Mortal, MortalConfig};
use kyoku::replay::{inspector::format_event, replayer::Replayer};

#[path = "common/mortal_output.rs"]
mod mortal_output;
use mortal_output::write_decision;

const USAGE: &str = "Usage: cargo run --bin mortal -- --player <0..3> [OPTIONS] <tenhou-json|->

Replay a four-player Tenhou log and print Mortal decisions after each event.
Event indices are zero-based, matching the replay command.

Options:
  --event N          evaluate only the decision after event N (replay all earlier events)
  --python PATH      Python with torch/numpy (default: mortal/.venv/bin/python)
  --runtime PATH     compiled official Mortal checkout (default: mortal/runtime)
  --model PATH       V4 checkpoint (default: mortal/models/mortal_582500.pth)
  -h, --help         show help
";

fn main() {
    if let Err(error) = run() {
        eprintln!("mortal: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let Some(args) = Args::parse(env::args().skip(1))? else {
        print!("{USAGE}");
        return Ok(());
    };
    let json = if args.input == "-" {
        let mut json = String::new();
        io::stdin().read_to_string(&mut json)?;
        json
    } else {
        fs::read_to_string(&args.input)?
    };
    let log = Log::from_json_str(&json)?;
    let events = tenhou_to_mjai(&log)?;
    if args.event.is_some_and(|index| index >= events.len()) {
        return Err("--event is outside the event log".into());
    }
    let config = MortalConfig {
        python: &args.python,
        runtime: &args.runtime,
        checkpoint: &args.model,
    };
    let mut mortal = Mortal::start(&config, args.player)?;
    println!(
        "Model: {} (V{}, sha256={})",
        mortal.model().tag,
        mortal.model().version,
        mortal.model().sha256
    );
    let mut replayer = Replayer::new();
    let mut decisions = 0;
    for (index, event) in events.iter().enumerate() {
        replayer
            .apply(event)
            .map_err(|error| format!("replay at G{index:03}: {error}"))?;
        let decision = mortal
            .react(event)
            .map_err(|error| format!("inference at G{index:03}: {error}"))?;
        if args.event.is_none_or(|target| target == index) {
            if let Some(decision) = decision {
                println!("\nG{index:03} after {}", format_event(event));
                write_decision(io::stdout().lock(), &decision)?;
                decisions += 1;
            } else if args.event.is_some() {
                println!(
                    "G{index:03}: no decision opportunity for P{}",
                    args.player.get_id()
                );
            }
        }
        if args.event == Some(index) {
            break;
        }
    }
    mortal.finish()?;
    println!(
        "\n{decisions} decisions shown for P{}",
        args.player.get_id()
    );
    Ok(())
}

struct Args {
    player: PlayerIndex,
    event: Option<usize>,
    python: PathBuf,
    runtime: PathBuf,
    model: PathBuf,
    input: String,
}

#[cfg(test)]
#[path = "mortal/tests.rs"]
mod tests;

impl Args {
    fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Option<Self>, Box<dyn Error>> {
        let mut player = None;
        let mut event = None;
        let mut python = PathBuf::from("mortal/.venv/bin/python");
        let mut runtime = PathBuf::from("mortal/runtime");
        let mut model = PathBuf::from("mortal/models/mortal_582500.pth");
        let mut input = None;
        let mut arguments = arguments.into_iter();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "-h" | "--help" => return Ok(None),
                "--player" | "--event" | "--python" | "--runtime" | "--model" => {
                    let value = arguments
                        .next()
                        .ok_or_else(|| format!("missing value for {argument}"))?;
                    match argument.as_str() {
                        "--player" => player = Some(PlayerIndex::try_from(value.parse::<u8>()?)?),
                        "--event" => event = Some(value.parse::<usize>()?),
                        "--python" => python = value.into(),
                        "--runtime" => runtime = value.into(),
                        "--model" => model = value.into(),
                        _ => unreachable!(),
                    }
                }
                value if value.starts_with('-') && value != "-" => {
                    return Err(format!("unknown option {value}").into());
                }
                _ if input.is_none() => input = Some(argument),
                _ => return Err("expected only one input".into()),
            }
        }
        Ok(Some(Self {
            player: player.ok_or("missing --player (0..3)")?,
            event,
            python,
            runtime,
            model,
            input: input.ok_or("missing Tenhou JSON input")?,
        }))
    }
}
