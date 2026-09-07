//! 只投影当前局已经发生的公开事件；摸牌事件不保留任何人的牌值。

use crate::mahjong::{player_index::PlayerIndex, tile::Tile};
use convlog::Event;

/// 当前局内的公开动作及它在原牌谱中的位置。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicEvent {
    pub event_index: usize,
    pub player: PlayerIndex,
    pub action: PublicAction,
}

/// 防守与事件役所需的最小动作信息，不包含配牌、摸牌值或结算数据。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublicAction {
    Draw,
    Discard { tile: Tile },
    Call,
    RiichiDeclared,
    RiichiAccepted,
}

/// 输入必须是已经回放校验的历史前缀；None 表示没有当前局的开始事件。
pub(crate) fn public_history(events: &[Event]) -> Option<Vec<PublicEvent>> {
    let start = events
        .iter()
        .rposition(|event| matches!(event, Event::StartKyoku { .. }))?;
    Some(
        events
            .iter()
            .enumerate()
            .skip(start)
            .filter_map(|(event_index, event)| {
                let (actor, action) = match event {
                    Event::Tsumo { actor, .. } => (*actor, PublicAction::Draw),
                    Event::Dahai { actor, pai, .. } => (
                        *actor,
                        PublicAction::Discard {
                            tile: Tile::new(pai.as_u8())
                                .unwrap_or_else(|| unreachable!("回放已校验弃牌")),
                        },
                    ),
                    Event::Chi { actor, .. }
                    | Event::Pon { actor, .. }
                    | Event::Daiminkan { actor, .. }
                    | Event::Ankan { actor, .. }
                    | Event::Kakan { actor, .. } => (*actor, PublicAction::Call),
                    Event::Reach { actor } => (*actor, PublicAction::RiichiDeclared),
                    Event::ReachAccepted { actor } => (*actor, PublicAction::RiichiAccepted),
                    _ => return None,
                };
                Some(PublicEvent {
                    event_index,
                    player: PlayerIndex::new(actor)
                        .unwrap_or_else(|| unreachable!("回放已校验玩家")),
                    action,
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_resets_at_round_start_and_never_keeps_draw_values() {
        let log = convlog::tenhou::Log::from_json_str(include_str!(
            "../../fixtures/tenhou/ranked_game.json"
        ))
        .unwrap();
        let events = convlog::tenhou_to_mjai(&log).unwrap();
        let starts: Vec<_> = events
            .iter()
            .enumerate()
            .filter_map(|(i, e)| matches!(e, Event::StartKyoku { .. }).then_some(i))
            .collect();
        let end = starts[1] + 1;
        let history = public_history(&events[..=end]).unwrap();
        assert!(
            history
                .iter()
                .all(|e| e.event_index > starts[1] && e.event_index <= end)
        );
        let mut changed = events[..=end].to_vec();
        for event in &mut changed {
            if let Event::Tsumo { pai, .. } = event {
                *pai = convlog::Tile::try_from(33u8).unwrap();
            }
        }
        assert_eq!(public_history(&changed), Some(history));
        assert_eq!(public_history(&[]), None);
    }

    #[test]
    fn agent_context_projects_only_the_requested_prefix() {
        let log = convlog::tenhou::Log::from_json_str(include_str!(
            "../../fixtures/tenhou/ranked_game.json"
        ))
        .unwrap();
        let events = convlog::tenhou_to_mjai(&log).unwrap();
        let end = events
            .iter()
            .position(|e| matches!(e,Event::Tsumo{actor,..} if *actor!=0))
            .unwrap();
        let p = PlayerIndex::new(0).unwrap();
        let context = crate::agent::AgentContext::from_events(&events, p, end).unwrap();
        let prefix = crate::agent::AgentContext::from_events(&events[..=end], p, end).unwrap();
        assert_eq!(context.evidence(), prefix.evidence());
        let history = context.evidence()["position"]["history"]
            .as_array()
            .unwrap();
        for e in history {
            assert!(e["event_index"].as_u64().unwrap() <= end as u64);
            if e["kind"] == "draw" {
                assert!(e["tile"].is_null());
            }
        }
    }
}
