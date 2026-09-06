use serde::Deserialize;
use serde_json::Value;

/// 模型只提供段落内容；来源标签和段落间隔由程序生成。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Answer {
    sections: Vec<Section>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Section {
    source: Source,
    text: String,
    facts: Vec<Fact>,
    #[serde(default)]
    draws_for: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Source {
    Position,
    Calculation,
    Mortal,
    Limitation,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fact {
    path: String,
    value: Value,
}

pub(super) fn render(text: &str, evidence: &Value) -> Result<String, String> {
    let answer: Answer = serde_json::from_str(text)
        .map_err(|_| "请只返回约定的 JSON 对象，sections 每项必须包含 source、text、facts，可选 draws_for；不要添加其他字段或代码围栏。")?;
    if answer.sections.is_empty() || answer.sections.len() > 6 {
        return Err("sections 必须包含 1 至 6 个段落。".into());
    }
    let mut paragraphs = Vec::new();
    for section in answer.sections {
        let text = section.text.trim();
        // 不让模型生成第二套排版，也避免终端控制字符进入 CLI。
        if text.is_empty()
            || text.len() > 3000
            || text.chars().any(char::is_control)
            || ["**", "`", "#", "|", "<", ">", "【", "】", "\\n"]
                .iter()
                .any(|mark| text.contains(mark))
        {
            return Err(
                "text 必须是简短单段纯文本，不含 Markdown、HTML、来源标签、控制字符或字面量反斜杠 n。".into(),
            );
        }
        let (label, prefix) = match section.source {
            Source::Position => ("局面", "/position/"),
            Source::Calculation => {
                if evidence["analysis_status"] != "available" {
                    return Err(
                        "当前没有可用切牌计算，只能说明未分析或未提供，不得生成 calculation 段落。"
                            .into(),
                    );
                }
                ("计算", "/discards/")
            }
            Source::Mortal => {
                if evidence["mortal"]["status"] != "available" {
                    return Err(
                        "当前没有 Mortal 决策，不得生成 mortal 段落；程序会附上实际状态。".into(),
                    );
                }
                ("Mortal", "/mortal/decision/")
            }
            Source::Limitation => ("说明", ""),
        };
        if section.facts.len() > 24 || (!prefix.is_empty() && section.facts.is_empty()) {
            return Err(
                "事实段落必须引用 1 至 24 项与来源一致的证据，使用相对于 review 的 JSON Pointer 和原始 value。".into(),
            );
        }
        for fact in section.facts {
            if (!prefix.is_empty() && !fact.path.starts_with(prefix))
                || !(fact.path.starts_with("/position/")
                    || fact.path.starts_with("/discards/")
                    || fact.path.starts_with("/mortal/decision/"))
            {
                return Err(format!(
                    "{label}段落的 facts 来源不匹配；请仅引用 {prefix} 下的证据，其他来源拆成独立段落。"
                ));
            }
            let expected = evidence
                .pointer(&fact.path)
                .filter(|value| !value.is_null())
                .ok_or_else(|| {
                    "证据引用不存在或为 null；请核对工具的实际字段，不要猜测路径。".to_owned()
                })?;
            if &fact.value != expected {
                // 反馈只引用已有证据，不回显模型补造的值。
                return Err(format!(
                    "证据 {} 的 value 必须原样使用 {}，不能四舍五入或改写；请同时核对正文。",
                    fact.path, expected
                ));
            }
        }
        let mut paragraph = format!("【{label}】{}", display_text(text, evidence));
        if let Some(discard) = section.draws_for {
            if !matches!(section.source, Source::Calculation) {
                return Err("draws_for 只能用于 calculation 段落。".into());
            }
            paragraph.push_str("\n\n");
            paragraph.push_str(&draw_list(&discard, evidence)?);
        }
        paragraphs.push(paragraph);
    }
    match evidence["mortal"]["status"].as_str() {
        Some("not_analyzed") => paragraphs.push("【Mortal】未分析。".into()),
        Some("no_decision") => {
            paragraphs.push("【Mortal】此事件没有该玩家的决策结果，不代表主动跳过。".into())
        }
        _ => {}
    }
    Ok(paragraphs.join("\n\n"))
}

fn honor(tile: &str) -> Option<&'static str> {
    match tile {
        "E" => Some("东"),
        "S" => Some("南"),
        "W" => Some("西"),
        "N" => Some("北"),
        "P" => Some("白"),
        "F" => Some("发"),
        "C" => Some("中"),
        _ => None,
    }
}

fn display_text(text: &str, evidence: &Value) -> String {
    let decision = &evidence["mortal"]["decision"];
    let q_values: Vec<_> = ["candidates", "kan_candidates"]
        .into_iter()
        .flat_map(|key| decision[key].as_array().into_iter().flatten())
        .filter_map(|candidate| candidate["q_value"].as_f64())
        .collect();
    let mut rendered = String::new();
    let mut token = String::new();
    let append = |token: &str, rendered: &mut String| {
        if let Some(name) = honor(token) {
            rendered.push_str(name);
        } else if let Ok(value) = token.parse::<f64>()
            && token.contains(['.', 'e', 'E'])
            && q_values.contains(&value)
        {
            // 只缩短与原始 Q 相符的数值，不改写点棒、枚数或其他数字。
            let value = if value.abs() < 0.0005 { 0.0 } else { value };
            rendered.push_str(&format!("{value:.3}"));
        } else {
            rendered.push_str(token);
        }
    };
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '+' | '-' | '_') {
            token.push(ch);
        } else {
            append(&token, &mut rendered);
            token.clear();
            rendered.push(ch);
        }
    }
    append(&token, &mut rendered);
    rendered
}

fn draw_list(discard: &str, evidence: &Value) -> Result<String, String> {
    let candidate = evidence["discards"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|candidate| candidate["discard"] == discard)
        .ok_or("draws_for 必须使用工具实际提供的 discard 原始牌值，不能自行编造候选。")?;
    let draws = candidate["draws"]
        .as_array()
        .ok_or("该切牌没有进张明细。")?;
    // 按花色和不可见枚数分组，避免让模型重抄长列表时漏牌、错译或加错总数。
    let mut groups: [std::collections::BTreeMap<u64, Vec<String>>; 4] =
        std::array::from_fn(|_| std::collections::BTreeMap::new());
    let mut total = 0;
    for draw in draws {
        let tile = draw["tile"].as_str().ok_or("进张牌值缺失。")?;
        let unseen = draw["unseen"]
            .as_u64()
            .filter(|count| *count <= 4)
            .ok_or("不可见枚数无效。")?;
        let (suit, name) = if let Some(name) = honor(tile) {
            (3, name.to_owned())
        } else {
            let bytes = tile.as_bytes();
            if bytes.len() != 2 || !(b'1'..=b'9').contains(&bytes[0]) {
                return Err("进张牌值无效。".into());
            }
            let suit = match bytes[1] {
                b'm' => 0,
                b'p' => 1,
                b's' => 2,
                _ => return Err("进张花色无效。".into()),
            };
            (suit, tile.to_owned())
        };
        groups[suit].entry(unseen).or_default().push(name);
        total += unseen;
    }
    if candidate["total_unseen"].as_u64() != Some(total) {
        return Err("进张明细与不可见总数不一致。".into());
    }
    let title = match candidate["draw_kind"].as_str() {
        Some("effective") => "有效牌",
        Some("winning_shape") => "完成牌形的牌（不代表可以合法和牌）",
        _ => return Err("进张类型无效。".into()),
    };
    let discard = honor(discard).unwrap_or(discard);
    let mut lines = vec![format!("切 {discard} 后的{title}：")];
    for (suit, group) in ["万", "筒", "索", "字牌"].into_iter().zip(groups) {
        if group.is_empty() {
            continue;
        }
        let parts: Vec<_> = group
            .iter()
            .rev()
            .map(|(count, tiles)| {
                let each = if tiles.len() > 1 { "各" } else { "" };
                format!("{}（{each}{count}枚）", tiles.join("、"))
            })
            .collect();
        lines.push(format!("- {suit}：{}", parts.join("；")));
    }
    // 空行让 GUI 的 Markdown 渲染保留列表；CLI 也能直接阅读。
    lines.insert(1, String::new());
    lines.push(format!(
        "\n共 {} 种，合计 {total} 枚不可见牌（包含对手暗牌，并非牌山剩余枚数）。",
        draws.len()
    ));
    Ok(lines.join("\n"))
}
