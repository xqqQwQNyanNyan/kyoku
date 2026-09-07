//! 面向复盘问题的计算工具。每次只读取固定快照，结果可被逐项引用。

use super::position::{Snapshot, ToolError};
use serde_json::{Value, json};

mod actions;
mod defense;
mod hand;
mod scores;

fn definition(name: &str, description: &str, properties: Value) -> Value {
    let required: Vec<_> = properties
        .as_object()
        .into_iter()
        .flat_map(|p| p.keys())
        .cloned()
        .collect();
    json!({"type":"function", "name":name, "description":description, "strict":true,
        "parameters":{"type":"object","properties":properties,"required":required,"additionalProperties":false}})
}

pub(super) fn definitions() -> Vec<Value> {
    vec![
        definition(
            "analyze_hand",
            "分析当前等效13张手牌，或当前候选切牌之后的手牌：向听、常用役种路线、完整待牌及荣和/自摸条件打点，符合条件时比较立直。检查自家舍牌振听，不声称已完成全部和牌合法性检查。discard=null 分析当前13张，否则须用当前 discards 中的牌。",
            json!({"discard":{"type":["string","null"]}}),
        ),
        definition(
            "analyze_yaku_route",
            "计算当前13张手牌或切牌后到指定役种完成形的向听。不可达与尚未支持不同；距离不代表完成概率或预计打点。",
            json!({"discard":{"type":["string","null"]},"yaku":{"type":"string","enum":hand::ROUTES.iter().map(|(name,_)| *name).collect::<Vec<_>>()}}),
        ),
        definition(
            "analyze_defense",
            "逐张检查自家暗牌针对各家的现物、立直后已通过牌、筋来源及可见枚数，统计切牌后剩余现物。仅提供防守证据，不计算放铳率；筋和壁不保证安全。",
            json!({}),
        ),
        definition(
            "analyze_actions",
            "分析当前 Mortal 已提供的吃碰杠、立直和跳过候选的具体后果；跳过检查见逃后的即时振听，null 表示原状态不明；吃碰枚举赤牌消耗差异及禁止食替后的切牌，杠只作固定公开信息下的岭上牌形分支。没有总体收益、放铳率或未知新宝牌。",
            json!({}),
        ),
        definition(
            "analyze_score_targets",
            "按当前点数、本场、立直棒和起家同点顺序，计算超过指定玩家所需的荣和支付门槛以及自摸支付档位。是本次单人和牌后的点差条件，不是终局预测，也不代表自家手牌可达到相应打点。",
            json!({"target":{"type":"integer","minimum":0,"maximum":3}}),
        ),
    ]
}

pub(super) fn execute(name: &str, evidence: &Value, arguments: &str) -> Value {
    let run = || -> Result<Value, ToolError> {
        let definition = definitions()
            .into_iter()
            .find(|d| d["name"] == name)
            .ok_or_else(|| ("unknown_tool", "不存在此分析工具。".into()))?;
        let args: Value = serde_json::from_str(arguments).map_err(|_| invalid_arguments())?;
        let object = args.as_object().ok_or_else(invalid_arguments)?;
        let required = definition["parameters"]["required"]
            .as_array()
            .ok_or_else(invalid_arguments)?;
        if object.len() != required.len()
            || required
                .iter()
                .any(|key| !object.contains_key(key.as_str().unwrap()))
        {
            return Err(invalid_arguments());
        }
        let snapshot = Snapshot::read(evidence)?;
        let (key, analysis) = match name {
            "analyze_hand" => hand::analyze(&snapshot, evidence, &args)?,
            "analyze_yaku_route" => hand::route(&snapshot, evidence, &args)?,
            "analyze_defense" => ("defense".into(), defense::analyze(&snapshot)?),
            "analyze_actions" => ("actions".into(), actions::analyze(&snapshot, evidence)?),
            "analyze_score_targets" => scores::analyze(&snapshot, &args)?,
            _ => return Err(invalid_arguments()),
        };
        Ok(json!({"ok":true,"key":key,"reference":format!("/analyses/{key}"),"analysis":analysis}))
    };
    match run() {
        Ok(value) => value,
        Err((code, message)) => json!({"ok":false,"error":{"code":code,"message":message}}),
    }
}

fn invalid_arguments() -> ToolError {
    (
        "invalid_arguments",
        "参数必须包含工具要求的全部字段，且不能添加额外字段。".into(),
    )
}

pub(super) fn remember(evidence: &mut Value, result: &Value) {
    if result["ok"] == true
        && result["analysis"].is_object()
        && let Some(key) = result["key"].as_str()
    {
        evidence["analyses"][key] = result["analysis"].clone();
    }
}

#[cfg(test)]
mod tests;
