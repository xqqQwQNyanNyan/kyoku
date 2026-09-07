use super::*;
use crate::agent::{AgentContext, answer, execute_tool, output};

fn evidence() -> Value {
    let log = convlog::tenhou::Log::from_json_str(include_str!(
        "../../../fixtures/tenhou/ranked_game.json"
    ))
    .unwrap();
    let events = convlog::tenhou_to_mjai(&log).unwrap();
    let mut evidence = AgentContext::from_events(&events, PlayerIndex::new(0).unwrap(), 2)
        .unwrap()
        .evidence()
        .clone();
    // 比较工具只用已提供候选的身份，效率重新由可见局面计算。
    evidence["analysis_status"] = json!("available");
    evidence["discards"] = json!([{"discard":"2p"},{"discard":"3m"},{"discard":"E"}]);
    evidence
}

#[test]
fn comparison_uses_visible_snapshot_and_reports_action_differences() {
    let evidence = evidence();
    let result = execute(&evidence, r#"{"first":"2p","second":"E","draw":null}"#);
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["comparison"]["first"]["shanten"], 2);
    assert_eq!(
        result["comparison"]["difference"]["unseen_first_minus_second"],
        0
    );
    assert_eq!(
        result["comparison"]["difference"]["only_first_draws"],
        json!([])
    );
    assert!(
        result["comparison"]["second"]["concealed_after"]
            .as_array()
            .unwrap()
            .contains(&json!("2p"))
    );
    assert!(
        !result["comparison"]["second"]["concealed_after"]
            .as_array()
            .unwrap()
            .contains(&json!("E"))
    );
    assert!(
        !execute_tool(
            &evidence,
            "compare_discards",
            r#"{"first":"2p","second":"E","draw":null}"#
        )
        .1
    );
    let mut altered = evidence.clone();
    altered["unrelated_future"] = json!("must never be read");
    altered["position"]["players"][1]["concealed"] = json!(["4m", "4m", "4m", "4m"]);
    altered["mortal"]["decision"] = json!({"recommended":"invented recommendation"});
    assert_eq!(
        execute(&altered, r#"{"first":"2p","second":"E","draw":null}"#),
        result
    );
}

#[test]
fn issue9_four_man_draw_exposes_a_concrete_followup_difference() {
    let result = execute(&evidence(), r#"{"first":"2p","second":"3m","draw":"4m"}"#);
    assert_eq!(result["ok"], true, "{result}");
    let first = &result["comparison"]["first"]["followup"];
    let second = &result["comparison"]["second"]["followup"];
    assert_eq!(first["draw"], "4m");
    assert_eq!(second["draw"], "4m");
    assert_eq!(first["best_shanten_after_discard"], 2);
    assert_eq!(second["best_shanten_after_discard"], 2);
    assert_eq!(first["best_unseen_at_best_shanten"], 28);
    assert_eq!(second["best_unseen_at_best_shanten"], 20);
    assert_eq!(first["next_discards"]["E"]["total_unseen"], 28);
    assert_eq!(first["next_discards"]["C"]["total_unseen"], 24);
    assert_eq!(second["next_discards"]["4m"]["total_unseen"], 20);
}

#[test]
fn invalid_arguments_and_unavailable_states_cannot_produce_comparisons() {
    let evidence = evidence();
    for args in [
        "{}",
        "[]",
        "null",
        r#"{"first":"2p","second":"E"}"#,
        r#"{"first":"2p","second":"E","draw":"5mr"}"#,
        r#"{"first":"2p","second":"东","draw":null}"#,
        r#"{"first":"2p","second":"E","draw":null,"event":3}"#,
    ] {
        assert_eq!(
            execute(&evidence, args)["error"]["code"],
            "invalid_arguments",
            "{args}"
        );
    }
    let args = r#"{"first":"2p","second":"E","draw":"4m"}"#;
    let mut invalid = evidence.clone();
    invalid["position"]["players"][0]["riichi"] = json!("accepted");
    assert_eq!(
        execute(&invalid, args)["error"]["code"],
        "unsupported_state"
    );
    invalid = evidence.clone();
    invalid["analysis_status"] = json!("not_analyzed");
    assert_eq!(
        execute(&invalid, args)["error"]["code"],
        "analysis_unavailable"
    );
    invalid = evidence.clone();
    invalid["position"]["phase"]["player"] = json!(1);
    assert_eq!(
        execute(&invalid, args)["error"]["code"],
        "unsupported_state"
    );
    assert_eq!(
        execute(&evidence, r#"{"first":"2p","second":"9m","draw":null}"#)["error"]["code"],
        "candidate_not_provided"
    );
}

fn assessment(result: &Value) -> Value {
    let reference = result["reference"].as_str().unwrap();
    json!({"sections":[{"source":"assessment","text":"两种选择的直接进张相同，这不足以说明整体价值相同。","facts":[
        {"path":format!("{reference}/first/shanten"),"value":result["comparison"]["first"]["shanten"]},
        {"path":format!("{reference}/second/shanten"),"value":result["comparison"]["second"]["shanten"]},
        {"path":format!("{reference}/first/draws"),"value":result["comparison"]["first"]["draws"]},
        {"path":format!("{reference}/second/draws"),"value":result["comparison"]["second"]["draws"]}
    ]}]})
}

#[test]
fn assessment_requires_executed_comparison_and_both_sides() {
    let mut evidence = evidence();
    evidence["comparisons"] = json!({});
    let result = execute(&evidence, r#"{"first":"2p","second":"E","draw":null}"#);
    let reply = assessment(&result);
    assert!(output::render(&reply.to_string(), &evidence).is_err());
    remember(&mut evidence, &result);
    assert!(
        output::render(&reply.to_string(), &evidence)
            .unwrap()
            .starts_with("【判断】")
    );
    let mut bad = reply.clone();
    bad["sections"][0]["facts"] = json!([reply["sections"][0]["facts"][0]]);
    assert!(output::render(&bad.to_string(), &evidence).is_err());
    bad = reply.clone();
    bad["sections"][0]["facts"][0]["value"] = json!(-1);
    assert!(output::render(&bad.to_string(), &evidence).is_err());
}

#[test]
fn successful_comparison_is_citable_in_current_answer_and_followup() {
    fn call(id: &str, name: &str, arguments: &str) -> Value {
        json!({"type":"function_call","status":"completed","call_id":id,"name":name,"arguments":arguments})
    }
    fn raw_message(text: &str) -> Value {
        json!({"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":text}]})
    }
    fn response(output: Vec<Value>) -> Value {
        json!({"status":"completed","output":output})
    }
    let evidence = evidence();
    let args = r#"{"first":"2p","second":"E","draw":null}"#;
    let result = execute(&evidence, args);
    let reply = assessment(&result).to_string();
    let mut step = 0;
    let (text, history) = answer(&evidence, &[], false, "为什么切 2p？", |_, forced| {
        step += 1;
        Ok(response(match step {
            1 => {
                assert!(forced);
                vec![call("review", "get_review", "{}")]
            }
            2 => {
                assert!(!forced);
                vec![call("compare", "compare_discards", args)]
            }
            _ => vec![raw_message(&reply)],
        }))
    })
    .unwrap();
    assert!(text.starts_with("【判断】"));
    assert_eq!(step, 3);
    assert!(
        answer(&evidence, &history, true, "再说一下", |_, _| Ok(
            response(vec![raw_message(&reply)])
        ))
        .is_ok()
    );
}

#[test]
fn hypothetical_results_cannot_render_the_original_draw_list() {
    let mut evidence = evidence();
    evidence["comparisons"] = json!({});
    let result = execute(&evidence, r#"{"first":"2p","second":"E","draw":null}"#);
    remember(&mut evidence, &result);
    let mut reply = assessment(&result);
    reply["sections"][0]["source"] = json!("calculation");
    reply["sections"][0]["draws_for"] = json!("2p");
    assert!(
        output::render(&reply.to_string(), &evidence)
            .unwrap_err()
            .contains("draws_for")
    );
}

#[test]
fn called_discard_is_counted_only_in_the_public_meld() {
    let mut evidence = evidence();
    evidence["position"]["players"][1]["melds"] = json!([{
        "kind":"pon", "tiles":["E","E","E"], "called":"E", "from":2
    }]);
    evidence["position"]["players"][2]["discards"] = json!([{
        "tile":"E", "called":true, "tsumogiri":false, "riichi":false
    }]);
    let args = r#"{"first":"2p","second":"E","draw":null}"#;
    assert_eq!(execute(&evidence, args)["ok"], true);
    assert_eq!(
        execute(&evidence, r#"{"first":"2p","second":"E","draw":"E"}"#)["error"]["code"],
        "exhausted_draw"
    );
    evidence["position"]["players"][2]["discards"][0]["called"] = json!(false);
    assert_eq!(
        execute(&evidence, args)["error"]["code"],
        "invalid_position"
    );
}
