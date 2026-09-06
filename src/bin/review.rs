use std::env;
use std::error::Error;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

use convlog::{tenhou::Log, tenhou_to_mjai};
use kyoku::mahjong::player_index::PlayerIndex;
use kyoku::mortal::MortalConfig;
use kyoku::review::{review_at, review_game};
use review_output::write_review;

#[path = "common/decision_output.rs"]
mod decision_output;
#[path = "common/mortal_output.rs"]
mod mortal_output;
#[path = "common/review_output.rs"]
mod review_output;

const USAGE: &str =
    "Usage: cargo run --bin review -- --player <0..3> [--event N] [OPTIONS] <tenhou-json|->

Review the visible position, discard efficiency and Mortal decision after event N.
Without --event, list every decision in the game.
Event indices are zero-based, matching the replay command.

Options:
  --python PATH      Python with torch/numpy (default: mortal/.venv/bin/python)
  --runtime PATH     compiled official Mortal checkout (default: mortal/runtime)
  --model PATH       V4 checkpoint (default: mortal/models/mortal_582500.pth)
  -h, --help         show help
";

fn main() {
    if let Err(error) = run() {
        eprintln!("review: {error}");
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
    let events = tenhou_to_mjai(&Log::from_json_str(&json)?)?;
    let config = MortalConfig {
        python: &args.python,
        runtime: &args.runtime,
        checkpoint: &args.model,
    };
    if let Some(event) = args.event {
        let review = review_at(&events, args.player, event, &config)?;
        write_review(io::stdout().lock(), &review)?;
    } else {
        eprintln!("正在推理整场牌谱，模型只加载一次…");
        let game = review_game(&events, args.player, &config)?;
        decision_output::write_decisions(io::stdout().lock(), game.decisions())?;
    }
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

#[cfg(test)]
#[path = "review/tests.rs"]
mod tests;
