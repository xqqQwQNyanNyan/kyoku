//! 组织本轮模型输入；完整历史与原始工具结果仍由会话保存。

use serde_json::{Value, json};

mod knowledge;

/// 只带当前局面、最近两轮问答正文和本轮完整工具链。
pub(super) fn prepare(history: &[Value]) -> Vec<Value> {
    let Some((start, evidence)) = history
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, item)| super::context_evidence(item).map(|value| (index, value)))
    else {
        // 连接探测及只有 get_review 工具消息的旧会话没有可靠的局面分段标记。
        return history.to_vec();
    };
    // 同一事件补上分析结果时更新证据，但继续保留该局面的追问正文。
    let mut conversation_start = start;
    for (index, previous) in history[..start]
        .iter()
        .enumerate()
        .rev()
        .filter_map(|(index, item)| super::context_evidence(item).map(|value| (index, value)))
    {
        if previous["player"] != evidence["player"]
            || previous["event_index"] != evidence["event_index"]
            || previous["position"] != evidence["position"]
        {
            break;
        }
        conversation_start = index;
    }
    let segment = &history[conversation_start + 1..];
    let questions: Vec<_> = segment
        .iter()
        .enumerate()
        .filter(|(_, item)| item["role"] == "user")
        .map(|(index, _)| index)
        .collect();
    let Some(&current) = questions.last() else {
        return history.to_vec();
    };
    let mut input = vec![history[start].clone()];
    for pair in questions[questions.len().saturating_sub(3)..].windows(2) {
        input.push(segment[pair[0]].clone());
        let turn = &segment[pair[0] + 1..pair[1]];
        // 工具调用阶段的解释尚未核查，追问只接续本轮最后一次工具之后的终稿。
        let final_start = turn
            .iter()
            .rposition(|item| item["type"] == "function_call_output")
            .map_or(0, |index| index + 1);
        let text: Vec<_> = turn[final_start..]
            .iter()
            .filter(|item| item["type"] == "message" && item["role"] == "assistant")
            .filter_map(|item| item["content"].as_array())
            .flatten()
            .filter(|part| part["type"] == "output_text")
            .filter_map(|part| part["text"].as_str())
            .collect();
        if !text.is_empty() {
            input.push(json!({"role":"assistant","content":text.join("\n\n")}));
        }
    }
    // 只移除已结束轮次的供应商推理；本轮工具续答仍需要原始消息与调用配对。
    input.extend_from_slice(&segment[current..]);
    input.push(knowledge::reference(&evidence, &segment[current..]));
    input
}

#[cfg(test)]
mod tests;
