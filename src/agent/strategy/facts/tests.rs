use super::*;

fn evidence(tiles: &str) -> Value {
    let concealed: Vec<_> = tiles.split_whitespace().collect();
    json!({"player":0,"event_index":2,"analysis_status":"available",
        "discards":concealed.iter().map(|tile|json!({"discard":tile})).collect::<Vec<_>>(),
        "position":{"concealed":concealed,"dora_indicators":[],"remaining_draws":60,"dealer":0,
            "round":{"wind":"E","number":1},"honba":0,"riichi_sticks":0,
            "phase":{"kind":"after_draw","player":0},"history":null,
            "players":(0..4).map(|player|json!({"player":player,"riichi":"not_declared","score":25000,
                "melds":[],"discards":[]})).collect::<Vec<_>>()}})
}

fn run(name: &str, evidence: &Value, args: Value) -> Value {
    let result = super::super::execute(name, evidence, &args.to_string());
    assert_eq!(result["ok"], true, "{result}");
    result["analysis"].clone()
}

#[test]
fn safety_comparison_is_symmetric_and_does_not_expand_unrelated_reports() {
    let evidence = evidence("3m 7m 8m 1p 1p 2p 4p 5p 7s 8s 9s E C C");
    let forward = run(
        "compare_discard_safety",
        &evidence,
        json!({"first":"2p","second":"3m"}),
    );
    let reverse = run(
        "compare_discard_safety",
        &evidence,
        json!({"first":"3m","second":"2p"}),
    );
    assert_eq!(forward["first"], reverse["second"]);
    assert_eq!(forward["second"], reverse["first"]);
    for field in ["hand", "followup", "one_shanten_to_tenpai"] {
        assert!(forward["first"].get(field).is_none());
    }
    assert!(forward.get("context").is_none());
}

#[test]
fn selected_followup_remembers_both_discards_for_furiten() {
    let evidence = evidence("1m 2m 3m 4m 5m 6m 7p 8p 9p 2s 3s 5p 5p E");
    let shape = run("analyze_hand", &evidence, json!({"discard":"5p"}));
    assert_eq!(shape["shanten"], 1);
    for field in ["waits", "routes", "one_shanten_to_tenpai"] {
        assert!(shape.get(field).is_none());
    }
    let ready = run(
        "analyze_discard_followup",
        &evidence,
        json!({"discard":"5p","draw":"1s","next_discard":"E"}),
    );
    assert_eq!(ready["hand"]["shanten"], 0);
    assert_eq!(ready["hand"]["discard_furiten"]["blocked"], true);
    assert_eq!(
        ready["hand"]["discard_furiten"]["intersecting_waits"],
        json!(["5p"])
    );
    assert!(ready["hand"]["waits"]["5p"].is_object());
    assert!(ready.get("next_discards").is_none());
    let slower = run(
        "analyze_discard_followup",
        &evidence,
        json!({"discard":"5p","draw":"1s","next_discard":"1m"}),
    );
    assert!(slower["hand"]["shanten"].as_i64().unwrap() > 0);
    assert!(slower["hand"].get("routes").is_none());
}

#[test]
fn followup_rejects_unavailable_draws_discards_and_locked_positions() {
    let mut evidence = evidence("1m 2m 3m 4m 5m 6m 7p 8p 9p 2s 3s 5p 5p E");
    let args = json!({"discard":"5p","draw":"1s","next_discard":"C"});
    assert_eq!(
        super::super::execute("analyze_discard_followup", &evidence, &args.to_string())["error"]["code"],
        "invalid_discard"
    );
    evidence["position"]["remaining_draws"] = json!(0);
    assert_eq!(
        super::super::execute("analyze_discard_followup", &evidence, &args.to_string())["error"]["code"],
        "unsupported_state"
    );
}

#[test]
fn feeding_checks_seats_riichi_and_honor_constraints() {
    let mut evidence = evidence("3m 7m 8m 1p 1p 2p 4p 5p 7s 8s 9s E C C");
    let args = json!({"first":"3m","second":"E"});
    let result = run("compare_discard_safety", &evidence, args.clone());
    let calls = &result["first"]["possible_calls_by_rules_and_known_counts"]["opponents"];
    assert!(
        !calls["1"]["chi_consumed_kind_combinations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        calls["2"]["chi_consumed_kind_combinations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(result["second"]["possible_calls_by_rules_and_known_counts"]["opponents"]["1"]["chi_consumed_kind_combinations"].as_array().unwrap().is_empty());
    evidence["position"]["players"][1]["riichi"] = json!("accepted");
    let result = run("compare_discard_safety", &evidence, args);
    assert!(result["first"]["possible_calls_by_rules_and_known_counts"]["opponents"]["1"]["chi_consumed_kind_combinations"].as_array().unwrap().is_empty());
}

#[test]
fn shape_keeps_red_bonus_and_indicator_cycles_without_scoring() {
    let mut evidence = evidence("3m 7m 8m 1p 1p 2p 4p 5pr 7s 8s 9s E C C");
    evidence["position"]["dora_indicators"] = json!(["4p"]);
    let kept = run("analyze_hand", &evidence, json!({"discard":"3m"}));
    let cut = run("analyze_hand", &evidence, json!({"discard":"5pr"}));
    assert_eq!(kept["known_bonus"]["aka_dora"], 1);
    assert_eq!(kept["known_bonus"]["dora"], 1);
    assert_eq!(cut["known_bonus"]["aka_dora"], 0);
    evidence["position"]["dora_indicators"] = json!(["9m", "N", "C", "5sr"]);
    let result = run("analyze_hand", &evidence, json!({"discard":"3m"}));
    assert_eq!(
        result["known_bonus"]["dora_tiles"],
        json!(["1m", "E", "P", "6s"])
    );
}

#[test]
fn waits_and_settlement_are_separate_without_losing_honba_or_sticks() {
    let mut evidence = evidence("1m 2m 3m 4m 5m 6m 7p 8p 9p 2s 3s 5p 5pr E");
    evidence["position"]["honba"] = json!(2);
    evidence["position"]["riichi_sticks"] = json!(1);
    let report = run("analyze_waits", &evidence, json!({"discard":"E"}));
    let value =
        &report["waits"]["1s"]["scenarios"]["declare_riichi"]["ron"]["best_interpretations"][0];
    assert!(value.get("conditional_settlements").is_none());
    let outcome = run(
        "analyze_win_outcome",
        &evidence,
        json!({"winner":0,"payer":1,"fu":value["fu"],"han":value["total_han"]}),
    );
    // 新立直棒由自己和牌收回，净变化只包含原供托。
    assert_eq!(
        outcome["outcome"]["deltas"][0].as_i64().unwrap(),
        value["payments"]["amount"].as_i64().unwrap() + 600 + 1000
    );
}

#[test]
fn draw_conditions_and_opponent_win_conditions_do_not_guess_hidden_hands() {
    let mut evidence = evidence("3m 7m 8m 1p 1p 2p 4p 5p 7s 8s 9s E C C");
    evidence["position"]["honba"] = json!(2);
    evidence["position"]["riichi_sticks"] = json!(1);
    let draws = run("analyze_draw_outcomes", &evidence, json!({}));
    assert_eq!(draws["combinations"].as_array().unwrap().len(), 16);
    for case in draws["combinations"].as_array().unwrap() {
        assert_eq!(
            case["outcome"]["deltas"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_i64().unwrap())
                .sum::<i64>(),
            0
        );
        assert_eq!(case["carried_riichi_sticks"], 1);
        assert_eq!(case["next_honba_if_match_continues"], 3);
    }
    let win = run(
        "analyze_win_outcome",
        &evidence,
        json!({"winner":1,"payer":0,"fu":30,"han":3}),
    );
    assert_eq!(win["outcome"]["deltas"], json!([-4500, 5500, 0, 0]));
}

#[test]
fn a_selected_winning_draw_can_be_scored_without_discarding_the_win() {
    let evidence = evidence("1m 2m 3m 4m 5m 6m 7p 8p 9p 2s 3s 5p 5p E");
    let completed = run(
        "analyze_discard_followup",
        &evidence,
        json!({"discard":"E","draw":"1s","next_discard":null}),
    );
    assert_eq!(completed["completion"]["has_yaku"], true);
    assert!(
        !completed["completion"]["best_interpretations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        super::super::execute(
            "analyze_discard_followup",
            &evidence,
            r#"{"discard":"5p","draw":"1s","next_discard":null}"#
        )["error"]["code"],
        "discard_required"
    );
}
