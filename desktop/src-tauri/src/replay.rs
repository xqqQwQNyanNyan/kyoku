use convlog::{Event, tenhou::Log, tenhou_to_mjai};
use kyoku::mahjong::{meld::Meld, player::RiichiState, round::RoundPhase};
use kyoku::replay::{inspector::format_tile, replayer::Replayer};
use serde::Serialize;

use crate::UiError;

pub(crate) const MAX_LOG_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Serialize)]
pub(crate) struct DiscardView {
    tile: String,
    tsumogiri: bool,
    riichi: bool,
    called: bool,
}

#[derive(Clone, Serialize)]
pub(crate) struct MeldView {
    kind: &'static str,
    tiles: Vec<String>,
    called: Option<String>,
    from: Option<u8>,
}

#[derive(Clone, Serialize)]
pub(crate) struct PlayerView {
    score: i32,
    riichi: bool,
    concealed: Vec<String>,
    discards: Vec<DiscardView>,
    melds: Vec<MeldView>,
}

#[derive(Clone, Serialize)]
pub(crate) struct EventView {
    kind: &'static str,
    actor: Option<u8>,
    target: Option<u8>,
    tile: Option<String>,
}

#[derive(Clone, Serialize)]
pub(crate) struct Frame {
    pub event_index: usize,
    pub round: String,
    pub honba: u8,
    dealer: u8,
    riichi_sticks: u8,
    remaining_draws: u8,
    dora_indicators: Vec<String>,
    active_player: Option<u8>,
    settled: bool,
    drawn: Option<(u8, String)>,
    players: [PlayerView; 4],
    event: EventView,
}

#[derive(Clone, Serialize)]
pub(crate) struct RoundEntry {
    frame_index: usize,
    label: String,
}

#[derive(Serialize)]
pub(crate) struct ReplayData {
    pub mortal_supported: bool,
    pub names: [String; 4],
    pub frames: Vec<Frame>,
    pub rounds: Vec<RoundEntry>,
}

pub(crate) fn parse(json: &str) -> Result<(Vec<Event>, ReplayData), UiError> {
    if json.len() > MAX_LOG_BYTES {
        return Err(UiError::new("log_too_large", "牌谱文件不能超过 16 MiB"));
    }
    let log = Log::from_json_str(json)
        .map_err(|_| UiError::new("invalid_json", "无法读取天凤 JSON 牌谱，请检查文件格式"))?;
    let events = tenhou_to_mjai(&log)
        .map_err(|error| UiError::new("conversion", format!("牌谱转换失败：{error}")))?;
    let data = replay(&events)?;
    Ok((events, data))
}

fn replay(events: &[Event]) -> Result<ReplayData, UiError> {
    let mut replayer = Replayer::new();
    let mut names = std::array::from_fn(|i| format!("玩家 {}", i + 1));
    let mut frames = Vec::new();
    let mut rounds = Vec::new();
    let mut drawn = None;
    for (event_index, event) in events.iter().enumerate() {
        if let Event::StartGame { names: players, .. } = event {
            names = players.clone();
        }
        replayer.apply(event).map_err(|error| {
            UiError::at("replay", format!("局面重建失败：{error}"), event_index)
        })?;
        let Some(state) = replayer.state() else {
            continue;
        };
        match event {
            Event::Tsumo { actor, pai } => drawn = Some((*actor, pai.to_string())),
            Event::Reach { .. } | Event::ReachAccepted { .. } | Event::Dora { .. } => {}
            _ => drawn = None,
        }
        let round = format!(
            "{}{}局",
            match state.round().wind() {
                kyoku::mahjong::round::Wind::East => "东",
                kyoku::mahjong::round::Wind::South => "南",
                kyoku::mahjong::round::Wind::West => "西",
                kyoku::mahjong::round::Wind::North => "北",
            },
            ["一", "二", "三", "四"][usize::from(state.round().number() - 1)]
        );
        if matches!(event, Event::StartKyoku { .. }) {
            rounds.push(RoundEntry {
                frame_index: frames.len(),
                label: format!("{round} · {} 本场", state.honba()),
            });
        }
        let active_player = match state.phase() {
            RoundPhase::AfterDraw { player, .. }
            | RoundPhase::AfterDiscard { player }
            | RoundPhase::AfterCall { player }
            | RoundPhase::AfterKanDeclaration { player, .. } => Some(player.get_id()),
            _ => None,
        };
        frames.push(Frame {
            event_index,
            round,
            honba: state.honba(),
            dealer: state.round().dealer().get_id(),
            riichi_sticks: state.riichi_sticks(),
            remaining_draws: state.remaining_draws(),
            dora_indicators: state
                .dora_indicators()
                .iter()
                .copied()
                .map(format_tile)
                .collect(),
            active_player,
            settled: matches!(
                state.phase(),
                RoundPhase::AwaitingEnd(_) | RoundPhase::Ended(_)
            ),
            drawn: drawn.clone(),
            players: std::array::from_fn(|i| {
                let player = &state.players()[i];
                PlayerView {
                    score: player.score(),
                    riichi: player.riichi() != RiichiState::NotDeclared,
                    concealed: player
                        .hand()
                        .concealed()
                        .iter()
                        .copied()
                        .map(format_tile)
                        .collect(),
                    discards: player
                        .discards()
                        .iter()
                        .map(|d| DiscardView {
                            tile: format_tile(d.tile()),
                            tsumogiri: d.is_tsumogiri(),
                            riichi: d.is_riichi(),
                            called: d.is_called(),
                        })
                        .collect(),
                    melds: player
                        .hand()
                        .melds()
                        .iter()
                        .map(|m| MeldView {
                            kind: match m {
                                Meld::Chi { .. } => "chi",
                                Meld::Pon { .. } => "pon",
                                Meld::Daiminkan { .. } => "daiminkan",
                                Meld::Ankan { .. } => "ankan",
                                Meld::Kakan { .. } => "kakan",
                            },
                            tiles: m.tiles().iter().copied().map(format_tile).collect(),
                            called: m.called().map(format_tile),
                            from: m.from().map(|p| p.get_id()),
                        })
                        .collect(),
                }
            }),
            event: event_view(event),
        });
    }
    if frames.is_empty() {
        return Err(UiError::new("empty_log", "牌谱中没有可回放的局面"));
    }
    Ok(ReplayData {
        mortal_supported: matches!(
            events.first(),
            Some(Event::StartGame { kyoku_first: 0, .. })
        ),
        names,
        frames,
        rounds,
    })
}

fn event_view(event: &Event) -> EventView {
    let (kind, actor, target, tile) = match event {
        Event::Tsumo { actor, pai } => ("tsumo", Some(*actor), None, Some(pai.to_string())),
        Event::Dahai { actor, pai, .. } => ("dahai", Some(*actor), None, Some(pai.to_string())),
        Event::Chi {
            actor, target, pai, ..
        } => ("chi", Some(*actor), Some(*target), Some(pai.to_string())),
        Event::Pon {
            actor, target, pai, ..
        } => ("pon", Some(*actor), Some(*target), Some(pai.to_string())),
        Event::Daiminkan {
            actor, target, pai, ..
        } => (
            "daiminkan",
            Some(*actor),
            Some(*target),
            Some(pai.to_string()),
        ),
        Event::Ankan { actor, .. } => ("ankan", Some(*actor), None, None),
        Event::Kakan { actor, pai, .. } => ("kakan", Some(*actor), None, Some(pai.to_string())),
        Event::Reach { actor } => ("reach", Some(*actor), None, None),
        Event::ReachAccepted { actor } => ("reach_accepted", Some(*actor), None, None),
        Event::Hora { actor, target, .. } => ("hora", Some(*actor), Some(*target), None),
        Event::Dora { dora_marker } => ("dora", None, None, Some(dora_marker.to_string())),
        Event::Ryukyoku { .. } => ("ryukyoku", None, None, None),
        Event::StartKyoku { .. } => ("start_kyoku", None, None, None),
        Event::EndKyoku => ("end_kyoku", None, None, None),
        Event::EndGame => ("end_game", None, None, None),
        _ => ("none", None, None, None),
    };
    EventView {
        kind,
        actor,
        target,
        tile,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_replay_keeps_non_decision_events_and_final_scores() {
        let (events, data) =
            parse(include_str!("../../../fixtures/tenhou/ranked_game.json")).unwrap();
        let mut replayer = Replayer::new();
        for event in &events {
            replayer.apply(event).unwrap();
        }
        let last = data.frames.last().unwrap();
        assert_eq!(last.event_index, events.len() - 1);
        for (view, player) in last.players.iter().zip(replayer.state().unwrap().players()) {
            assert_eq!(view.score, player.score());
        }
        for round in &data.rounds {
            assert_eq!(data.frames[round.frame_index].event.kind, "start_kyoku");
        }
        for frame in &data.frames {
            if let Event::Tsumo { actor, pai } = &events[frame.event_index] {
                assert_eq!(frame.drawn, Some((*actor, pai.to_string())));
                assert!(
                    frame.players[usize::from(*actor)]
                        .concealed
                        .contains(&pai.to_string())
                );
            }
        }
    }

    #[test]
    fn invalid_inputs_are_errors() {
        assert_eq!(parse("not json").err().unwrap().code, "invalid_json");
        assert_eq!(replay(&[]).err().unwrap().code, "empty_log");
        let error = replay(&[Event::Tsumo {
            actor: 0,
            pai: convlog::Tile::try_from(0u8).unwrap(),
        }])
        .err()
        .unwrap();
        assert_eq!(error.event_index, Some(0));
    }

    #[test]
    fn east_only_records_remain_replayable_but_are_not_marked_for_mortal() {
        let sample =
            include_str!("../../../services/majsoul/test/fixtures/ranked-round.tenhou.json");
        let mut log: serde_json::Value = serde_json::from_str(sample).unwrap();
        for (rule, supported) in [("四般東喰赤", false), ("四般南喰赤", true)] {
            log["rule"]["disp"] = rule.into();
            let (_, data) = parse(&log.to_string()).unwrap();
            assert_eq!(data.mortal_supported, supported);
            assert!(!data.frames.is_empty());
        }
    }
}
