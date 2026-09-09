//! 同一可见快照下的候选事实；双方共用公开信息，不产生策略总分。

use super::super::position::{Snapshot, ToolError, bad_position, kind_name, parse_tile};
use crate::{
    analysis::hand_structure,
    mahjong::{hand::Hand, tile::Tile},
    replay::inspector::format_tile,
};
use serde_json::{Value, json};

pub(super) fn compare_safety(
    snapshot: &Snapshot,
    evidence: &Value,
    args: &Value,
) -> Result<(String, Value), ToolError> {
    let first = snapshot.require_discard(
        evidence,
        args["first"]
            .as_str()
            .ok_or_else(super::invalid_arguments)?,
    )?;
    let second = snapshot.require_discard(
        evidence,
        args["second"]
            .as_str()
            .ok_or_else(super::invalid_arguments)?,
    )?;
    if first == second {
        return Err(("same_discard", "比较需要两个不同候选。".into()));
    }
    let defense = super::defense::analyze(snapshot)?;
    let candidate = |discard: Tile| -> Result<Value, ToolError> {
        let mut hand = snapshot.hand.clone();
        hand.discard(discard).map_err(|_| bad_position())?;
        let safety: serde_json::Map<_, _> = (0..4)
            .filter(|&p| p != snapshot.player)
            .map(|p| {
                (
                    p.to_string(),
                    defense["opponents"][p.to_string()]["tiles"][format_tile(discard)].clone(),
                )
            })
            .collect();
        Ok(
            json!({"discard":format_tile(discard),"discard_safety":safety,
            "safe_inventory":inventory(snapshot,&hand,&defense,&[]),
            "possible_calls_by_rules_and_known_counts":possible_calls(snapshot,discard)}),
        )
    };
    Ok((
        format!(
            "discard_safety_{}_{}",
            format_tile(first),
            format_tile(second)
        ),
        json!({"first":candidate(first)?,"second":candidate(second)?,
            "scope":{"no_deal_in_probability":true,"other_players_unchanged":true}}),
    ))
}

/// 只展开模型点选的一次摸切，保留两次弃牌的振听影响。
pub(super) fn followup(
    snapshot: &Snapshot,
    evidence: &Value,
    args: &Value,
) -> Result<(String, Value), ToolError> {
    let first = snapshot.require_discard(
        evidence,
        args["discard"]
            .as_str()
            .ok_or_else(super::invalid_arguments)?,
    )?;
    let draw = args["draw"]
        .as_str()
        .and_then(parse_tile)
        .filter(|t| !t.is_aka())
        .ok_or_else(super::invalid_arguments)?;
    let discard = if args["next_discard"].is_null() {
        None
    } else {
        Some(
            args["next_discard"]
                .as_str()
                .and_then(parse_tile)
                .ok_or_else(super::invalid_arguments)?,
        )
    };
    if snapshot.unseen[draw.kind().as_u8() as usize] == 0 {
        return Err(("exhausted_draw", "指定摸牌已无未见副本。".into()));
    }
    if snapshot.position.remaining_draws == 0
        || snapshot.position.players[snapshot.player].riichi != "not_declared"
    {
        return Err(("unsupported_state", "当前局面不能自由摸切。".into()));
    }
    let mut hand = snapshot.hand.clone();
    hand.discard(first).map_err(|_| bad_position())?;
    hand.draw(draw).map_err(|_| bad_position())?;
    let completed = crate::analysis::shanten::hand_shanten(&hand) == -1;
    if discard.is_none() {
        if !completed {
            return Err((
                "discard_required",
                "摸牌后尚未完成牌形，请指定next_discard。".into(),
            ));
        }
        return Ok((
            format!("completion_{}_{}", format_tile(first), format_tile(draw)),
            json!({"discard":format_tile(first),"draw":format_tile(draw),"next_discard":null,
                "completion":super::hand::completed_draw(snapshot,&hand,draw)?,
                "scope":{"other_players_unchanged":true,"assumes_first_discard_passed":true}}),
        ));
    }
    let discard = discard.ok_or_else(super::invalid_arguments)?;
    hand.discard(discard)
        .map_err(|_| ("invalid_discard", "指定后续切牌不在摸牌后的手牌中。".into()))?;
    let mut unseen = snapshot.unseen;
    unseen[draw.kind().as_u8() as usize] -= 1;
    let report = super::hand::continuation(snapshot, &hand, &[first, discard], &unseen)?;
    let defense = super::defense::analyze(snapshot)?;
    Ok((
        format!(
            "followup_{}_{}_{}",
            format_tile(first),
            format_tile(draw),
            format_tile(discard)
        ),
        json!({"discard":format_tile(first),"draw":format_tile(draw),"next_discard":format_tile(discard),
            "hand":report,"safe_inventory":inventory(snapshot,&hand,&defense,&[first]),"structure":structure(&hand),
            "scope":{"other_players_unchanged":true,"assumes_first_discard_passed":true,"no_new_riichi_in_wait_scoring":true}}),
    ))
}

pub(super) fn structure(hand: &Hand) -> Value {
    let facts = hand_structure::analyze(hand);
    json!({"components":facts.components.into_iter().map(|component|json!({"kind":component.kind,
        "tiles":component.tiles.into_iter().map(kind_name).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "conflicts":facts.conflicts.into_iter().map(|conflict|json!({"first":conflict.first,"second":conflict.second,
            "insufficient_tiles":conflict.insufficient_tiles.into_iter().map(kind_name).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "isolated_kinds":facts.isolated.into_iter().map(kind_name).collect::<Vec<_>>(),
        "components_overlap_and_are_not_a_complete_partition":true,
        "absence_of_pairwise_conflict_does_not_prove_joint_compatibility":true})
}

pub(super) fn inventory(
    snapshot: &Snapshot,
    hand: &Hand,
    defense: &Value,
    assumed_passed: &[Tile],
) -> Value {
    let mut by_player = serde_json::Map::new();
    let mut safe_masks = [[false; 34]; 4];
    for player in (0..4).filter(|&p| p != snapshot.player) {
        if let Some(kinds) = defense["opponents"][player.to_string()]["known_safe_kinds"].as_array()
        {
            for kind in kinds.iter().filter_map(|v| v.as_str().and_then(parse_tile)) {
                safe_masks[player][kind.kind().as_u8() as usize] = true;
            }
        }
        if snapshot.position.players[player].riichi == "accepted" {
            for tile in assumed_passed {
                safe_masks[player][tile.kind().as_u8() as usize] = true;
            }
        }
        let tiles: Vec<_> = hand
            .concealed()
            .iter()
            .filter(|tile| safe_masks[player][tile.kind().as_u8() as usize])
            .copied()
            .map(format_tile)
            .collect();
        by_player.insert(
            player.to_string(),
            json!({"copies":tiles.len(),"tiles":tiles}),
        );
    }
    let common: Vec<_> = hand
        .concealed()
        .iter()
        .filter(|tile| {
            (0..4)
                .filter(|&p| p != snapshot.player)
                .all(|p| safe_masks[p][tile.kind().as_u8() as usize])
        })
        .copied()
        .map(format_tile)
        .collect();
    json!({"by_player":by_player,"common_safe_tiles":common,"common_safe_copies":common.len(),
        "only_against_ron":true,"not_number_of_safe_turns":true,
        "assumed_passed_discards":assumed_passed.iter().copied().map(format_tile).collect::<Vec<_>>()})
}

fn possible_calls(snapshot: &Snapshot, discard: Tile) -> Value {
    let kind = discard.kind().as_u8() as usize;
    let mut opponents = serde_json::Map::new();
    let kans = snapshot
        .position
        .players
        .iter()
        .flat_map(|player| &player.melds)
        .filter(|meld| matches!(meld.kind.as_str(), "ankan" | "kakan" | "daiminkan"))
        .count();
    for player in (0..4).filter(|&p| p != snapshot.player) {
        let public = &snapshot.position.players[player];
        let can_call = public.riichi == "not_declared"
            && public.melds.len() < 4
            && snapshot.position.remaining_draws > 0;
        let mut chi = Vec::new();
        if can_call && player == (snapshot.player + 1) % 4 && kind < 27 {
            for start in kind.saturating_sub(2)..=kind {
                if start / 9 != kind / 9 || start % 9 > 6 {
                    continue;
                }
                let partners: Vec<_> = (start..start + 3).filter(|&tile| tile != kind).collect();
                if partners.iter().all(|&tile| snapshot.unseen[tile] > 0) {
                    chi.push(
                        partners
                            .into_iter()
                            .map(|tile| format_tile(Tile::new(tile as u8).unwrap()))
                            .collect::<Vec<_>>(),
                    );
                }
            }
        }
        opponents.insert(
            player.to_string(),
            json!({"chi_consumed_kind_combinations":chi,
            "pon_not_ruled_out":can_call && snapshot.unseen[kind]>=2,
            "daiminkan_not_ruled_out":can_call && kans<4 && snapshot.unseen[kind]>=3}),
        );
    }
    json!({"opponents":opponents,"scope":{"rules_and_visible_counts_only":true,
        "not_actual_opponent_actions_or_call_probability":true,"ron_has_priority_over_calls":true,
        "does_not_resolve_competing_responses":true}})
}

#[cfg(test)]
mod tests;
