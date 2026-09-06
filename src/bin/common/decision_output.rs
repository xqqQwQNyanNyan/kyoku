use std::io::{self, Write};

use kyoku::replay::inspector::format_event;
use kyoku::review::{DecisionPoint, RecordedAction};

pub(super) fn write_decisions(mut output: impl Write, points: &[DecisionPoint]) -> io::Result<()> {
    writeln!(
        output,
        "共 {} 个行动机会（手番按自家牌河长度 + 1 计）",
        points.len()
    )?;
    for point in points {
        let review = &point.review;
        let actual = match &point.actual {
            RecordedAction::Taken { action, .. } => format_event(action),
            RecordedAction::Passed => "跳过".into(),
            RecordedAction::Unresolved => "无法确定（他家抢先行动、流局或牌谱截断）".into(),
        };
        let recommended = review
            .decision
            .as_ref()
            .map(|decision| format_event(&decision.recommended))
            .unwrap_or_else(|| "无行动机会".into());
        writeln!(
            output,
            "G{:03}  {:?}{} {}本场  P{} 第{}手番  实际: {}  Mortal: {}",
            review.event_index,
            review.position.round.wind(),
            review.position.round.number(),
            review.position.honba,
            review.player.get_id(),
            point.turn,
            actual,
            recommended
        )?;
    }
    Ok(())
}
