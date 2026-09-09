use super::super::{
    evidence::discard_evidence,
    position::{Snapshot, ToolError, bad_position, kind_name, parse_tile},
};
use crate::{
    analysis::{shanten::hand_shanten, tile_efficiency::analyze_discard},
    mahjong::{hand::Hand, meld::Meld, player_index::PlayerIndex, tile::Tile},
    replay::inspector::format_tile,
};
use serde_json::{Value, json};

pub(super) fn analyze(snapshot: &Snapshot, evidence: &Value) -> Result<Value, ToolError> {
    analyze_selected(snapshot, evidence, None)
}

struct Selection<'a> {
    action: &'a str,
    variant: Option<&'a str>,
    discard: Option<&'a str>,
    draw: Option<Tile>,
}

pub(super) fn detail(
    snapshot: &Snapshot,
    evidence: &Value,
    args: &Value,
) -> Result<(String, Value), ToolError> {
    let optional = |key: &str| -> Result<Option<&str>, ToolError> {
        if args[key].is_null() {
            Ok(None)
        } else {
            args[key]
                .as_str()
                .map(Some)
                .ok_or_else(super::invalid_arguments)
        }
    };
    let action = args["action"]
        .as_str()
        .ok_or_else(super::invalid_arguments)?;
    let variant = optional("variant")?;
    let discard = optional("discard")?;
    let draw = optional("draw")?
        .map(|name| {
            parse_tile(name)
                .filter(|t| !t.is_aka())
                .ok_or_else(super::invalid_arguments)
        })
        .transpose()?;
    let valid = match action {
        "pass" => variant.is_none() && discard.is_none() && draw.is_none(),
        "riichi" => variant.is_none() && discard.is_some() && draw.is_none(),
        "chi_low" | "chi_middle" | "chi_high" | "pon" => {
            variant.is_some() && discard.is_some() && draw.is_none()
        }
        "kan" => variant.is_some() && discard.is_none(),
        _ => false,
    };
    if !valid {
        return Err(("invalid_arguments", "过牌不带分支；立直指定discard；吃碰指定variant和discard；杠指定variant，可用draw查看一张岭上牌。".into()));
    }
    if draw.is_some_and(|t| snapshot.unseen[t.kind().as_u8() as usize] == 0) {
        return Err(("exhausted_draw", "指定摸牌已无未见副本。".into()));
    }
    let selected = Selection {
        action,
        variant,
        discard,
        draw,
    };
    let all = analyze_selected(snapshot, evidence, Some(&selected))?;
    let result = match action {
        "pass" => &all["pass"],
        "riichi" => &all["riichi"]["by_discard"][discard.unwrap()],
        "kan" => &all["kan"]["variants"][variant.unwrap()],
        _ => &all[action]["variants"][variant.unwrap()]["next_discards"][discard.unwrap()],
    };
    if result.is_null() {
        return Err((
            "candidate_not_provided",
            "请从analyze_actions返回的动作、variant和切牌中选择；禁止食替的切牌不能展开。".into(),
        ));
    }
    Ok((
        format!(
            "action_{}_{}_{}_{}",
            action,
            variant.unwrap_or("none"),
            discard.unwrap_or("none"),
            draw.map(format_tile).unwrap_or_else(|| "none".into())
        ),
        json!({"action":action,"variant":variant,"discard":discard,"draw":draw.map(format_tile),"result":result}),
    ))
}

fn discard_report(
    snapshot: &Snapshot,
    hand: &Hand,
    discard: Tile,
    can_riichi: bool,
    detail: bool,
) -> Result<Value, ToolError> {
    if detail {
        return super::hand::report(snapshot, hand, Some(discard), can_riichi);
    }
    let efficiency = super::hand::efficiency(hand, &snapshot.unseen)?;
    let closed = hand.melds().iter().all(|m| !m.is_open());
    Ok(
        json!({"shanten":efficiency["shanten"],"total_unseen":efficiency["total_unseen"],"closed":closed,
        "can_declare_riichi_under_current_conditions":can_riichi && closed && efficiency["shanten"]==0
            && snapshot.position.players[snapshot.player].riichi=="not_declared"
            && snapshot.position.players[snapshot.player].score >= 1000 && snapshot.position.remaining_draws >= 4}),
    )
}

fn analyze_selected(
    snapshot: &Snapshot,
    evidence: &Value,
    selection: Option<&Selection<'_>>,
) -> Result<Value, ToolError> {
    if evidence["mortal"]["status"] != "available" {
        return Err((
            "analysis_unavailable",
            "动作比较需要当前 Mortal 提供的候选动作。".into(),
        ));
    }
    let candidates = evidence["mortal"]["decision"]["candidates"]
        .as_array()
        .ok_or_else(bad_position)?;
    let defense = super::defense::analyze(snapshot)?;
    let offered = |kind: &str| candidates.iter().any(|c| c["action"]["kind"] == kind);
    let wanted = |kind: &str| offered(kind) && selection.is_none_or(|s| s.action == kind);
    let mut result = serde_json::Map::new();
    let phase = &snapshot.position.phase;
    if wanted("pass") && snapshot.hand.effective_tile_count() == 13 {
        let mut report = pass_report(snapshot, evidence, offered("win"))?;
        report["structure"] = super::facts::structure(&snapshot.hand);
        report["safe_inventory"] = super::facts::inventory(snapshot, &snapshot.hand, &defense, &[]);
        result.insert("pass".into(), report);
    }
    if wanted("riichi") && phase["kind"] == "after_draw" && phase["player"] == snapshot.player {
        let mut discards = serde_json::Map::new();
        for candidate in evidence["discards"].as_array().into_iter().flatten() {
            let name = candidate["discard"].as_str().ok_or_else(bad_position)?;
            if selection.is_some_and(|s| s.discard != Some(name)) {
                continue;
            }
            let tile = snapshot.require_discard(evidence, name)?;
            let mut hand = snapshot.hand.clone();
            hand.discard(tile).map_err(|_| bad_position())?;
            if hand_shanten(&hand) != 0 {
                continue;
            }
            let mut report = discard_report(snapshot, &hand, tile, true, selection.is_some())?;
            if selection.is_some() {
                report["safe_inventory"] = super::facts::inventory(snapshot, &hand, &defense, &[]);
            }
            if report["can_declare_riichi_under_current_conditions"] == true {
                discards.insert(name.into(), report);
            }
        }
        result.insert(
            "riichi".into(),
            json!({"by_discard":discards,"future_discards_locked":true,"deposit":1000,
            "scope":"比较不含一发、未知里宝牌、立直对他家的行为影响或放铳风险"}),
        );
    }
    if phase["kind"] == "after_discard"
        && phase["player"] != snapshot.player
        && snapshot.hand.effective_tile_count() == 13
    {
        let from = phase["player"]
            .as_u64()
            .filter(|&p| p < 4)
            .ok_or_else(bad_position)? as usize;
        let called = snapshot.position.players[from]
            .discards
            .last()
            .and_then(|d| parse_tile(&d.tile))
            .ok_or_else(bad_position)?;
        let from_player = PlayerIndex::new(from as u8).unwrap();
        for kind in ["chi_low", "chi_middle", "chi_high", "pon"] {
            if !wanted(kind) {
                continue;
            }
            if snapshot.position.remaining_draws == 0
                || snapshot.position.players[snapshot.player].riichi != "not_declared"
            {
                return Err((
                    "unsupported_state",
                    "末张或立直后的局面不能作吃碰分析。".into(),
                ));
            }
            let chi = kind != "pon";
            if chi && from != (snapshot.player + 3) % 4 {
                return Err(("unsupported_state", "吃牌来源必须是上家。".into()));
            }
            let mut variants = serde_json::Map::new();
            for i in 0..snapshot.hand.concealed().len() {
                for j in i + 1..snapshot.hand.concealed().len() {
                    let consumed = [snapshot.hand.concealed()[i], snapshot.hand.concealed()[j]];
                    let mut hand = snapshot.hand.clone();
                    if chi {
                        let mut kinds = [consumed[0].kind(), consumed[1].kind(), called.kind()];
                        kinds.sort_unstable();
                        let offset = match kind {
                            "chi_low" => 0,
                            "chi_middle" => 1,
                            _ => 2,
                        };
                        if kinds[offset] != called.kind()
                            || hand.chi(called, from_player, consumed).is_err()
                        {
                            continue;
                        }
                    } else if hand.pon(called, from_player, consumed).is_err() {
                        continue;
                    }
                    let key = consumed.map(format_tile).join("_");
                    if selection.is_some_and(|s| s.variant != Some(key.as_str())) {
                        continue;
                    }
                    if variants.contains_key(&key) {
                        continue;
                    }
                    let mut discards = serde_json::Map::new();
                    let mut forbidden = Vec::new();
                    let mut choices = hand.concealed().to_vec();
                    choices.dedup();
                    for tile in choices {
                        if kuikae(tile, called, consumed, chi) {
                            forbidden.push(format_tile(tile));
                            continue;
                        }
                        if selection.is_some_and(|s| s.discard != Some(format_tile(tile).as_str()))
                        {
                            continue;
                        }
                        let mut after = hand.clone();
                        after.discard(tile).map_err(|_| bad_position())?;
                        let mut report =
                            discard_report(snapshot, &after, tile, false, selection.is_some())?;
                        if selection.is_some() {
                            report["safe_inventory"] =
                                super::facts::inventory(snapshot, &after, &defense, &[]);
                            report["structure"] = super::facts::structure(&after);
                        }
                        discards.insert(format_tile(tile), report);
                    }
                    // 没有合法切牌的消耗组合不能作为可执行变体返回。
                    if discards.is_empty() {
                        continue;
                    }
                    variants.insert(key,json!({"consumed":consumed.map(format_tile),"called":format_tile(called),
                        "from":from,"next_discards":discards,"forbidden_kuikae_discards":forbidden,"closed_after":false}));
                }
            }
            result.insert(kind.into(), json!({"variants":variants}));
        }
        if wanted("kan") {
            let matching: Vec<_> = snapshot
                .hand
                .concealed()
                .iter()
                .filter(|t| t.kind() == called.kind())
                .copied()
                .collect();
            if let Ok(consumed) = <[Tile; 3]>::try_from(matching)
                && selection.is_none_or(|s| s.variant == Some("daiminkan"))
            {
                let mut hand = snapshot.hand.clone();
                hand.daiminkan(called, from_player, consumed)
                    .map_err(|_| bad_position())?;
                result.insert("kan".into(),json!({"variants":{"daiminkan":kan_report(snapshot,&hand,"daiminkan",&consumed,selection.and_then(|s|s.draw))?}}));
            }
        }
    } else if wanted("kan") && phase["kind"] == "after_draw" && phase["player"] == snapshot.player {
        let decision = &evidence["mortal"]["decision"];
        let mut allowed = Vec::new();
        for candidate in decision["kan_candidates"].as_array().into_iter().flatten() {
            let kind = candidate["tile"]
                .as_str()
                .and_then(parse_tile)
                .ok_or_else(bad_position)?
                .kind();
            if !allowed.contains(&kind) {
                allowed.push(kind);
            }
        }
        if allowed.is_empty() {
            let recommendation = &decision["recommended"];
            let tile = match recommendation["type"].as_str() {
                Some("ankan") => recommendation["consumed"][0].as_str().and_then(parse_tile),
                Some("kakan") => recommendation["pai"].as_str().and_then(parse_tile),
                _ => None,
            };
            if let Some(tile) = tile {
                allowed.push(tile.kind());
            }
        }
        let mut variants = serde_json::Map::new();
        for kind in allowed {
            let matching: Vec<_> = snapshot
                .hand
                .concealed()
                .iter()
                .filter(|t| t.kind() == kind)
                .copied()
                .collect();
            if let Ok(consumed) = <[Tile; 4]>::try_from(matching.clone())
                && selection.is_none_or(|s| {
                    s.variant == Some(format!("ankan_{}", kind_name(kind)).as_str())
                })
            {
                let mut hand = snapshot.hand.clone();
                hand.ankan(consumed).map_err(|_| bad_position())?;
                variants.insert(
                    format!("ankan_{}", kind_name(kind)),
                    kan_report(
                        snapshot,
                        &hand,
                        "ankan",
                        &consumed,
                        selection.and_then(|s| s.draw),
                    )?,
                );
            }
            for meld in snapshot.hand.melds() {
                if let Meld::Pon { tiles, .. } = meld
                    && tiles[0].kind() == kind
                    && let Some(&added) = matching.first()
                    && selection.is_none_or(|s| {
                        s.variant == Some(format!("kakan_{}", kind_name(kind)).as_str())
                    })
                {
                    let mut hand = snapshot.hand.clone();
                    hand.kakan(added, *tiles).map_err(|_| bad_position())?;
                    variants.insert(
                        format!("kakan_{}", kind_name(kind)),
                        kan_report(
                            snapshot,
                            &hand,
                            "kakan",
                            &[added],
                            selection.and_then(|s| s.draw),
                        )?,
                    );
                }
            }
        }
        result.insert(
            "kan".into(),
            json!({"details_available":!variants.is_empty(),"variants":variants,
            "candidate_source":"Mortal的杠牌种候选；缺少候选时只展开已明确推荐的杠"}),
        );
    }
    if result.is_empty() {
        return Err((
            "unsupported_state",
            "当前没有可展开的吃碰杠、立直或跳过候选；切牌请使用切牌比较工具。".into(),
        ));
    }
    result.insert("scope".into(),
        json!({"current_offered_actions_only":true,"other_players_unchanged":true,
        "pass_does_not_insert_a_free_draw":true,"kuikae_forbidden":true,"no_combined_expected_value":true,
        "does_not_resolve_competing_calls_or_ron":true}),
    );
    result.insert(
        "safe_inventory_before_action".into(),
        super::facts::inventory(snapshot, &snapshot.hand, &defense, &[]),
    );
    Ok(Value::Object(result))
}

fn pass_report(snapshot: &Snapshot, evidence: &Value, can_ron: bool) -> Result<Value, ToolError> {
    let phase = &snapshot.position.phase;
    let other = phase["player"]
        .as_u64()
        .filter(|&p| p < 4 && p as usize != snapshot.player)
        .ok_or_else(|| {
            (
                "unsupported_state",
                "跳过分析需要他家的弃牌或杠响应窗口。".into(),
            )
        })? as usize;
    let mut report = super::hand::report(snapshot, &snapshot.hand, None, false)?;
    let passes_wait = match phase["kind"].as_str() {
        Some("after_discard") => {
            let discard = snapshot.position.players[other]
                .discards
                .last()
                .ok_or_else(bad_position)?;
            let tile = parse_tile(&discard.tile).ok_or_else(bad_position)?;
            if discard.called {
                return Err(bad_position());
            }
            // 无役也可能触发振听，不能只检查引擎是否提供荣和动作。
            Some(report["waits"].get(kind_name(tile.kind())).is_some())
        }
        Some("after_kan_declaration") => {
            // 快照不记录本次杠的牌种；有荣和候选可确认见逃，否则不猜测待牌。
            can_ron.then_some(true)
        }
        _ => {
            return Err((
                "unsupported_state",
                "跳过分析需要他家的弃牌或杠响应窗口。".into(),
            ));
        }
    };
    let before = if report["discard_furiten"]["blocked"] == true {
        Some(true)
    } else {
        evidence["mortal"]["decision"]["at_furiten"].as_bool()
    };
    if can_ron && (passes_wait == Some(false) || before == Some(true)) {
        return Err((
            "invalid_position",
            "荣和候选与当前待牌或振听状态矛盾。".into(),
        ));
    }
    let riichi = snapshot.position.players[snapshot.player].riichi != "not_declared";
    // 缺少原始振听诊断时，只能确认本次见逃带来的限制，不能把未知旧状态写成 false。
    let relevant_state = match (passes_wait, before) {
        (Some(true), _) => Some(true),
        (Some(false), Some(false)) => Some(false),
        _ => None,
    };
    let temporary = if riichi { Some(false) } else { relevant_state };
    let persistent = if riichi { relevant_state } else { Some(false) };
    let blocked = match (before, passes_wait) {
        (Some(true), _) | (_, Some(true)) => Some(true),
        (Some(false), Some(false)) => Some(false),
        _ => None,
    };
    report["passes_current_winning_tile"] = json!(can_ron);
    report["passed_tile_completes_shape"] = json!(passes_wait);
    report["furiten_before_pass"] = json!(before);
    report["temporary_furiten_after_pass"] = json!(temporary);
    report["riichi_furiten_after_pass"] = json!(persistent);
    report["ron_blocked_by_furiten_after_pass"] = json!(blocked);
    report["scope"]["furiten_state_is_immediately_after_pass"] = json!(true);
    report["scope"]["null_furiten_state_means_unknown"] = json!(true);
    for wait in report["waits"]
        .as_object_mut()
        .into_iter()
        .flat_map(|waits| waits.values_mut())
    {
        for scenario in wait["scenarios"]
            .as_object_mut()
            .into_iter()
            .flat_map(|scenarios| scenarios.values_mut())
        {
            scenario["ron"]["blocked_by_furiten_after_pass"] = json!(blocked);
            scenario["tsumo"]["blocked_by_furiten_after_pass"] = json!(false);
        }
    }
    Ok(report)
}

fn kuikae(discard: Tile, called: Tile, consumed: [Tile; 2], chi: bool) -> bool {
    if discard.kind() == called.kind() {
        return true;
    }
    if !chi {
        return false;
    }
    let mut kinds = [
        discard.kind().as_u8(),
        consumed[0].kind().as_u8(),
        consumed[1].kind().as_u8(),
    ];
    kinds.sort_unstable();
    kinds[0] < 27
        && kinds[0] / 9 == kinds[2] / 9
        && kinds[0] + 1 == kinds[1]
        && kinds[1] + 1 == kinds[2]
}

fn kan_report(
    snapshot: &Snapshot,
    hand: &Hand,
    kind: &str,
    consumed: &[Tile],
    selected_draw: Option<Tile>,
) -> Result<Value, ToolError> {
    if snapshot.position.remaining_draws == 0 {
        return Err(("unsupported_state", "牌山耗尽后不能模拟岭上摸牌。".into()));
    }
    let mut draws = serde_json::Map::new();
    let locked = snapshot.position.players[snapshot.player].riichi == "accepted";
    if let Some(tile) = selected_draw {
        let draw = tile.kind().as_u8();
        let mut complete = hand.clone();
        complete.draw(tile).map_err(|_| bad_position())?;
        let completed = hand_shanten(&complete) == -1;
        let mut unseen = snapshot.unseen;
        unseen[draw as usize] -= 1;
        let mut discards = if locked {
            vec![tile]
        } else {
            complete.concealed().to_vec()
        };
        discards.dedup();
        let efficiencies = discards
            .into_iter()
            .map(|discard| analyze_discard(&complete, discard, |k| unseen[k.as_u8() as usize]))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ("analysis_failed", e.to_string()))?;
        let min = efficiencies.iter().map(|d| d.shanten).min();
        let max = efficiencies
            .iter()
            .filter(|d| Some(d.shanten) == min)
            .map(|d| d.total_unseen)
            .max();
        draws.insert(format_tile(tile),json!({"unseen_before_draw":snapshot.unseen[draw as usize],"completed_shape":completed,
            "best_shanten_after_discard":min,"best_unseen_at_best_shanten":max,
            "best_discards":efficiencies.iter().filter(|d|Some(d.shanten)==min && Some(d.total_unseen)==max).map(|d|(format_tile(d.discard),discard_evidence(d))).collect::<serde_json::Map<_,_>>()}));
    }
    Ok(
        json!({"kind":kind,"consumed":consumed.iter().copied().map(format_tile).collect::<Vec<_>>(),
        "concealed_after":hand.concealed().iter().copied().map(format_tile).collect::<Vec<_>>(),
        "closed_after":hand.melds().iter().all(|m|!m.is_open()),"shanten_before_replacement":hand_shanten(hand),
        "replacement_draws":draws,"replacement_draws_expanded":selected_draw.is_some(),"scope":{"new_dora_and_ura_unknown":true,"shape_only":true,
            "no_robbing_kan_risk_estimate":true,"unseen_is_not_rinshan_probability":true,"riichi_draw_discard_lock":locked}}),
    )
}
