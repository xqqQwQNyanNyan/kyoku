use serde_json::{Value, json};

use crate::analysis::{DiscardEfficiency, DrawCandidates};
use crate::mahjong::{
    meld::Meld,
    player::RiichiState,
    round::{DrawSource, KanKind, RoundPhase, RoundResult, Wind},
    tile::Tile,
};
use crate::mortal::Action;
use crate::replay::inspector::format_tile;
use crate::review::{Review, VisiblePosition};

/// 将可见复盘数据转换为工具使用的 JSON 证据，无需 LLM 配置或网络请求。
/// 协议见 `docs/agent/agent.md`，与 `AgentSession::evidence` 使用同一份投影。
pub fn review_evidence(review: &Review) -> Value {
    let mut evidence =
        position_evidence(review.event_index, review.player.get_id(), &review.position);
    let discards: Vec<_> = review.discards.iter().map(discard_evidence).collect();
    let decision = review.decision.as_ref().map(|decision| {
        let candidates: Vec<_> = decision
            .candidates
            .iter()
            .map(|candidate| {
                json!({
                    "action": action(candidate.action), "q_value": candidate.q_value,
                })
            })
            .collect();
        let kan_candidates: Vec<_> = decision
            .kan_candidates
            .iter()
            .map(|candidate| {
                json!({
                    "tile": tile_kind(candidate.tile.as_u8()), "q_value": candidate.q_value,
                })
            })
            .collect();
        json!({
            "recommended": decision.recommended,
            "candidates": candidates, "kan_candidates": kan_candidates,
            "shanten": decision.shanten, "at_furiten": decision.at_furiten,
        })
    });
    evidence["discards"] = json!(discards);
    evidence["analysis_status"] = json!(if review.discards.is_empty() {
        "unavailable"
    } else {
        "available"
    });
    evidence["mortal"] = json!({
        "status": if decision.is_some() { "available" } else { "no_decision" },
        "model": {"version": review.model.version, "tag": review.model.tag, "sha256": review.model.sha256},
        "decision": decision,
    });
    evidence
}

pub(super) fn discard_evidence(discard: &DiscardEfficiency) -> Value {
    let (kind, candidates) = match &discard.candidates {
        DrawCandidates::Effective(tiles) => ("effective", tiles),
        DrawCandidates::Winning(tiles) => ("winning_shape", tiles),
    };
    let draws: Vec<_> = candidates
        .iter()
        .map(|candidate| {
            json!({
                "tile": tile_kind(candidate.kind.as_u8()), "unseen": candidate.unseen,
            })
        })
        .collect();
    json!({
        "discard": format_tile(discard.discard), "shanten": discard.shanten,
        "draw_kind": kind, "draws": draws, "total_unseen": discard.total_unseen,
    })
}

pub(super) fn position_evidence(
    event_index: usize,
    player: u8,
    position: &VisiblePosition,
) -> Value {
    let players: Vec<_> = position
        .players
        .iter()
        .enumerate()
        .map(|(index, player)| {
            let discards: Vec<_> = player
                .discards
                .iter()
                .map(|discard| {
                    json!({
                        "tile": format_tile(discard.tile()),
                        "tsumogiri": discard.is_tsumogiri(),
                        "riichi": discard.is_riichi(),
                        "called": discard.is_called(),
                    })
                })
                .collect();
            let melds: Vec<_> = player
                .melds
                .iter()
                .map(|meld| {
                    json!({
                        "kind": match meld {
                            Meld::Chi { .. } => "chi",
                            Meld::Pon { .. } => "pon",
                            Meld::Daiminkan { .. } => "daiminkan",
                            Meld::Ankan { .. } => "ankan",
                            Meld::Kakan { .. } => "kakan",
                        },
                        "tiles": tiles(meld.tiles()),
                        "called": meld.called().map(format_tile),
                        "from": meld.from().map(|player| player.get_id()),
                    })
                })
                .collect();
            json!({
                "player": index, "score": player.score,
                "riichi": match player.riichi {
                    RiichiState::NotDeclared => "not_declared",
                    RiichiState::Declared => "declared",
                    RiichiState::Accepted => "accepted",
                },
                "discards": discards, "melds": melds,
            })
        })
        .collect();
    json!({
        "schema_version": 2,
        "event_index": event_index, "player": player,
        "position": {
            "round": {"wind": match position.round.wind() {
                Wind::East => "E", Wind::South => "S", Wind::West => "W", Wind::North => "N",
            }, "number": position.round.number()},
            "dealer": position.round.dealer().get_id(),
            "honba": position.honba, "riichi_sticks": position.riichi_sticks,
            "remaining_draws": position.remaining_draws, "phase": phase(position.phase),
            "dora_indicators": tiles(&position.dora_indicators),
            "concealed": tiles(&position.concealed), "players": players,
        },
        "discards": [],
        "analysis_status": "not_analyzed",
        "mortal": {"status": "not_analyzed", "model": null, "decision": null},
        "limitations": [
            "unseen_includes_opponent_hands",
            "winning_shape_is_not_legal_agari",
            "q_is_not_probability_or_expected_points",
            "recommended_overrides_q_ranking",
            "kan_q_is_separate",
            "no_discard_scoring_or_defense_risk",
            "no_future_events_or_opponent_concealed_tiles",
            "mortal_preference_does_not_explain_its_cause",
        ],
    })
}

fn tiles(values: &[Tile]) -> Vec<String> {
    values.iter().copied().map(format_tile).collect()
}

fn tile_kind(value: u8) -> String {
    format_tile(Tile::new(value).expect("领域牌种必定是合法实体牌编码"))
}

fn action(action: Action) -> Value {
    let kind = match action {
        Action::Discard(tile) => return json!({"kind": "discard", "tile": format_tile(tile)}),
        Action::Riichi => "riichi",
        Action::ChiLow => "chi_low",
        Action::ChiMiddle => "chi_middle",
        Action::ChiHigh => "chi_high",
        Action::Pon => "pon",
        Action::Kan => "kan",
        Action::Win => "win",
        Action::AbortiveDraw => "abortive_draw",
        Action::Pass => "pass",
    };
    json!({"kind": kind})
}

fn phase(phase: RoundPhase) -> Value {
    match phase {
        RoundPhase::Initial => json!({"kind": "initial"}),
        RoundPhase::AfterDraw { player, source } => json!({
            "kind": "after_draw", "player": player.get_id(),
            "source": match source { DrawSource::Wall => "wall", DrawSource::Rinshan => "rinshan" },
        }),
        RoundPhase::AfterDiscard { player } => {
            json!({"kind": "after_discard", "player": player.get_id()})
        }
        RoundPhase::AfterCall { player } => {
            json!({"kind": "after_call", "player": player.get_id()})
        }
        RoundPhase::AfterKanDeclaration { player, kind } => json!({
            "kind": "after_kan_declaration", "player": player.get_id(),
            "kan_kind": match kind { KanKind::Daiminkan => "daiminkan", KanKind::Ankan => "ankan", KanKind::Kakan => "kakan" },
        }),
        RoundPhase::AwaitingEnd(result) => {
            json!({"kind": "awaiting_end", "result": round_result(result)})
        }
        RoundPhase::Ended(result) => json!({"kind": "ended", "result": round_result(result)}),
    }
}

fn round_result(result: RoundResult) -> Value {
    match result {
        RoundResult::Hora { score_deltas } => json!({"kind": "hora", "score_deltas": score_deltas}),
        RoundResult::Ryukyoku => json!({"kind": "ryukyoku"}),
    }
}
