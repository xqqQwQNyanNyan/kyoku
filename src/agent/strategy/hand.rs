use super::super::position::{Snapshot, ToolError, bad_position, kind_name, parse_tiles};
use crate::{
    analysis::{
        self, AgariContext, Payments, RiichiStatus, RonSource, TsumoSource, WinMethod, Yaku,
        shanten::hand_shanten,
        winning_value::{best_win_values, total_payment},
    },
    mahjong::{hand::Hand, round::Wind, tile::Tile},
    replay::inspector::format_tile,
};
use serde_json::{Value, json};

pub(super) const ROUTES: [(&str, Yaku); 21] = [
    ("chiitoitsu", Yaku::Chiitoitsu),
    ("kokushi", Yaku::Kokushi),
    ("toitoi", Yaku::Toitoi),
    ("tanyao", Yaku::Tanyao),
    ("honitsu", Yaku::Honitsu),
    ("chinitsu", Yaku::Chinitsu),
    ("ittsu", Yaku::Ittsu),
    ("sanshoku_doujun", Yaku::SanshokuDoujun),
    ("iipeikou", Yaku::Iipeikou),
    ("honroutou", Yaku::Honroutou),
    ("chanta", Yaku::Chanta),
    ("junchan", Yaku::Junchan),
    ("sanshoku_doukou", Yaku::SanshokuDoukou),
    ("ryanpeikou", Yaku::Ryanpeikou),
    ("shousangen", Yaku::Shousangen),
    ("daisangen", Yaku::Daisangen),
    ("shousuushi", Yaku::Shousuushi),
    ("daisuushi", Yaku::Daisuushi),
    ("tsuuiisou", Yaku::Tsuuiisou),
    ("chinroutou", Yaku::Chinroutou),
    ("ryuuiisou", Yaku::Ryuuiisou),
];

fn after_discard(
    snapshot: &Snapshot,
    evidence: &Value,
    args: &Value,
) -> Result<(Hand, Option<Tile>), ToolError> {
    let mut hand = snapshot.hand.clone();
    let discard = if args["discard"].is_null() {
        None
    } else {
        Some(
            snapshot.require_discard(
                evidence,
                args["discard"]
                    .as_str()
                    .ok_or_else(super::invalid_arguments)?,
            )?,
        )
    };
    if let Some(discard) = discard {
        hand.discard(discard).map_err(|_| bad_position())?;
    }
    if hand.effective_tile_count() != Hand::MIN_TILE_COUNT {
        return Err((
            "unsupported_state",
            "分析需要等效13张手牌；当前14张时请指定已提供的切牌候选。".into(),
        ));
    }
    Ok((hand, discard))
}

pub(super) fn analyze(
    snapshot: &Snapshot,
    evidence: &Value,
    args: &Value,
) -> Result<(String, Value), ToolError> {
    let (hand, discard) = after_discard(snapshot, evidence, args)?;
    let can_declare = discard.is_some() && snapshot.position.phase["kind"] == "after_draw";
    let report = report(snapshot, &hand, discard, can_declare)?;
    Ok((
        format!(
            "hand_{}",
            discard.map(format_tile).unwrap_or_else(|| "current".into())
        ),
        report,
    ))
}

pub(super) fn route(
    snapshot: &Snapshot,
    evidence: &Value,
    args: &Value,
) -> Result<(String, Value), ToolError> {
    let (hand, discard) = after_discard(snapshot, evidence, args)?;
    let name = args["yaku"].as_str().ok_or_else(super::invalid_arguments)?;
    let yaku = ROUTES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, y)| *y)
        .ok_or_else(|| ("unsupported_yaku", "该役种尚未提供距离计算。".into()))?;
    let shanten =
        analysis::yaku_shanten(&hand, yaku).map_err(|e| ("analysis_failed", e.to_string()))?;
    Ok((
        format!(
            "route_{}_{}",
            discard.map(format_tile).unwrap_or_else(|| "current".into()),
            name
        ),
        json!({
            "discard":discard.map(format_tile),"yaku":name,"reachable":shanten.is_some(),"shanten":shanten,
            "counts_unseen_tiles":false,"is_completion_probability":false,
        }),
    ))
}

pub(super) fn routes(hand: &Hand) -> Result<Value, ToolError> {
    ROUTES[..8]
        .iter()
        .map(|(name, yaku)| {
            let shanten = analysis::yaku_shanten(hand, *yaku)
                .map_err(|e| ("analysis_failed", e.to_string()))?;
            Ok((
                (*name).to_owned(),
                json!({"reachable":shanten.is_some(),"shanten":shanten}),
            ))
        })
        .collect::<Result<serde_json::Map<_, _>, _>>()
        .map(Value::Object)
}

pub(super) fn efficiency(hand: &Hand, unseen: &[u8; 34]) -> Result<Value, ToolError> {
    let shanten = hand_shanten(hand);
    let kinds = if shanten == 0 {
        analysis::winning_tile_kinds(hand)
    } else {
        analysis::effective_tile_kinds(hand)
    }
    .map_err(|e| ("analysis_failed", e.to_string()))?;
    let total: u16 = kinds
        .iter()
        .map(|k| u16::from(unseen[k.as_u8() as usize]))
        .sum();
    Ok(
        json!({"shanten":shanten,"draw_kind":if shanten==0 {"winning_shape"} else {"effective"},
        "draws":kinds.into_iter().map(|k| json!({"tile":kind_name(k),"unseen":unseen[k.as_u8() as usize]})).collect::<Vec<_>>(),"total_unseen":total}),
    )
}

pub(super) fn report(
    snapshot: &Snapshot,
    hand: &Hand,
    discard: Option<Tile>,
    can_declare: bool,
) -> Result<Value, ToolError> {
    let mut result = efficiency(hand, &snapshot.unseen)?;
    result["discard"] = json!(discard.map(format_tile));
    result["concealed_after"] = json!(
        hand.concealed()
            .iter()
            .copied()
            .map(format_tile)
            .collect::<Vec<_>>()
    );
    result["routes"] = routes(hand)?;
    let closed = hand.melds().iter().all(|m| !m.is_open());
    result["closed"] = json!(closed);
    let own = &snapshot.position.players[snapshot.player];
    let established = own.riichi != "not_declared";
    result["riichi_state"] = json!(own.riichi);
    let can_riichi = result["shanten"] == 0
        && closed
        && !established
        && can_declare
        && own.score >= 1000
        && snapshot.position.remaining_draws >= 4;
    result["can_declare_riichi_under_current_conditions"] = json!(can_riichi);
    result["riichi_deposit_if_declared"] = json!(if can_riichi { 1000 } else { 0 });
    result["scope"] = json!({"conditional_ordinary_win":true,"unseen_is_not_wall_count":true,
        "winning_draw_is_non_red":true,"unknown_ura_and_new_dora_excluded":true,"ippatsu_last_tile_and_rinshan_excluded":true,
        "other_ron_restrictions":"not_checked","route_distance_ignores_visible_exhaustion":true,
        "payments_exclude_honba_and_sticks":true,"not_expected_value":true,
        "assumes_pending_riichi_is_accepted":own.riichi=="declared"});
    if result["shanten"] != 0 {
        result["waits"] = json!({});
        return Ok(result);
    }
    let kinds =
        analysis::winning_tile_kinds(hand).map_err(|e| ("analysis_failed", e.to_string()))?;
    let mut river = own
        .discards
        .iter()
        .map(|d| {
            super::super::position::parse_tile(&d.tile)
                .map(|t| t.kind())
                .ok_or_else(bad_position)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(tile) = discard {
        river.push(tile.kind());
    }
    let furiten_tiles: Vec<_> = kinds
        .iter()
        .filter(|k| river.contains(k))
        .map(|&k| kind_name(k))
        .collect();
    result["discard_furiten"] =
        json!({"blocked":!furiten_tiles.is_empty(),"intersecting_waits":furiten_tiles});
    let winds = [Wind::East, Wind::South, Wind::West, Wind::North];
    let round_wind = match snapshot.position.round.wind.as_str() {
        "E" => Wind::East,
        "S" => Wind::South,
        "W" => Wind::West,
        "N" => Wind::North,
        _ => return Err(bad_position()),
    };
    let seat_wind = winds[(snapshot.player + 4 - snapshot.position.dealer as usize) % 4];
    let riichi = riichi_status(snapshot);
    result["riichi_history_known"] = json!(snapshot.position.history.is_some());
    let dora = parse_tiles(&snapshot.position.dora_indicators)?;
    let mut waits = serde_json::Map::new();
    for kind in kinds {
        let mut complete = hand.clone();
        complete
            .draw(Tile::new(kind.as_u8()).unwrap_or_else(|| unreachable!("牌种可转换为普通牌")))
            .map_err(|_| bad_position())?;
        let mut scenarios = serde_json::Map::new();
        for (label, status) in if established {
            vec![(
                if own.riichi == "accepted" {
                    "established_riichi"
                } else {
                    "pending_riichi_if_accepted"
                },
                riichi,
            )]
        } else if can_riichi {
            vec![("dama", RiichiStatus::None), ("declare_riichi", riichi)]
        } else {
            vec![("without_riichi", RiichiStatus::None)]
        } {
            let mut methods = serde_json::Map::new();
            for (method, win_method) in [
                ("ron", WinMethod::Ron(RonSource::Discard)),
                ("tsumo", WinMethod::Tsumo(TsumoSource::Wall)),
            ] {
                let context = AgariContext {
                    winning_tile: kind,
                    win_method,
                    round_wind,
                    seat_wind,
                    riichi: status,
                };
                let values = best_win_values(&complete, &context, &dora)
                    .map_err(|e| ("scoring_failed", e.to_string()))?;
                methods.insert(method.into(),json!({"has_yaku":!values.is_empty(),
                    "blocked_by_discard_furiten":method=="ron" && result["discard_furiten"]["blocked"]==true,
                    "best_interpretations":values.into_iter().map(|v| json!({
                        "yaku":v.yaku.iter().map(|y| format!("{y:?}")).collect::<Vec<_>>(),"fu":v.value.fu,"yaku_han":v.value.han,
                        "total_han":v.value.total_han(&v.bonus),"yakuman":v.value.yakuman,"wait_type":v.wait,
                        "bonus":{"dora":v.bonus.dora,"aka_dora":v.bonus.aka_dora,"ura_dora":v.bonus.ura_dora},
                        "payments":payments(&v.payments),"base_receipts":total_payment(&v.payments),
                    })).collect::<Vec<_>>()}));
            }
            scenarios.insert(label.into(), Value::Object(methods));
        }
        waits.insert(
            kind_name(kind),
            json!({"unseen":snapshot.unseen[kind.as_u8() as usize],"scenarios":scenarios}),
        );
    }
    result["waits"] = Value::Object(waits);
    Ok(result)
}

fn riichi_status(snapshot: &Snapshot) -> RiichiStatus {
    let double = snapshot.position.history.as_ref().is_some_and(|events| {
        let declaration = events
            .iter()
            .position(|e| e.player as usize == snapshot.player && e.kind == "riichi_declared")
            .unwrap_or(events.len());
        !events[..declaration].iter().any(|e| {
            e.kind == "call" || (e.player as usize == snapshot.player && e.kind == "discard")
        })
    });
    if double {
        RiichiStatus::DoubleRiichi { ippatsu: false }
    } else {
        RiichiStatus::Riichi { ippatsu: false }
    }
}

pub(super) fn payments(value: &Payments) -> Value {
    match *value {
        Payments::Ron { amount } => json!({"kind":"ron","amount":amount}),
        Payments::DealerTsumo { each } => json!({"kind":"dealer_tsumo","each":each}),
        Payments::NonDealerTsumo {
            dealer,
            each_non_dealer,
        } => json!({"kind":"non_dealer_tsumo","dealer":dealer,"each_non_dealer":each_non_dealer}),
    }
}
