//! 同一可见快照下的候选事实；双方共用公开信息，不产生策略总分。

use super::super::position::{
    Snapshot, ToolError, bad_position, kind_name, parse_meld, parse_tile, parse_tiles,
};
use crate::{
    analysis::{self, hand_structure, tile_efficiency::EfficiencyCache},
    mahjong::{hand::Hand, tile::Tile},
    replay::inspector::format_tile,
};
use serde_json::{Value, json};

pub(super) fn compare(
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
    let draw = if args["draw"].is_null() {
        None
    } else {
        let tile = args["draw"]
            .as_str()
            .and_then(parse_tile)
            .filter(|tile| !tile.is_aka())
            .ok_or_else(super::invalid_arguments)?;
        if snapshot.unseen[tile.kind().as_u8() as usize] == 0 {
            return Err(("exhausted_draw", "指定摸牌已无不可见副本。".into()));
        }
        if snapshot.position.remaining_draws == 0
            || snapshot.position.players[snapshot.player].riichi != "not_declared"
        {
            return Err((
                "unsupported_state",
                "该局面不支持自由摸切的假设分支。".into(),
            ));
        }
        Some(tile)
    };
    let defense = super::defense::analyze(snapshot)?;
    let mut cache = EfficiencyCache::default();
    let first_facts = candidate(snapshot, first, draw, &defense, &mut cache)?;
    let second_facts = candidate(snapshot, second, draw, &defense, &mut cache)?;
    let route_differences = first_facts["hand"]["routes"].as_object().ok_or_else(bad_position)?.iter().map(|(name,first)| {
        let second = &second_facts["hand"]["routes"][name];
        let difference = match (first["available_shanten"].as_i64(), second["available_shanten"].as_i64()) {
            (Some(a),Some(b))=>json!(a-b), _=>Value::Null,
        };
        (name.clone(),json!({"first":first,"second":second,"available_shanten_first_minus_second":difference}))
    }).collect::<serde_json::Map<_,_>>();
    let difference = json!({
        "shanten_first_minus_second":first_facts["hand"]["shanten"].as_i64().unwrap()-second_facts["hand"]["shanten"].as_i64().unwrap(),
        "direct_unseen_first_minus_second":first_facts["hand"]["total_unseen"].as_i64().unwrap()-second_facts["hand"]["total_unseen"].as_i64().unwrap(),
        "routes":route_differences,"not_a_strategy_ranking":true});
    let key = format!(
        "discard_facts_{}_{}_{}",
        format_tile(first),
        format_tile(second),
        draw.map(format_tile).unwrap_or_else(|| "current".into())
    );
    Ok((
        key,
        json!({"context":public_context(snapshot)?,"first":first_facts,"second":second_facts,
        "difference":difference,"scope":{"facts_only":true,"same_routes_for_both_candidates":true,
            "no_success_or_deal_in_probability":true,"no_opponent_style_or_hidden_hand_inference":true,
            "unseen_is_not_wall_count":true,"future_ron_restrictions_not_fully_known":true,
            "does_not_decide_match_end":true}}),
    ))
}

fn candidate(
    snapshot: &Snapshot,
    discard: Tile,
    draw: Option<Tile>,
    defense: &Value,
    cache: &mut EfficiencyCache,
) -> Result<Value, ToolError> {
    let mut hand = snapshot.hand.clone();
    hand.discard(discard).map_err(|_| bad_position())?;
    let report = super::hand::report(
        snapshot,
        &hand,
        Some(discard),
        snapshot.position.phase["kind"] == "after_draw",
    )?;
    let mut direct_defense = serde_json::Map::new();
    for player in (0..4).filter(|&p| p != snapshot.player) {
        direct_defense.insert(
            player.to_string(),
            defense["opponents"][player.to_string()]["tiles"][format_tile(discard)].clone(),
        );
    }
    let shape_tenpai = report["shanten"] == 0
        && report["waits"]
            .as_object()
            .is_some_and(|waits| !waits.is_empty());
    let mut facts = json!({"discard":format_tile(discard),"hand":report,"structure":structure(&hand),
        "discard_safety":direct_defense,"safe_inventory":inventory(snapshot,&hand,defense,&[]),
        "possible_calls_by_rules_and_known_counts":possible_calls(snapshot,discard),
        "own_tenpai_at_exhaustive_draw_if_hand_unchanged":shape_tenpai,
        "tenpai_condition_does_not_require_yaku_or_non_furiten":true});
    if let Some(draw) = draw {
        facts["followup"] = followup(snapshot, &hand, discard, draw, defense, cache, false)?;
    } else if facts["hand"]["shanten"] == 1
        && snapshot.position.remaining_draws > 0
        && snapshot.position.players[snapshot.player].riichi == "not_declared"
    {
        let mut transitions = serde_json::Map::new();
        let draws = facts["hand"]["draws"].as_array().ok_or_else(bad_position)?;
        for next in draws
            .iter()
            .filter(|next| next["unseen"].as_u64().is_some_and(|n| n > 0))
        {
            let draw = next["tile"]
                .as_str()
                .and_then(parse_tile)
                .ok_or_else(bad_position)?;
            transitions.insert(
                format_tile(draw),
                followup(snapshot, &hand, discard, draw, defense, cache, true)?,
            );
        }
        facts["one_shanten_to_tenpai"] = Value::Object(transitions);
    }
    Ok(facts)
}

fn followup(
    snapshot: &Snapshot,
    after_discard: &Hand,
    first_discard: Tile,
    draw: Tile,
    defense: &Value,
    cache: &mut EfficiencyCache,
    tenpai_only: bool,
) -> Result<Value, ToolError> {
    let mut drawn = after_discard.clone();
    drawn.draw(draw).map_err(|_| bad_position())?;
    let mut unseen = snapshot.unseen;
    unseen[draw.kind().as_u8() as usize] -= 1;
    let mut choices = drawn.concealed().to_vec();
    choices.dedup();
    let mut next = serde_json::Map::new();
    for discard in choices {
        let mut after = drawn.clone();
        after.discard(discard).map_err(|_| bad_position())?;
        if tenpai_only && cache.shanten(&after) != 0 {
            continue;
        }
        let mut report =
            super::hand::continuation(snapshot, &after, &[first_discard, discard], &unseen)?;
        // 明细保留每个切牌的事实；省略重复的四家支付表，具体支付可由和牌情景工具展开。
        compact_waits(&mut report);
        if !tenpai_only {
            report["routes"] = super::hand::routes(&after, &unseen)?;
        }
        next.insert(
            format_tile(discard),
            json!({"hand":report,
            "safe_inventory":inventory(snapshot,&after,defense,&[first_discard]),
            "structure":if tenpai_only {Value::Null} else {structure(&after)}}),
        );
    }
    let completed = cache.shanten(&drawn) == -1;
    Ok(
        json!({"draw":format_tile(draw),"unseen_before_draw":snapshot.unseen[draw.kind().as_u8() as usize],
        "completed_shape_before_discard":completed,
        "completion":if completed {super::hand::completed_draw(snapshot,&drawn,draw)?} else {Value::Null},"next_discards":next,
        "scope":{"given_same_non_red_draw":true,"other_players_unchanged":true,
            "assumes_first_discard_passed":true,"no_new_riichi_in_wait_scoring":true,
            "tenpai_discards_only":tenpai_only,"no_discard_selected_by_overall_value":true}}),
    )
}

fn compact_waits(report: &mut Value) {
    if let Some(waits) = report["waits"].as_object_mut() {
        for wait in waits.values_mut() {
            if let Some(scenarios) = wait["scenarios"].as_object_mut() {
                for scenario in scenarios.values_mut() {
                    if let Some(methods) = scenario.as_object_mut() {
                        for method in methods.values_mut() {
                            if let Some(values) = method["best_interpretations"].as_array_mut() {
                                for value in values {
                                    if let Some(object) = value.as_object_mut() {
                                        object.remove("conditional_settlements");
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
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

fn public_context(snapshot: &Snapshot) -> Result<Value, ToolError> {
    let indicators = parse_tiles(&snapshot.position.dora_indicators)?;
    let winds = ["E", "S", "W", "N"];
    let mut players = serde_json::Map::new();
    for (index, player) in snapshot.position.players.iter().enumerate() {
        let melds = player
            .melds
            .iter()
            .map(parse_meld)
            .collect::<Result<Vec<_>, _>>()?;
        let bonus = analysis::known_bonus(melds.iter().flat_map(|meld| meld.tiles()), &indicators);
        let seat_wind = winds[(index + 4 - snapshot.position.dealer as usize) % 4];
        let mut yakuhai = Vec::new();
        for meld in &melds {
            if matches!(meld, crate::mahjong::meld::Meld::Chi { .. }) {
                continue;
            }
            let name = kind_name(meld.tiles()[0].kind());
            if ["P", "F", "C"].contains(&name.as_str()) {
                yakuhai.push(json!({"tile":name,"role":"dragon"}));
            }
            if name == seat_wind {
                yakuhai.push(json!({"tile":name,"role":"seat_wind"}));
            }
            if name == snapshot.position.round.wind {
                yakuhai.push(json!({"tile":name,"role":"round_wind"}));
            }
        }
        let draws = snapshot.position.history.as_ref().map(|events| {
            events
                .iter()
                .filter(|event| event.player as usize == index && event.kind == "draw")
                .count()
        });
        players.insert(
            index.to_string(),
            json!({"score":player.score,"seat_wind":seat_wind,
            "relative_seat_from_self":(index+4-snapshot.player)%4,"riichi":player.riichi,
            "discard_events":player.discards.len(),"draw_events_in_round":draws,
            "open_meld_count":melds.iter().filter(|meld|meld.is_open()).count(),
            "yakuhai_in_fixed_melds":yakuhai,"known_dora_in_fixed_melds":bonus.dora,
            "known_red_in_fixed_melds":bonus.aka_dora,"not_total_hand_value":true}),
        );
    }
    let own_bonus = analysis::known_bonus(
        snapshot
            .hand
            .concealed()
            .iter()
            .chain(snapshot.hand.melds().iter().flat_map(|m| m.tiles())),
        &indicators,
    );
    Ok(
        json!({"players":players,"round_wind":snapshot.position.round.wind,"round_number":snapshot.position.round.number,
        "dealer":snapshot.position.dealer,"honba":snapshot.position.honba,"riichi_sticks":snapshot.position.riichi_sticks,
        "remaining_total_draws":snapshot.position.remaining_draws,
        "self_draws_after_this_discard_if_no_calls_kans_or_early_end":snapshot.position.remaining_draws/4,
        "own_known_dora":own_bonus.dora,"own_known_red":own_bonus.aka_dora,
        "history_available":snapshot.position.history.is_some(),"draw_outcomes":super::scores::draws(snapshot),
        "match_end_rules_available":false}),
    )
}

#[cfg(test)]
mod tests;
