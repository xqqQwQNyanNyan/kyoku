use convlog::{Event, tenhou::Log, tenhou_to_mjai};
use kyoku::mahjong::{meld::Meld, player::RiichiState, round::RoundPhase};
use kyoku::replay::{inspector::format_tile, replayer::Replayer};
use serde::{Deserialize, Serialize};

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
    result: Option<RoundResult>,
}

#[derive(Clone, Serialize)]
struct RoundResult {
    wins: Vec<(u8, u8)>,
    deltas: [i32; 4],
    scores: [i32; 4],
    details: Option<RoundDetails>,
}

/// 原始牌谱中的结算说明；事件流本身不保留役种和流局原因。
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum RoundDetails {
    Hora { wins: Vec<WinDetails> },
    Ryukyoku { reason: String },
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WinDetails {
    actor: u8,
    target: u8,
    score: Option<String>,
    yaku: Vec<String>,
}

impl ReplayData {
    pub(crate) fn round_details(&self) -> Vec<RoundDetails> {
        self.rounds
            .iter()
            .filter_map(|round| round.result.as_ref()?.details.clone())
            .collect()
    }

    pub(crate) fn with_details(mut self, details: &[RoundDetails]) -> Result<Self, UiError> {
        if details.is_empty() {
            return Ok(self);
        }
        let invalid = || UiError::new("replay_result", "牌谱结算详情与事件不匹配");
        if details.len() != self.rounds.len() {
            return Err(invalid());
        }
        for (round, detail) in self.rounds.iter_mut().zip(details) {
            let result = round.result.as_mut().ok_or_else(invalid)?;
            let valid_text = |text: &str| !text.is_empty() && text.len() <= 512;
            let valid = match detail {
                RoundDetails::Hora { wins } => {
                    !wins.is_empty()
                        && wins.len() == result.wins.len()
                        && wins.len() <= 3
                        && wins
                            .iter()
                            .zip(&result.wins)
                            .all(|(win, &(actor, target))| {
                                (win.actor, win.target) == (actor, target)
                                    && win.score.as_deref().is_none_or(valid_text)
                                    && win.yaku.len() <= 64
                                    && win.yaku.iter().all(|s| valid_text(s))
                            })
                }
                RoundDetails::Ryukyoku { reason } => result.wins.is_empty() && valid_text(reason),
            };
            if !valid {
                return Err(invalid());
            }
            result.details = Some(detail.clone());
        }
        Ok(self)
    }
}

fn parse_details(json: &str) -> Result<Vec<RoundDetails>, UiError> {
    let invalid = || UiError::new("replay_result", "无法读取牌谱结算详情");
    let value: serde_json::Value = serde_json::from_str(json).map_err(|_| invalid())?;
    value["log"]
        .as_array()
        .ok_or_else(invalid)?
        .iter()
        .map(|round| {
            let result = round
                .as_array()
                .and_then(|r| r.last())
                .and_then(|r| r.as_array())
                .ok_or_else(invalid)?;
            let reason = result
                .first()
                .and_then(|r| r.as_str())
                .ok_or_else(invalid)?;
            if reason != "和了" {
                return Ok(RoundDetails::Ryukyoku {
                    reason: reason.into(),
                });
            }
            let mut wins = Vec::new();
            for pair in result[1..].chunks(2) {
                let detail = pair.get(1).and_then(|r| r.as_array()).ok_or_else(invalid)?;
                let player = |i: usize| {
                    detail
                        .get(i)
                        .and_then(|v| v.as_u64())
                        .filter(|p| *p < 4)
                        .map(|p| p as u8)
                        .ok_or_else(invalid)
                };
                wins.push(WinDetails {
                    actor: player(0)?,
                    target: player(1)?,
                    score: detail
                        .get(3)
                        .map(|v| v.as_str().map(String::from).ok_or_else(invalid))
                        .transpose()?,
                    yaku: detail
                        .get(4..)
                        .unwrap_or_default()
                        .iter()
                        .map(|v| v.as_str().map(String::from).ok_or_else(invalid))
                        .collect::<Result<_, _>>()?,
                });
            }
            Ok(RoundDetails::Hora { wins })
        })
        .collect()
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
    let data = replay(&events)?.with_details(&parse_details(json)?)?;
    Ok((events, data))
}

pub(crate) fn replay(events: &[Event]) -> Result<ReplayData, UiError> {
    let mut replayer = Replayer::new();
    let mut names = std::array::from_fn(|i| format!("玩家 {}", i + 1));
    let mut frames = Vec::new();
    let mut rounds = Vec::new();
    let mut drawn = None;
    let mut wins = Vec::new();
    let mut deltas = [0; 4];
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
            wins.clear();
            deltas = [0; 4];
            rounds.push(RoundEntry {
                frame_index: frames.len(),
                label: format!("{round} · {} 本场", state.honba()),
                result: None,
            });
        }
        if let Event::Hora { actor, target, .. } = event {
            wins.push((*actor, *target));
        }
        if let Event::Hora {
            deltas: Some(change),
            ..
        }
        | Event::Ryukyoku {
            deltas: Some(change),
        } = event
        {
            for (total, change) in deltas.iter_mut().zip(change) {
                *total += change;
            }
        }
        if matches!(event, Event::EndKyoku)
            && let Some(round) = rounds.last_mut()
        {
            round.result = Some(RoundResult {
                wins: wins.clone(),
                deltas,
                scores: std::array::from_fn(|i| state.players()[i].score()),
                details: None,
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
    fn settlement_keeps_source_yaku_and_aggregates_multiple_wins() {
        let (mut events, data) =
            parse(include_str!("../../../fixtures/tenhou/ranked_game.json")).unwrap();
        let result = data.rounds[0].result.as_ref().unwrap();
        assert_eq!(result.wins, [(3, 2)]);
        assert_eq!(result.deltas, [0, 0, -7700, 7700]);
        let RoundDetails::Hora { wins } = result.details.as_ref().unwrap() else {
            panic!("和牌详情缺失");
        };
        assert_eq!(wins[0].score.as_deref(), Some("30符4飜7700点"));
        assert_eq!(wins[0].yaku, ["役牌 發(1飜)", "混一色(2飜)", "赤ドラ(1飜)"]);
        let end = events
            .iter()
            .position(|event| matches!(event, Event::EndKyoku))
            .unwrap();
        events.truncate(end + 1);
        events.insert(
            end,
            Event::Hora {
                actor: 0,
                target: 2,
                deltas: Some([8000, 0, -8000, 0]),
                ura_markers: None,
            },
        );
        let replay = replay(&events).unwrap();
        let result = replay.rounds[0].result.as_ref().unwrap();
        assert_eq!(result.wins, [(3, 2), (0, 2)]);
        assert_eq!(result.deltas, [8000, 0, -15700, 7700]);
        assert_eq!(
            result.scores,
            replay
                .frames
                .last()
                .unwrap()
                .players
                .clone()
                .map(|p| p.score)
        );
    }

    #[test]
    fn draw_reason_and_invalid_source_details() {
        let original = include_str!("../../../fixtures/tenhou/ranked_game.json");
        let mut json: serde_json::Value = serde_json::from_str(original).unwrap();
        let first_round = json["log"][0].as_array_mut().unwrap();
        *first_round.last_mut().unwrap() = serde_json::json!(["九種九牌", [0, 0, 0, 0]]);
        let details = parse_details(&json.to_string()).unwrap();
        assert!(matches!(&details[0], RoundDetails::Ryukyoku { reason } if reason == "九種九牌"));
        let (events, data) = parse(original).unwrap();
        assert!(replay(&events).unwrap().with_details(&details).is_err());
        let serialized = serde_json::to_string(&data.round_details()).unwrap();
        let restored: Vec<RoundDetails> = serde_json::from_str(&serialized).unwrap();
        assert!(replay(&events).unwrap().with_details(&restored).is_ok());
        let mut invalid = restored;
        if let RoundDetails::Hora { wins } = &mut invalid[0] {
            wins[0].actor = 4;
        }
        assert!(replay(&events).unwrap().with_details(&invalid).is_err());
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
