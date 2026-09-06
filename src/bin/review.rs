use std::env;
use std::error::Error;
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use convlog::{tenhou::Log, tenhou_to_mjai};
use kyoku::analysis::DrawCandidates;
use kyoku::mahjong::{meld::Meld, player_index::PlayerIndex, tile::Tile};
use kyoku::mortal::MortalConfig;
use kyoku::replay::inspector::{format_phase, format_tile};
use kyoku::review::{Review, review_at};

#[path = "common/mortal_output.rs"]
mod mortal_output;

const USAGE: &str =
    "Usage: cargo run --bin review -- --player <0..3> --event N [OPTIONS] <tenhou-json|->

Review the visible position, discard efficiency and Mortal decision after event N.
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
    let review = review_at(&events, args.player, args.event, &config)?;
    write_review(io::stdout().lock(), review)?;
    Ok(())
}

fn write_review(mut output: impl Write, review: Review) -> io::Result<()> {
    let position = &review.position;
    writeln!(
        output,
        "G{:03} after event, P{} view",
        review.event_index,
        review.player.get_id()
    )?;
    writeln!(
        output,
        "Round: {:?}{} honba={} riichi_sticks={} remaining_draws={}",
        position.round.wind(),
        position.round.number(),
        position.honba,
        position.riichi_sticks,
        position.remaining_draws
    )?;
    writeln!(output, "Phase: {}", format_phase(position.phase))?;
    writeln!(
        output,
        "Dora indicators: {}",
        tiles(&position.dora_indicators)
    )?;
    writeln!(output, "Concealed: {}", tiles(&position.concealed))?;
    for (index, player) in position.players.iter().enumerate() {
        writeln!(
            output,
            "P{index}: score={} riichi={:?}",
            player.score, player.riichi
        )?;
        let river = player
            .discards
            .iter()
            .map(|discard| {
                let mut label = format_tile(discard.tile());
                if discard.is_tsumogiri() {
                    label.push_str("(tsumogiri)");
                }
                if discard.is_riichi() {
                    label.push_str("(riichi)");
                }
                if discard.is_called() {
                    label.push_str("(called)");
                }
                label
            })
            .collect::<Vec<_>>()
            .join(" ");
        writeln!(output, "  River: [{river}]")?;
        let melds = player
            .melds
            .iter()
            .map(|meld| {
                let kind = match meld {
                    Meld::Chi { .. } => "chi",
                    Meld::Pon { .. } => "pon",
                    Meld::Daiminkan { .. } => "daiminkan",
                    Meld::Ankan { .. } => "ankan",
                    Meld::Kakan { .. } => "kakan",
                };
                let mut label = format!("{kind}{}", tiles(meld.tiles()));
                if let (Some(called), Some(from)) = (meld.called(), meld.from()) {
                    label.push_str(&format!(
                        "(called={} from=P{})",
                        format_tile(called),
                        from.get_id()
                    ));
                }
                label
            })
            .collect::<Vec<_>>()
            .join(" ");
        writeln!(output, "  Melds: [{melds}]")?;
    }
    writeln!(
        output,
        "\nDiscard efficiency (Mortal discard candidates only):"
    )?;
    if review.discards.is_empty() {
        writeln!(output, "  No discard candidates at this event.")?;
    }
    for discard in &review.discards {
        let (label, candidates) = match &discard.candidates {
            DrawCandidates::Effective(tiles) => ("effective", tiles),
            DrawCandidates::Winning(tiles) => ("winning shape", tiles),
        };
        let candidates = candidates
            .iter()
            .map(|candidate| {
                format!(
                    "{}:{}",
                    format_tile(Tile::new(candidate.kind.as_u8()).expect("valid tile kind")),
                    candidate.unseen
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        writeln!(
            output,
            "  discard {}: shanten={} unseen={} {label}=[{candidates}]",
            format_tile(discard.discard),
            discard.shanten,
            discard.total_unseen
        )?;
    }
    writeln!(
        output,
        "Unseen counts include opponents' concealed tiles; they are not live-wall counts."
    )?;
    writeln!(
        output,
        "Winning shape does not establish yaku or legal agari."
    )?;
    writeln!(
        output,
        "\nModel: {} (V{}, sha256={})",
        review.model.tag, review.model.version, review.model.sha256
    )?;
    match review.decision {
        Some(decision) => mortal_output::write_decision(&mut output, decision)?,
        None => writeln!(
            output,
            "Mortal: no decision opportunity for P{}",
            review.player.get_id()
        )?,
    }
    writeln!(
        output,
        "Raw Q values are not probabilities or expected points."
    )
}

fn tiles(values: &[Tile]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .copied()
            .map(format_tile)
            .collect::<Vec<_>>()
            .join(" ")
    )
}

struct Args {
    player: PlayerIndex,
    event: usize,
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
            event: event.ok_or("missing --event")?,
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
