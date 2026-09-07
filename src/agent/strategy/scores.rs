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
