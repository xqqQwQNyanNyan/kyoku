//! 面向复盘问题的计算工具。每次只读取固定快照，结果可被逐项引用。

use super::position::{Snapshot, ToolError};
use serde_json::{Value, json};

mod actions;
mod defense;
mod facts;
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
            "compare_discard_facts",
            "完整比较两个已有切牌候选时首先调用。已包含双方analyze_hand的事实、舍牌安全依据、防守库存和流局组合，不需为相同问题再调用analyze_hand或analyze_defense。draw=null比较当前事实；指定普通牌则保留该次假设摸牌后的所有切牌事实，不按受入筛选。没有综合评分，不模拟他家未来。",
            json!({"first":{"type":"string"},"second":{"type":"string"},"draw":{"type":["string","null"]}}),
        ),
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
        definition(
            "analyze_draw_outcomes",
            "枚举荒牌流局时四家听牌与未听的16种给定组合，计算3000点罚符、当前顺位、本场供托与继续比赛时的庄家。不是流局概率，不含途中流局、流局满贯或终局预测。",
            json!({}),
        ),
        definition(
            "analyze_win_outcome",
            "给定和牌者、付款者、符和总番，计算本场供托及四家点数顺位变化；可核验放铳损失、自摸或横移的条件后果。payer=null 表示自摸。符番由问题条件明确给出，至少有一役；不推测他家暗手价值，不判断该符番可达。",
            json!({"winner":{"type":"integer","minimum":0,"maximum":3},"payer":{"type":["integer","null"],"minimum":0,"maximum":3},
                "fu":{"type":"integer","minimum":20,"maximum":110},"han":{"type":"integer","minimum":1,"maximum":13}}),
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
            "compare_discard_facts" => facts::compare(&snapshot, evidence, &args)?,
            "analyze_hand" => hand::analyze(&snapshot, evidence, &args)?,
            "analyze_yaku_route" => hand::route(&snapshot, evidence, &args)?,
            "analyze_defense" => ("defense".into(), defense::analyze(&snapshot)?),
            "analyze_actions" => ("actions".into(), actions::analyze(&snapshot, evidence)?),
            "analyze_score_targets" => scores::analyze(&snapshot, &args)?,
            "analyze_draw_outcomes" => ("draw_outcomes".into(), scores::draws(&snapshot)),
            "analyze_win_outcome" => scores::win(&snapshot, &args)?,
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

#[cfg(test)]
mod tests;
