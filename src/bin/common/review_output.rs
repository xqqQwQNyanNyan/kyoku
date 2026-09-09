use super::mortal_output;
use kyoku::analysis::DrawCandidates;
use kyoku::mahjong::{meld::Meld, tile::Tile};
use kyoku::replay::inspector::{format_phase, format_tile};
use kyoku::review::Review;
use std::io::{self, Write};

pub(super) fn write_review(mut output: impl Write, review: &Review) -> io::Result<()> {
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
    match &review.decision {
        Some(decision) => mortal_output::write_decision(&mut output, decision)?,
        None => writeln!(
            output,
            "Mortal: no decision opportunity for P{}",
            review.player.get_id()
        )?,
    }
    Ok(())
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
