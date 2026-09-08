//! 比较工具只消费固定快照，假设摸牌由参数提供，不能访问实际后续牌谱。

use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    evidence::discard_evidence,
    position::{Snapshot, bad_position, kind_name, parse_tile},
};
use crate::{
    analysis::{
        DrawCandidates,
        discard_comparison::{ComparisonContext, ComparisonError, DiscardBranch, DrawBranch},
    },
    replay::inspector::format_tile,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    first: String,
    second: String,
    draw: Option<String>,
}

pub(super) fn definition() -> Value {
    json!({
        "type": "function", "name": "compare_discards", "strict": true,
        "description": "比较 get_review 已提供的两个不同切牌。返回切后暗牌、局部连接、向听和直接进张差异。draw=null 只比较当前切牌；指定普通牌记法则假设双方随后摸到同种牌，枚举下一次切牌的牌形效率，用于核验具体改良假设。连接可能重叠，不是唯一拆分。假设其他玩家无动作、公开牌不变；不模拟鸣牌、立直、打点或防守，不读取实际未来，不证明 Mortal 的内在原因。",
        "parameters": {
            "type": "object", "additionalProperties": false,
            "properties": {
                "first": {"type": "string", "description": "第一个切牌，须在当前 discards 中；如1m、5mr、P。"},
                "second": {"type": "string", "description": "另一个当前候选切牌。"},
                "draw": {"type": ["string", "null"], "description": "假设摸入的普通非赤牌；不研究后续时传 null。"}
            },
            "required": ["first", "second", "draw"]
        }
    })
}

pub(super) fn execute(evidence: &Value, arguments: &str) -> Value {
    match run(evidence, arguments) {
        Ok(result) => result,
        Err((code, message)) => json!({"ok": false, "error": {"code": code, "message": message}}),
    }
}

pub(super) fn all_definition() -> Value {
    let mut definition = definition();
    definition["name"] = json!("compare_improvements");
    definition["description"] = json!(
        "完整枚举两个切牌之后的全部可用摸牌种类，返回每个分支的摘要：draw、unseen、completed_shape、best_shanten、best_unseen、best_discard_names，以及双方占优的覆盖枚数。不是摸牌概率、总体收益或最优策略；具体进张和全部后续切牌用 compare_discards 的 draw 参数展开。"
    );
    definition["parameters"]["properties"]
        .as_object_mut()
        .unwrap()
        .remove("draw");
    definition["parameters"]["required"] = json!(["first", "second"]);
    definition
}

pub(super) fn execute_all(evidence: &Value, arguments: &str) -> Value {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Pair {
        first: String,
        second: String,
    }
    let run_all = || -> Result<Value, ToolError> {
        let args: Pair = serde_json::from_str(arguments).map_err(|_| {
            (
                "invalid_arguments",
                "需要 first 和 second 两个切牌字段。".into(),
            )
        })?;
        let mut result = run(
            evidence,
            &json!({"first": args.first, "second": args.second, "draw": null}).to_string(),
        )?;
        let snapshot = Snapshot::read(evidence)?;
        if snapshot.position.remaining_draws == 0
            || snapshot.position.players[snapshot.player].riichi != "not_declared"
        {
            return Err((
                "unsupported_state",
                "完整改良比较不支持立直宣言后或牌山已耗尽的局面。".into(),
            ));
        }
        let first = parse_tile(&args.first).ok_or_else(bad_position)?;
        let second = parse_tile(&args.second).ok_or_else(bad_position)?;
        let context = ComparisonContext::new(snapshot.hand, &snapshot.additional_visible)
            .map_err(analysis_error)?;
        let mut first_draws = serde_json::Map::new();
        let mut second_draws = serde_json::Map::new();
        let mut outcomes = serde_json::Map::new();
        let mut weights = [0u16; 3];
        for (unseen, comparison) in context.compare_all(first, second).map_err(analysis_error)? {
            let first = compact_followup(
                comparison
                    .first
                    .followup
                    .as_ref()
                    .ok_or_else(bad_position)?,
                unseen,
            );
            let second = compact_followup(
                comparison
                    .second
                    .followup
                    .as_ref()
                    .ok_or_else(bad_position)?,
                unseen,
            );
            let rank = |value: &Value| {
                if value["completed_shape"] == true {
                    (-1, 0)
                } else {
                    (
                        value["best_shanten"].as_i64().unwrap(),
                        -value["best_unseen"].as_i64().unwrap(),
                    )
                }
            };
            let winner = match rank(&first).cmp(&rank(&second)) {
                std::cmp::Ordering::Less => 0,
                std::cmp::Ordering::Greater => 1,
                std::cmp::Ordering::Equal => 2,
            };
            let draw = first["draw"].as_str().unwrap().to_owned();
            weights[winner] += u16::from(unseen);
            outcomes.insert(draw.clone(), json!({"unseen": unseen, "favored_by_direct_efficiency": (["first", "second", "equal"][winner])}));
            first_draws.insert(draw.clone(), first);
            second_draws.insert(draw, second);
        }
        let key = format!("{}_{}_all", args.first, args.second);
        result["key"] = json!(key);
        result["reference"] = json!(format!("/comparisons/{key}"));
        result["comparison"]["first"]["improvements"] = json!(first_draws);
        result["comparison"]["second"]["improvements"] = json!(second_draws);
        result["comparison"]["coverage"] = json!({
            "by_draw": outcomes, "first_favored_unseen": weights[0], "second_favored_unseen": weights[1],
            "equal_metrics_unseen": weights[2], "total_unseen": weights.iter().sum::<u16>(),
            "is_probability": false, "equal_metrics_is_not_equal_strategy_value": true,
        });
        result["comparison"]["scope"]["all_available_draw_kinds"] = json!(true);
        // 当前切牌的直接进张差，不能被误读为全部未来摸牌分支的差异。
        let comparison = result["comparison"]
            .as_object_mut()
            .ok_or_else(bad_position)?;
        let initial_difference = comparison.remove("difference").ok_or_else(bad_position)?;
        comparison.insert("initial_discard_difference".into(), initial_difference);
        Ok(result)
    };
    match run_all() {
        Ok(result) => result,
        Err((code, message)) => json!({"ok":false,"error":{"code":code,"message":message}}),
    }
}

fn compact_followup(followup: &DrawBranch, unseen: u8) -> Value {
    let shanten = followup.next_discards.iter().map(|d| d.shanten).min();
    let best_unseen = followup
        .next_discards
        .iter()
        .filter(|d| Some(d.shanten) == shanten)
        .map(|d| d.total_unseen)
        .max();
    json!({
        "draw":kind_name(followup.draw), "unseen":unseen, "completed_shape":followup.completed_shape,
        "best_shanten":shanten, "best_unseen":best_unseen,
        "best_discard_names":followup.next_discards.iter()
            .filter(|d| Some(d.shanten) == shanten && Some(d.total_unseen) == best_unseen)
            .map(|d| format_tile(d.discard)).collect::<Vec<_>>(),
    })
}

type ToolError = (&'static str, String);

fn run(evidence: &Value, arguments: &str) -> Result<Value, ToolError> {
    let invalid = || {
        (
            "invalid_arguments",
            "需要 first、second 和 draw 三个字段；牌使用工具中的原始记法。".into(),
        )
    };
    let value: Value = serde_json::from_str(arguments).map_err(|_| invalid())?;
    if value.get("draw").is_none() {
        return Err(invalid());
    }
    let args: Arguments = serde_json::from_value(value).map_err(|_| invalid())?;
    let first = parse_tile(&args.first).ok_or_else(invalid)?;
    let second = parse_tile(&args.second).ok_or_else(invalid)?;
    let draw = args
        .draw
        .as_deref()
        .map(|name| {
            parse_tile(name)
                .filter(|tile| !tile.is_aka())
                .map(|t| t.kind())
                .ok_or_else(invalid)
        })
        .transpose()?;
    if evidence["analysis_status"] != "available" {
        return Err((
            "analysis_unavailable",
            "当前没有切牌分析，不能比较候选。".into(),
        ));
    }
    let candidates = evidence["discards"].as_array().ok_or_else(bad_position)?;
    if [&args.first, &args.second].iter().any(|tile| {
        !candidates
            .iter()
            .any(|c| c["discard"].as_str() == Some(tile.as_str()))
    }) {
        return Err((
            "candidate_not_provided",
            "只能比较当前 discards 已提供的切牌；缺少候选不代表该动作非法。".into(),
        ));
    }
    let snapshot = Snapshot::read(evidence)?;
    let player = snapshot.player;
    let position = &snapshot.position;
    let phase = &evidence["position"]["phase"];
    if !matches!(phase["kind"].as_str(), Some("after_draw" | "after_call"))
        || phase["player"] != player
    {
        return Err(("unsupported_state", "当前不是所选玩家的切牌时刻。".into()));
    }
    if draw.is_some()
        && (position.remaining_draws == 0 || position.players[player].riichi != "not_declared")
    {
        return Err((
            "unsupported_state",
            "后续摸切模拟暂不支持立直宣言后或牌山已耗尽的局面；仍可用 draw=null 比较当前切牌。"
                .into(),
        ));
    }
    let comparison = ComparisonContext::new(snapshot.hand, &snapshot.additional_visible)
        .and_then(|context| context.compare(first, second, draw))
        .map_err(analysis_error)?;
    let first_draws = draws(&comparison.first.efficiency.candidates);
    let second_draws = draws(&comparison.second.efficiency.candidates);
    let only = |left: &[crate::analysis::TileAvailability],
                right: &[crate::analysis::TileAvailability]| {
        left.iter()
            .filter(|l| !right.iter().any(|r| r.kind == l.kind))
            .map(|t| json!({"tile": kind_name(t.kind), "unseen": t.unseen}))
            .collect::<Vec<_>>()
    };
    let key = format!(
        "{}_{}_{}",
        args.first,
        args.second,
        args.draw.as_deref().unwrap_or("none")
    );
    Ok(json!({
        "ok": true, "key": key, "reference": format!("/comparisons/{key}"),
        "comparison": {
            "first": branch(&comparison.first), "second": branch(&comparison.second),
            "difference": {
                "shanten_first_minus_second": comparison.first.efficiency.shanten - comparison.second.efficiency.shanten,
                "unseen_first_minus_second": i16::from(comparison.first.efficiency.total_unseen) - i16::from(comparison.second.efficiency.total_unseen),
                "only_first_draws": only(first_draws, second_draws), "only_second_draws": only(second_draws, first_draws)
            },
            "connections_before": comparison.connections_before.iter().map(|c| json!({
                "tile": kind_name(c.kind), "copies": c.copies,
                "sequence_neighbors": c.sequence_neighbors.iter().map(|&k| kind_name(k)).collect::<Vec<_>>()
            })).collect::<Vec<_>>(),
            "scope": {
                "hypothetical_draw": args.draw,
                "other_players_unchanged": true, "shape_only": true,
                "connections_can_overlap": true, "unseen_is_not_wall_count": true,
                "not_mortal_causal_explanation": true
            }
        }
    }))
}

fn branch(branch: &DiscardBranch) -> Value {
    let mut value = discard_evidence(&branch.efficiency);
    value["concealed_after"] = json!(
        branch
            .concealed
            .iter()
            .copied()
            .map(format_tile)
            .collect::<Vec<_>>()
    );
    value["followup"] = match &branch.followup {
        None => Value::Null,
        Some(followup) => {
            let best_shanten = followup.next_discards.iter().map(|d| d.shanten).min();
            let best_unseen = followup
                .next_discards
                .iter()
                .filter(|d| Some(d.shanten) == best_shanten)
                .map(|d| d.total_unseen)
                .max();
            json!({
                "draw": kind_name(followup.draw), "completed_shape": followup.completed_shape,
                "next_discards": followup.next_discards.iter()
                    .map(|discard| (format_tile(discard.discard), discard_evidence(discard)))
                    .collect::<serde_json::Map<String, Value>>(),
                "best_shanten_after_discard": best_shanten,
                "best_unseen_at_best_shanten": best_unseen,
                "best_discards_by_direct_efficiency": followup.next_discards.iter()
                    .filter(|d| Some(d.shanten) == best_shanten && Some(d.total_unseen) == best_unseen)
                    .map(|d| format_tile(d.discard)).collect::<Vec<_>>()
            })
        }
    };
    value
}

fn draws(candidates: &DrawCandidates) -> &[crate::analysis::TileAvailability] {
    match candidates {
        DrawCandidates::Effective(tiles) | DrawCandidates::Winning(tiles) => tiles,
    }
}

fn analysis_error(error: ComparisonError) -> ToolError {
    match error {
        ComparisonError::SameDiscard => ("invalid_arguments", "请比较两个不同的切牌。".into()),
        ComparisonError::ExhaustedDraw { kind } => (
            "exhausted_draw",
            format!("{} 已全部可见，不能假设再摸入一张。", kind_name(kind)),
        ),
        ComparisonError::TooManyCopies { kind } => (
            "invalid_position",
            format!("{} 的可见枚数超过四张。", kind_name(kind)),
        ),
        ComparisonError::InvalidMeld => bad_position(),
        ComparisonError::Analysis(error) => ("analysis_failed", error.to_string()),
    }
}

#[cfg(test)]
mod tests;
