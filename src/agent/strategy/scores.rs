use super::super::position::{Snapshot, ToolError};
use crate::{
    analysis::{
        BonusHan, HandValue, Payments, TsumoSource, WinMethod, calculate_payments,
        score_scenario::{ScoreOutcome, apply_win},
        winning_value::total_payment,
    },
    mahjong::player_index::PlayerIndex,
};
use serde_json::{Value, json};

pub(super) fn analyze(snapshot: &Snapshot, args: &Value) -> Result<(String, Value), ToolError> {
    let target = args["target"]
        .as_u64()
        .filter(|&p| p < 4 && p as usize != snapshot.player)
        .ok_or_else(|| {
            (
                "invalid_target",
                "target 必须是另一名玩家的固定索引0..3。".into(),
            )
        })? as usize;
    let scores: [i32; 4] = std::array::from_fn(|i| snapshot.position.players[i].score);
    let mut ron = serde_json::Map::new();
    let gap = i64::from(scores[target]) - i64::from(scores[snapshot.player]);
    let tie_increment = i64::from(snapshot.player > target);
    let pool = i64::from(snapshot.position.riichi_sticks) * 1000;
    let honba = i64::from(snapshot.position.honba) * 300;
    for payer in (0..4).filter(|&p| p != snapshot.player) {
        let multiplier = if payer == target { 2 } else { 1 };
        let needed = gap + tie_increment - pool - multiplier * honba;
        let units = (needed.max(0) + multiplier * 100 - 1) / (multiplier * 100);
        ron.insert(payer.to_string(),json!({"payer":payer,"minimum_ron_payment_excluding_honba":units*100,
            "includes_honba_in_threshold":false,"already_satisfied_without_base_payment":needed<=0}));
    }
    let mut best: Option<(Payments, ScoreOutcome)> = None;
    // 枚举计分器支持的常规支付档位；这里没有推断自家可以做成哪种符番。
    for fu in [20, 25, 30, 40, 50, 60, 70, 80, 90, 100, 110] {
        for han in 1..=13 {
            if (fu == 20 && han < 2) || (fu == 25 && han < 3) {
                continue;
            }
            let value = HandValue {
                fu: Some(fu),
                han,
                yakuman: 0,
            };
            let payments = calculate_payments(
                &value,
                &BonusHan::default(),
                snapshot.player == snapshot.position.dealer as usize,
                WinMethod::Tsumo(TsumoSource::Wall),
            )
            .map_err(|e| ("scoring_failed", e.to_string()))?;
            let outcome = outcome(snapshot, &payments, None)?;
            if outcome.ranks[snapshot.player] < outcome.ranks[target]
                && best
                    .as_ref()
                    .is_none_or(|(b, _)| total_payment(&payments) < total_payment(b))
            {
                best = Some((payments, outcome));
            }
        }
    }
    Ok((
        format!("score_target_{target}"),
        json!({"target":target,"player":snapshot.player,"scores_before":scores,
        "point_gap_target_minus_self":gap,"tie_favors_self":snapshot.player<target,"already_ahead":gap<0 || (gap==0 && snapshot.player<target),
        "honba":snapshot.position.honba,"riichi_sticks":snapshot.position.riichi_sticks,"ron_by_payer":ron,
        "minimum_tsumo_in_standard_payment_table":best.map(|(p,o)|json!({"payments":super::hand::payments(&p),"base_receipts":total_payment(&p),"outcome":outcome_value(&o)})),
        "scope":{"single_winner":true,"excludes_multiple_yakuman":true,"no_future_riichi_deposits":true,
            "ron_threshold_is_mathematical_not_a_guaranteed_hand_value":true,"payment_table_is_not_hand_reachability":true,
            "does_not_decide_match_end":true,"tie_order":"initial_seat_order"}}),
    ))
}

pub(super) fn outcome(
    snapshot: &Snapshot,
    payments: &Payments,
    payer: Option<usize>,
) -> Result<ScoreOutcome, ToolError> {
    let player = PlayerIndex::new(snapshot.player as u8).unwrap();
    let dealer = PlayerIndex::new(snapshot.position.dealer).unwrap();
    let payer = payer.and_then(|p| PlayerIndex::new(p as u8));
    apply_win(
        std::array::from_fn(|i| snapshot.position.players[i].score),
        player,
        dealer,
        payer,
        payments,
        snapshot.position.honba,
        snapshot.position.riichi_sticks,
    )
    .map_err(|e| ("invalid_score_scenario", format!("{e:?}")))
}

pub(super) fn outcome_value(outcome: &ScoreOutcome) -> Value {
    json!({"scores":outcome.scores,"deltas":outcome.deltas,"ranks":outcome.ranks})
}

/// 自家具体和牌形的条件结算；新增立直支出与回收在同一分支中只记一次。
pub(super) fn draws(snapshot: &Snapshot) -> Value {
    let scores = std::array::from_fn(|i| snapshot.position.players[i].score);
    let combinations: Vec<_> = (0..16u8).map(|mask| {
        let tenpai = std::array::from_fn(|i| mask & (1<<i) != 0);
        let result = crate::analysis::score_scenario::apply_exhaustive_draw(scores,tenpai);
        let continues = tenpai[snapshot.position.dealer as usize];
        json!({"tenpai":tenpai,"outcome":outcome_value(&result),
            "dealer_continues_if_match_continues":continues,
            "next_dealer_if_match_continues":if continues {snapshot.position.dealer} else {(snapshot.position.dealer+1)%4},
            "next_honba_if_match_continues":u16::from(snapshot.position.honba)+1,
            "carried_riichi_sticks":snapshot.position.riichi_sticks})
    }).collect();
    json!({"combinations":combinations,"scope":{"given_exhaustive_draw":true,
        "tenpai_combinations_are_conditions_not_predictions":true,"noten_pool":3000,
        "dealer_repeats_on_tenpai":true,"excludes_abortive_draws_and_nagashi_mangan":true,
        "does_not_decide_match_end":true,"no_additional_riichi_deposits":true}})
}

pub(super) fn win(snapshot: &Snapshot, args: &Value) -> Result<(String, Value), ToolError> {
    let winner = args["winner"]
        .as_u64()
        .and_then(|p| u8::try_from(p).ok())
        .and_then(PlayerIndex::new)
        .ok_or_else(super::invalid_arguments)?;
    let payer = if args["payer"].is_null() {
        None
    } else {
        Some(
            args["payer"]
                .as_u64()
                .and_then(|p| u8::try_from(p).ok())
                .and_then(PlayerIndex::new)
                .ok_or_else(super::invalid_arguments)?,
        )
    };
    let fu = args["fu"]
        .as_u64()
        .filter(|&fu| fu <= 110)
        .and_then(|fu| u32::try_from(fu).ok())
        .ok_or_else(super::invalid_arguments)?;
    let han = args["han"]
        .as_u64()
        .filter(|&han| (1..=13).contains(&han))
        .and_then(|han| u32::try_from(han).ok())
        .ok_or_else(super::invalid_arguments)?;
    let method = if payer.is_some() {
        WinMethod::Ron(crate::analysis::RonSource::Discard)
    } else {
        WinMethod::Tsumo(TsumoSource::Wall)
    };
    let payments = calculate_payments(
        &HandValue {
            fu: Some(fu),
            han,
            yakuman: 0,
        },
        &BonusHan::default(),
        winner.get_id() == snapshot.position.dealer,
        method,
    )
    .map_err(|e| ("invalid_score_scenario", e.to_string()))?;
    let result = apply_win(
        std::array::from_fn(|i| snapshot.position.players[i].score),
        winner,
        PlayerIndex::new(snapshot.position.dealer).unwrap(),
        payer,
        &payments,
        snapshot.position.honba,
        snapshot.position.riichi_sticks,
    )
    .map_err(|e| ("invalid_score_scenario", format!("{e:?}")))?;
    let payer_name = payer.map_or_else(|| "tsumo".into(), |p| format!("ron_from_{}", p.get_id()));
    Ok((
        format!("win_{}_{}_{}_{}", winner.get_id(), payer_name, fu, han),
        json!({
        "winner":winner.get_id(),"payer":payer.map(|p|p.get_id()),"fu":fu,"given_total_han":han,
        "payments":super::hand::payments(&payments),"outcome":outcome_value(&result),
        "scope":{"fu_han_are_given_not_inferred":true,"assumes_at_least_one_yaku":true,
            "not_an_opponent_value_estimate":true,"includes_honba_and_existing_sticks":true,
            "single_winner":true,"does_not_decide_match_end":true,"no_additional_riichi_deposits":true}}),
    ))
}
