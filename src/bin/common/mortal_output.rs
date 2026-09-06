use std::io::{self, Write};

use kyoku::mortal::{Action, Decision};
use kyoku::replay::inspector::format_event;

pub(super) fn write_decision(mut output: impl Write, mut decision: Decision) -> io::Result<()> {
    writeln!(output, "Mortal: {}", format_event(&decision.recommended))?;
    writeln!(
        output,
        "shanten={:?} furiten={:?}",
        decision.shanten, decision.at_furiten
    )?;
    decision
        .candidates
        .sort_by(|a, b| b.q_value.total_cmp(&a.q_value));
    writeln!(
        output,
        "Candidates (raw Q, descending; final Mortal action above takes precedence):"
    )?;
    for candidate in &decision.candidates {
        let label = match candidate.action {
            Action::Discard(tile) => {
                // 领域与 convlog 的 37 种实体牌编码一致。
                format!(
                    "discard {}",
                    convlog::Tile::try_from(tile.as_u8()).expect("valid domain tile")
                )
            }
            Action::Kan if decision.kan_candidates.len() == 1 => {
                let tile = convlog::Tile::try_from(decision.kan_candidates[0].tile.as_u8())
                    .expect("valid domain tile kind");
                // 候选只有牌种，不能据此区分暗杠和加杠。
                format!("Kan {tile}")
            }
            action => format!("{action:?}"),
        };
        let source = if candidate.action == Action::Kan {
            " (main)"
        } else {
            ""
        };
        writeln!(output, "  {label:<14} Q={:.5}{source}", candidate.q_value)?;
    }
    // 单候选不存在“杠哪个”的比较，展示主层评价即可。
    if decision.kan_candidates.len() > 1 {
        writeln!(output, "  Kan selection (separate evaluation):")?;
        for candidate in &decision.kan_candidates {
            let tile =
                convlog::Tile::try_from(candidate.tile.as_u8()).expect("valid domain tile kind");
            writeln!(output, "    {tile} Q={:.5}", candidate.q_value)?;
        }
    }
    Ok(())
}
