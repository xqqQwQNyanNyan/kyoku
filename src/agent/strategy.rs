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
            "compare_discard_safety",
            "只比较两个切牌对各家的安全依据、切后现物库存及公开信息允许的鸣牌。牌形用compare_discards，待牌打点用analyze_waits；不算放铳率。",
            json!({"first":{"type":"string"},"second":{"type":"string"}}),
        ),
        definition(
            "analyze_discard_followup",
            "只核验指定的切牌→普通摸牌→再次切牌这一条分支，返回待牌打点、结构和现物库存；不枚举其他分支。先用compare_discards的draw参数查看后续切牌候选；摸牌完成牌形时next_discard=null查看条件自摸打点。",
            json!({"discard":{"type":"string"},"draw":{"type":"string"},"next_discard":{"type":["string","null"]}}),
        ),
        definition(
            "analyze_waits",
            "只分析等效13张或指定切牌后的待牌、有役、舍牌振听及默听/立直的条件符番支付；不展开役种路线、后续摸切或四家结算。discard=null分析当前13张。",
            json!({"discard":{"type":["string","null"]}}),
        ),
        definition(
            "analyze_hand",
            "只看当前等效13张或指定切牌后的向听、直接进张、结构和已知宝牌。打点另用analyze_waits，役种距离另用analyze_yaku_route。discard=null分析当前13张。",
            json!({"discard":{"type":["string","null"]}}),
        ),
        definition(
            "analyze_yaku_route",
            "只计算当前13张或切牌后到一个指定役种的距离，不展开后续摸切。需要推进牌时另用analyze_yaku_progression。距离不是完成概率。",
            json!({"discard":{"type":["string","null"]},"yaku":{"type":"string","enum":hand::ROUTES.iter().map(|(name,_)| *name).collect::<Vec<_>>()}}),
        ),
        definition(
            "analyze_yaku_progression",
            "明确研究某一役种的推进牌时调用，展开该役种的一次摸切。只研究距离时用analyze_yaku_route；不返回其他役种、待牌打点或结算。",
            json!({"discard":{"type":["string","null"]},"yaku":{"type":"string","enum":hand::ROUTES.iter().map(|(name,_)| *name).collect::<Vec<_>>()}}),
        ),
        definition(
            "analyze_defense",
            "逐张检查自家暗牌针对各家的现物、立直后已通过牌、筋来源及可见枚数，统计切牌后剩余现物。仅提供防守证据，不计算放铳率；筋和壁不保证安全。",
            json!({}),
        ),
        definition(
            "analyze_actions",
            "先列出当前Mortal提供的动作、吃碰赤牌变体、可切牌的向听和进张总数，以及过牌后的即时振听。不批量计算吃碰后打点、役种或所有岭上摸牌；需要细节时用analyze_action_details点选。",
            json!({}),
        ),
        definition(
            "analyze_action_details",
            "展开analyze_actions中的一个分支。过牌三个附加字段全为null；立直指定discard；吃碰指定variant与discard；杠指定variant，draw=null仅看杠后状态，指定普通draw仅展开这一张岭上牌。variant原样使用摘要中的键。",
            json!({"action":{"type":"string","enum":["pass","riichi","chi_low","chi_middle","chi_high","pon","kan"]},
                "variant":{"type":["string","null"]},"discard":{"type":["string","null"]},"draw":{"type":["string","null"]}}),
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
            "compare_discard_safety" => facts::compare_safety(&snapshot, evidence, &args)?,
            "analyze_discard_followup" => facts::followup(&snapshot, evidence, &args)?,
            "analyze_waits" => hand::waits(&snapshot, evidence, &args)?,
            "analyze_hand" => hand::analyze(&snapshot, evidence, &args)?,
            "analyze_yaku_route" => hand::route(&snapshot, evidence, &args, false)?,
            "analyze_yaku_progression" => hand::route(&snapshot, evidence, &args, true)?,
            "analyze_defense" => ("defense".into(), defense::analyze(&snapshot)?),
            "analyze_actions" => ("actions".into(), actions::analyze(&snapshot, evidence)?),
            "analyze_action_details" => actions::detail(&snapshot, evidence, &args)?,
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
