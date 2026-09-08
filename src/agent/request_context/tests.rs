use super::*;
use crate::agent::{client, comparison, initial_evidence};

fn evidence() -> Value {
    let concealed = json!([
        "7m", "1p", "3p", "3p", "4p", "4p", "5p", "6p", "7p", "6s", "P", "F", "F", "C"
    ]);
    json!({"player":0,"event_index":208,"analysis_status":"available",
        "discards":concealed.as_array().unwrap().iter().map(|tile|json!({"discard":tile})).collect::<Vec<_>>(),
        "position":{"concealed":concealed,"dora_indicators":["7m"],"remaining_draws":55,"dealer":1,
            "round":{"wind":"E","number":2},"honba":1,"riichi_sticks":0,
            "phase":{"kind":"after_draw","player":0},"history":null,
            "players":(0..4).map(|player|json!({"player":player,"riichi":"not_declared","score":25000,
                "melds":[],"discards":[]})).collect::<Vec<_>>()}})
}

fn message(text: &str) -> Value {
    json!({"type":"message","role":"assistant","status":"completed",
        "content":[{"type":"output_text","text":text}]})
}

fn facts(input: &[Value]) -> Value {
    let text = input.last().unwrap()["content"].as_str().unwrap();
    let line = text.lines().nth(1).unwrap();
    serde_json::from_str(line.split_once('：').unwrap().1).unwrap()
}

#[test]
fn current_position_and_recent_answers_replace_old_tool_logs_without_changing_history() {
    let mut previous = evidence();
    previous["event_index"] = json!(150);
    let mut history = vec![
        initial_evidence(&previous),
        json!({"role":"user","content":"旧局面的牌"}),
        message("旧局面的回答"),
        initial_evidence(&evidence()),
    ];
    for index in 0..3 {
        history.push(json!({"role":"user","content":format!("追问{index}")}));
        history.push(json!({"type":"reasoning","encrypted_content":format!("old-{index}")}));
        history.push(json!({"type":"function_call","name":"analyze_defense","call_id":format!("old-{index}"),"arguments":"{}","status":"completed"}));
        history.push(json!({"type":"function_call_output","call_id":format!("old-{index}"),"output":"{\"ok\":true}"}));
        history.push(message(&format!("回答{index}")));
    }
    history.push(json!({"role":"user","content":"当前的问题"}));
    let current = history.len() - 1;
    history.push(json!({"type":"reasoning","encrypted_content":"current-reasoning"}));
    history.push(json!({"type":"function_call","name":"analyze_defense","call_id":"current","arguments":"{}","status":"completed"}));
    history
        .push(json!({"type":"function_call_output","call_id":"current","output":"{\"ok\":true}"}));
    let saved = history.clone();
    let input = prepare(&history);
    assert_eq!(history, saved);
    assert_eq!(input[0], initial_evidence(&evidence()));
    assert!(!json!(input).to_string().contains("旧局面的"));
    assert!(!json!(input).to_string().contains("追问0"));
    assert!(!json!(input).to_string().contains("old-"));
    assert_eq!(input[1]["content"], "追问1");
    assert_eq!(input[2], json!({"role":"assistant","content":"回答1"}));
    assert_eq!(input[3]["content"], "追问2");
    assert_eq!(&input[5..input.len() - 1], &history[current..]);
}

#[test]
fn chat_keeps_current_tool_groups_and_sends_old_answers_as_text() {
    let mut old = message("前一轮回答");
    old["_chat_message"] =
        json!({"role":"assistant","content":"前一轮回答","reasoning_content":"old-reasoning"});
    old["_chat_output_count"] = json!(1);
    let group = json!({"role":"assistant","content":null,"reasoning_content":"current-reasoning",
        "tool_calls":[{"id":"a","type":"function","function":{"name":"analyze_defense","arguments":"{}"}},
            {"id":"b","type":"function","function":{"name":"analyze_actions","arguments":"{}"}}]});
    let current = client::validate_chat_history(group.clone()).unwrap();
    let mut history = vec![
        initial_evidence(&evidence()),
        json!({"role":"user","content":"之前的问题"}),
        old,
        json!({"role":"user","content":"继续比较"}),
    ];
    history.extend(current);
    history.push(json!({"type":"function_call_output","call_id":"a","output":"{\"ok\":true}"}));
    history.push(json!({"type":"function_call_output","call_id":"b","output":"{\"ok\":true}"}));
    let input = prepare(&history);
    assert!(!json!(input).to_string().contains("old-reasoning"));
    assert!(json!(history).to_string().contains("old-reasoning"));
    assert!(
        input
            .iter()
            .any(|item| item.get("_chat_message") == Some(&group))
    );
    assert!(input.contains(&json!({"role":"assistant","content":"前一轮回答"})));
    assert_eq!(
        input
            .iter()
            .filter(|item| item["type"] == "function_call_output")
            .count(),
        2
    );
}

#[test]
fn dora_adjacency_regression_groups_real_effective_draws_and_kept_tiles() {
    let evidence = evidence();
    let args = r#"{"first":"6s","second":"7m","draw":null}"#;
    let result = comparison::execute(&evidence, args);
    assert_eq!(result["ok"], true, "{result}");
    let input = prepare(&[
        initial_evidence(&evidence),
        json!({"role":"user","content":"6s更靠中间，为什么先切6s不切7m？"}),
        json!({"type":"function_call","status":"completed","name":"compare_discards","call_id":"dora-case","arguments":args}),
        json!({"type":"function_call_output","call_id":"dora-case","output":result.to_string()}),
    ]);
    let facts = facts(&input);
    assert_eq!(
        facts["dora_from_indicators"],
        json!([{"indicator":"7m","dora":"8m"}])
    );
    let candidates = &facts["discard_comparisons"][0]["candidates"];
    assert_eq!(candidates[0]["discard"], "6s");
    assert_eq!(candidates[0]["shanten"], 3);
    assert_eq!(candidates[0]["effective_tile_kind_count"], 17);
    assert_eq!(candidates[1]["effective_tile_kind_count"], 17);
    assert_eq!(
        candidates[0]["dora_effective_draws"],
        json!([{"tile":"8m","unseen":4,"nearby_concealed_tiles":["7m"]}])
    );
    assert_eq!(candidates[1]["dora_effective_draws"], json!([]));
    let knowledge = input.last().unwrap()["content"].as_str().unwrap();
    assert!(knowledge.contains("没有按宝牌"));
    assert!(knowledge.contains("自己的弃牌只能被下家吃"));
}

#[test]
fn kan_regression_preserves_types_and_relative_seats_without_inferring_from_visibility() {
    let mut evidence = evidence();
    evidence["player"] = json!(1);
    for (player, kind, tile) in [(2, "daiminkan", "C"), (3, "ankan", "P"), (0, "kakan", "F")] {
        evidence["position"]["players"][player]["melds"] =
            json!([{"kind":kind,"tiles":[tile,tile,tile,tile]}]);
    }
    let input = prepare(&[
        initial_evidence(&evidence),
        json!({"role":"user","content":"下家的中是什么杠，还能立直吗？"}),
    ]);
    let facts = facts(&input);
    let melds = facts["fixed_melds"].as_array().unwrap();
    let daiminkan = melds
        .iter()
        .find(|meld| meld["kind"] == "daiminkan")
        .unwrap();
    assert_eq!(daiminkan["name"], "大明杠");
    assert_eq!(daiminkan["relative_seat"], "下家");
    assert_eq!(daiminkan["is_open"], true);
    let ankan = melds.iter().find(|meld| meld["kind"] == "ankan").unwrap();
    assert_eq!(ankan["name"], "暗杠");
    assert_eq!(ankan["is_open"], false);
    assert_eq!(
        melds.iter().find(|meld| meld["kind"] == "kakan").unwrap()["name"],
        "加杠"
    );
    let knowledge = input.last().unwrap()["content"].as_str().unwrap();
    assert!(knowledge.contains("牌是否显示在桌面上不能用来判断"));
    assert!(!knowledge.contains("structure："));
}

#[test]
fn unavailable_or_failed_analysis_does_not_create_candidate_facts() {
    let mut evidence = evidence();
    evidence["position"]["dora_indicators"] = json!([]);
    let history = vec![
        initial_evidence(&evidence),
        json!({"role":"user","content":"比较"}),
        json!({"type":"function_call","name":"compare_discards","call_id":"bad"}),
        json!({"type":"function_call_output","call_id":"bad","output":"{\"ok\":false,\"error\":{}}"}),
    ];
    let facts = facts(&prepare(&history));
    assert_eq!(facts, json!({}));
    let probe = vec![json!({"role":"user","content":"连接测试"})];
    assert_eq!(prepare(&probe), probe);
}

#[test]
fn analysis_update_keeps_same_position_followup_but_only_sends_latest_evidence() {
    let analysed = evidence();
    let mut before = analysed.clone();
    before["analysis_status"] = json!("not_analyzed");
    before["discards"] = json!([]);
    let input = prepare(&[
        initial_evidence(&before),
        json!({"role":"user","content":"这里的牌形怎样？"}),
        message("尚未提供分析。"),
        initial_evidence(&analysed),
        json!({"role":"user","content":"现在分析出来了，继续说。"}),
    ]);
    assert_eq!(input[0], initial_evidence(&analysed));
    assert!(input.contains(&json!({"role":"assistant","content":"尚未提供分析。"})));
    assert_eq!(
        input
            .iter()
            .filter_map(crate::agent::context_evidence)
            .count(),
        1
    );
}

#[test]
fn followups_do_not_repeat_unverified_text_from_the_tool_phase() {
    let history = vec![
        initial_evidence(&evidence()),
        json!({"role":"user","content":"前问"}),
        message("工具调用前的未核查解释"),
        json!({"type":"function_call","name":"get_review","call_id":"r","arguments":"{}"}),
        json!({"type":"function_call_output","call_id":"r","output":"{}"}),
        message("核查后的终稿"),
        json!({"role":"user","content":"追问"}),
    ];
    let input = prepare(&history);
    assert!(!json!(input).to_string().contains("工具调用前的未核查解释"));
    assert!(json!(input).to_string().contains("核查后的终稿"));
    assert!(
        json!(history)
            .to_string()
            .contains("工具调用前的未核查解释")
    );
}
