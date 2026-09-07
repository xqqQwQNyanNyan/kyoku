//! 比较工具只消费固定快照，假设摸牌由参数提供，不能访问实际后续牌谱。

use serde::Deserialize;
use serde_json::{Value, json};

use super::evidence::discard_evidence;
use crate::{
    analysis::{
        DrawCandidates,
        discard_comparison::{ComparisonContext, ComparisonError, DiscardBranch},
    },
    mahjong::{
        hand::Hand,
        meld::Meld,
        player_index::PlayerIndex,
        tile::{Tile, TileKind},
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

#[derive(Deserialize)]
struct Position {
    concealed: Vec<String>,
    dora_indicators: Vec<String>,
    players: Vec<PublicPlayer>,
    remaining_draws: u8,
}

#[derive(Deserialize)]
struct PublicPlayer {
    player: u8,
    riichi: String,
    melds: Vec<PublicMeld>,
    discards: Vec<PublicDiscard>,
}

#[derive(Deserialize)]
struct PublicDiscard {
    tile: String,
    called: bool,
}

#[derive(Deserialize)]
struct PublicMeld {
    kind: String,
    tiles: Vec<String>,
    called: Option<String>,
    from: Option<u8>,
}

pub(super) fn definition() -> Value {
    let tiles: Vec<_> = (0..=Tile::MAX_VALUE)
        .filter_map(Tile::new)
        .map(format_tile)
        .collect();
    let mut draws: Vec<Value> = tiles.iter().take(34).map(|s| json!(s)).collect();
    draws.push(Value::Null);
    json!({
        "type": "function", "name": "compare_discards", "strict": true,
        "description": "比较 get_review 已提供的两个不同切牌。返回切后暗牌、局部连接、向听和直接进张差异。draw=null 只比较当前切牌；指定普通牌记法则假设双方随后摸到同种牌，枚举下一次切牌的牌形效率，用于核验具体改良假设。连接可能重叠，不是唯一拆分。假设其他玩家无动作、公开牌不变；不模拟鸣牌、立直、打点或防守，不读取实际未来，不证明 Mortal 的内在原因。",
        "parameters": {
            "type": "object", "additionalProperties": false,
            "properties": {
                "first": {"type": "string", "enum": tiles, "description": "第一个切牌，须在当前 discards 中。"},
                "second": {"type": "string", "enum": tiles, "description": "作为对照的另一个切牌，须在当前 discards 中。"},
                "draw": {"type": ["string", "null"], "enum": draws, "description": "假设摸入的牌种，不区分赤牌；不研究后续时传 null。"}
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
    let player = evidence["player"]
        .as_u64()
        .filter(|&p| p < 4)
        .ok_or_else(bad_position)? as usize;
    let position: Position =
        serde_json::from_value(evidence["position"].clone()).map_err(|_| bad_position())?;
    if position.players.len() != 4
        || position
            .players
            .iter()
            .enumerate()
            .any(|(i, p)| p.player as usize != i)
    {
        return Err(bad_position());
    }
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
    let hand = Hand::new(
        parse_tiles(&position.concealed)?,
        position.players[player]
            .melds
            .iter()
            .map(parse_meld)
            .collect::<Result<_, _>>()?,
    )
    .map_err(|_| bad_position())?;
    let mut visible = parse_tiles(&position.dora_indicators)?;
    for (index, public) in position.players.iter().enumerate() {
        for discard in &public.discards {
            if !discard.called {
                visible.push(parse_tile(&discard.tile).ok_or_else(bad_position)?);
            }
        }
        if index != player {
            for meld in &public.melds {
                visible.extend(parse_tiles(&meld.tiles)?);
            }
        }
    }
    let comparison = ComparisonContext::new(hand, &visible)
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

fn parse_tile(name: &str) -> Option<Tile> {
    (0..=Tile::MAX_VALUE)
        .filter_map(Tile::new)
        .find(|&tile| format_tile(tile) == name)
}

fn kind_name(kind: TileKind) -> String {
    format_tile(
        Tile::try_from(kind.as_u8()).unwrap_or_else(|_| unreachable!("牌种必然是合法普通牌")),
    )
}

fn parse_tiles(names: &[String]) -> Result<Vec<Tile>, ToolError> {
    names
        .iter()
        .map(|name| parse_tile(name).ok_or_else(bad_position))
        .collect()
}

fn parse_meld(meld: &PublicMeld) -> Result<Meld, ToolError> {
    let mut tiles = parse_tiles(&meld.tiles)?;
    tiles.sort_unstable();
    if meld.kind == "ankan" {
        if meld.called.is_some() || meld.from.is_some() {
            return Err(bad_position());
        }
        return Ok(Meld::Ankan {
            tiles: tiles.try_into().map_err(|_| bad_position())?,
        });
    }
    let called = meld
        .called
        .as_deref()
        .and_then(parse_tile)
        .ok_or_else(bad_position)?;
    let from = PlayerIndex::new(meld.from.ok_or_else(bad_position)?).ok_or_else(bad_position)?;
    Ok(match meld.kind.as_str() {
        "chi" => Meld::Chi {
            tiles: tiles.try_into().map_err(|_| bad_position())?,
            called,
            from,
        },
        "pon" => Meld::Pon {
            tiles: tiles.try_into().map_err(|_| bad_position())?,
            called,
            from,
        },
        "daiminkan" => Meld::Daiminkan {
            tiles: tiles.try_into().map_err(|_| bad_position())?,
            called,
            from,
        },
        "kakan" => Meld::Kakan {
            tiles: tiles.try_into().map_err(|_| bad_position())?,
            called,
            from,
        },
        _ => return Err(bad_position()),
    })
}

fn bad_position() -> ToolError {
    (
        "invalid_position",
        "局面快照缺少有效的手牌或公开信息。".into(),
    )
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

/// 引用只接受实际成功的工具结果；不把模型自己生成的文字加入证据。
pub(super) fn remember(evidence: &mut Value, result: &Value) {
    if result["ok"] == true
        && result["comparison"].is_object()
        && let Some(key) = result["key"].as_str()
    {
        evidence["comparisons"][key] = result["comparison"].clone();
    }
}

#[cfg(test)]
mod tests;
