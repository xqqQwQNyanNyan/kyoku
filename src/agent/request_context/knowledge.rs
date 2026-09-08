//! 按当前快照和工具选择小段定义，并把分散的相关事实放在一起。

use crate::{
    agent::position::{kind_name, parse_tile},
    analysis::dora_from_indicator,
};
use serde_json::{Value, json};

const KNOWLEDGE: &str = include_str!("../../../docs/analysis/mahjong-knowledge.md");

pub(super) fn reference(evidence: &Value, current: &[Value]) -> Value {
    let position = &evidence["position"];
    let dora: Vec<_> = position["dora_indicators"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|indicator| {
            let tile = parse_tile(indicator.as_str()?)?;
            Some(json!({"indicator":indicator,"dora":kind_name(dora_from_indicator(tile))}))
        })
        .collect();
    let melds = fixed_melds(evidence);
    let comparisons: Vec<_> = current
        .iter()
        .filter(|call| call["type"] == "function_call" && call["name"] == "compare_discards")
        .filter_map(|call| comparison(current, call, &dora))
        .collect();
    let mut topics = Vec::new();
    if !dora.is_empty() {
        topics.push("dora");
    }
    if !comparisons.is_empty()
        || current.iter().any(|item| {
            item["type"] == "function_call"
                && matches!(
                    item["name"].as_str(),
                    Some(
                        "analyze_hand"
                            | "analyze_yaku_route"
                            | "compare_discards"
                            | "compare_improvements"
                    )
                )
        })
    {
        topics.push("structure");
    }
    if !melds.is_empty()
        || !comparisons.is_empty()
        || current.iter().any(|item| item["name"] == "analyze_actions")
    {
        topics.push("melds");
    }
    let mut facts = json!({});
    if !dora.is_empty() {
        facts["dora_from_indicators"] = json!(dora);
    }
    if !melds.is_empty() {
        facts["fixed_melds"] = json!(melds);
    }
    if !comparisons.is_empty() {
        facts["discard_comparisons"] = json!(comparisons);
    }
    let definitions: Vec<_> = topics.into_iter().filter_map(topic).collect();
    json!({"role":"developer","content":format!(
        "本次输入只含当前局面、同局面最近两轮问答正文和本轮工具结果。旧回答用于理解追问，不替代当前证据；需要核验的旧计算可用工具重新检查。\n相关事实汇集（来自当前快照与本轮工具，不是策略评分或Mortal原因）：{facts}\n按需规则与术语：\n{}",
        definitions.join("\n\n")
    )})
}

fn topic(name: &str) -> Option<&'static str> {
    KNOWLEDGE
        .split("\n## ")
        .skip(1)
        .find(|section| section.split_once('：').is_some_and(|(key, _)| key == name))
}

fn fixed_melds(evidence: &Value) -> Vec<Value> {
    let Some(own) = evidence["player"].as_u64().filter(|&player| player < 4) else {
        return Vec::new();
    };
    evidence["position"]["players"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .flat_map(|(index, player)| {
            player["melds"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(move |meld| {
                    let (name, open) = match meld["kind"].as_str()? {
                        "chi" => ("吃", true),
                        "pon" => ("碰", true),
                        "daiminkan" => ("大明杠", true),
                        "ankan" => ("暗杠", false),
                        "kakan" => ("加杠", true),
                        _ => return None,
                    };
                    let relative = ["自己", "下家", "对家", "上家"][(index + 4 - own as usize) % 4];
                    Some(
                        json!({"player":index,"relative_seat":relative,"kind":meld["kind"],
                    "name":name,"tiles":meld["tiles"],"is_open":open}),
                    )
                })
        })
        .collect()
}

fn comparison(current: &[Value], call: &Value, indicators: &[Value]) -> Option<Value> {
    let output = current.iter().find(|item| {
        item["type"] == "function_call_output" && item["call_id"] == call["call_id"]
    })?;
    let result: Value = serde_json::from_str(output["output"].as_str()?).ok()?;
    if result["ok"] != true {
        return None;
    }
    let dora: Vec<_> = indicators.iter().map(|pair| pair["dora"].clone()).collect();
    let candidates: Option<Vec<_>> = ["first", "second"]
        .iter()
        .map(|key| candidate(&result["comparison"][key], &dora))
        .collect();
    Some(
        json!({"source_call_id":call["call_id"],"stage":"切牌后的直接进张，尚未执行假设摸牌",
        "candidates":candidates?,"counts_are_not_value_weighted":true}),
    )
}

fn candidate(hand: &Value, dora: &[Value]) -> Option<Value> {
    let draws = hand["draws"].as_array()?;
    let concealed = hand["concealed_after"].as_array()?;
    let bonus_draws: Vec<_> = draws
        .iter()
        .filter(|draw| dora.contains(&draw["tile"]))
        .map(|draw| {
            let nearby: Vec<_> = concealed
                .iter()
                .filter(|tile| nearby_number_tiles(tile, &draw["tile"]))
                .cloned()
                .collect();
            json!({"tile":draw["tile"],"unseen":draw["unseen"],"nearby_concealed_tiles":nearby})
        })
        .collect();
    Some(json!({"discard":hand["discard"],"shanten":hand["shanten"],
        "effective_tile_kind_count":draws.len(),"effective_unseen_count":hand["total_unseen"],
        "dora_effective_draws":bonus_draws}))
}

fn nearby_number_tiles(first: &Value, second: &Value) -> bool {
    let Some(first) = first.as_str().and_then(parse_tile) else {
        return false;
    };
    let Some(second) = second.as_str().and_then(parse_tile) else {
        return false;
    };
    let first = first.kind().as_u8();
    let second = second.kind().as_u8();
    first < 27
        && second < 27
        && first / 9 == second / 9
        && (1..=2).contains(&first.abs_diff(second))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearby_tiles_do_not_cross_suits_wrap_terminals_or_connect_honors() {
        for (first, second, expected) in [
            ("7m", "8m", true),
            ("5mr", "6m", true),
            ("7m", "9m", true),
            ("9m", "1m", false),
            ("9m", "1p", false),
            ("7p", "8m", false),
            ("F", "C", false),
            ("8m", "8m", false),
        ] {
            assert_eq!(nearby_number_tiles(&json!(first), &json!(second)), expected);
        }
    }
}
