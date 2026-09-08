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

fn run(evidence: &Value, args: Value) -> Value {
    let result = super::super::execute("compare_discard_facts", evidence, &args.to_string());
    assert_eq!(result["ok"], true, "{result}");
    result["analysis"].clone()
}

#[test]
fn symmetric_comparison_keeps_routes_structure_and_public_context_together() {
    let evidence = evidence("3m 7m 8m 1p 1p 2p 4p 5p 7s 8s 9s E C C");
    let forward = run(&evidence, json!({"first":"2p","second":"3m","draw":null}));
    let reverse = run(&evidence, json!({"first":"3m","second":"2p","draw":null}));
    assert_eq!(forward["first"], reverse["second"]);
    assert_eq!(forward["second"], reverse["first"]);
    assert_eq!(forward["context"], reverse["context"]);
    assert_eq!(forward["difference"]["direct_unseen_first_minus_second"], 0);
    assert_eq!(
        forward["first"]["hand"]["routes"]
            .as_object()
            .unwrap()
            .len(),
        8
    );
    assert!(
        forward["first"]["structure"]["components"]
            .as_array()
            .unwrap()
            .len()
            > 3
    );
    assert_eq!(
        forward["context"]["self_draws_after_this_discard_if_no_calls_kans_or_early_end"],
        15
    );
    assert_eq!(forward["context"]["history_available"], false);
}

#[test]
fn explicit_followup_keeps_non_best_discards_and_remembers_the_first_discard_for_furiten() {
    let evidence = evidence("1m 2m 3m 4m 5m 6m 7p 8p 9p 2s 3s 5p 5p E");
    let result = run(&evidence, json!({"first":"5p","second":"E","draw":"1s"}));
    let next = &result["first"]["followup"]["next_discards"];
    assert!(
        next["E"]["hand"]["discard_furiten"]["blocked"]
            .as_bool()
            .unwrap()
    );
    assert_eq!(
        next["E"]["hand"]["discard_furiten"]["intersecting_waits"],
        json!(["5p"])
    );
    assert!(next.get("1m").is_some());
    assert!(
        next["1m"]["hand"]["shanten"].as_i64().unwrap()
            > next["E"]["hand"]["shanten"].as_i64().unwrap()
    );
    assert!(next["1m"]["hand"]["routes"].is_object());
    assert_eq!(
        result["second"]["followup"]["completed_shape_before_discard"],
        true
    );
}

#[test]
fn one_shanten_facts_show_each_reachable_wait_instead_of_assigning_a_quality_label() {
    let evidence = evidence("1m 2m 3m 4m 5m 6m 7p 8p 9p 2s 3s 5p 5p E");
    let result = run(&evidence, json!({"first":"5p","second":"E","draw":null}));
    let ready = &result["first"]["one_shanten_to_tenpai"]["1s"]["next_discards"]["E"]["hand"];
    assert_eq!(ready["shanten"], 0);
    assert!(ready["waits"]["5p"].is_object());
    assert_eq!(ready["discard_furiten"]["blocked"], true);
}

#[test]
fn feeding_checks_seats_public_constraints_and_known_tile_supply() {
    let evidence = evidence("3m 7m 8m 1p 1p 2p 4p 5p 7s 8s 9s E C C");
    let result = run(&evidence, json!({"first":"3m","second":"E","draw":null}));
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
    let mut declared = evidence.clone();
    declared["position"]["players"][1]["riichi"] = json!("accepted");
    let result = run(&declared, json!({"first":"3m","second":"E","draw":null}));
    assert!(result["first"]["possible_calls_by_rules_and_known_counts"]["opponents"]["1"]["chi_consumed_kind_combinations"].as_array().unwrap().is_empty());
}

#[test]
fn visible_value_and_walls_keep_their_specific_conditions() {
    let mut evidence = evidence("3m 7m 8m 1p 1p 2p 4p 5pr 7s 8s 9s E C C");
    evidence["position"]["dora_indicators"] = json!(["4p"]);
    evidence["position"]["players"][1]["discards"] = json!(
        (0..4)
            .map(|_| json!({"tile":"2m","called":false}))
            .collect::<Vec<_>>()
    );
    let result = run(&evidence, json!({"first":"5pr","second":"3m","draw":null}));
    assert_eq!(result["first"]["hand"]["known_bonus"]["aka_dora"], 0);
    assert_eq!(result["second"]["hand"]["known_bonus"]["aka_dora"], 1);
    assert_eq!(result["second"]["hand"]["known_bonus"]["dora"], 1);
    assert_eq!(
        result["second"]["hand"]["yakuhai_tiles"]["E"]["roles"],
        json!(["round_wind", "seat_wind"])
    );
    let waits = result["second"]["discard_safety"]["1"]["sequence_wait_shapes"]
        .as_array()
        .unwrap();
    assert_eq!(waits.len(), 3);
    assert_eq!(
        waits
            .iter()
            .filter(|wait| wait["ruled_out_by_visible_counts"] == true)
            .count(),
        2
    );
    assert_eq!(
        result["second"]["discard_safety"]["1"]["known_safe_against_ron_from_this_player"],
        false
    );
}

#[test]
fn dora_names_follow_number_wind_and_dragon_cycles_and_fold_red_indicators() {
    let mut evidence = evidence("1m 2m 3m 4m 5m 6m 7p 8p 9p 2s 3s 5p 5p E");
    evidence["position"]["dora_indicators"] = json!(["9m", "N", "C", "5pr"]);
    let result = super::super::execute("analyze_hand", &evidence, r#"{"discard":"3s"}"#);
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(
        result["analysis"]["known_bonus"]["dora_tiles"],
        json!(["1m", "E", "P", "6p"])
    );
    assert_eq!(result["analysis"]["known_bonus"]["dora"], 2);
    assert_eq!(result["analysis"]["known_bonus"]["aka_dora"], 0);
}

#[test]
fn declared_riichi_payment_is_net_of_the_deposit_exactly_once() {
    let mut evidence = evidence("1m 2m 3m 4m 5m 6m 7p 8p 9p 2s 3s 5p 5pr E");
    evidence["position"]["honba"] = json!(2);
    evidence["position"]["riichi_sticks"] = json!(1);
    let result = super::super::execute("analyze_hand", &evidence, r#"{"discard":"E"}"#);
    assert_eq!(result["ok"], true, "{result}");
    let values = &result["analysis"]["waits"]["1s"]["scenarios"]["declare_riichi"]["ron"]["best_interpretations"];
    for value in values.as_array().unwrap() {
        let payment = value["payments"]["amount"].as_i64().unwrap();
        let settlements = &value["conditional_settlements"];
        assert_eq!(settlements["new_riichi_deposit"], 1000);
        let outcome = &settlements["by_payer"]["ron_from_1"];
        assert_eq!(outcome["deltas"][0].as_i64().unwrap(), payment + 600 + 1000);
        assert_eq!(
            outcome["scores"][0].as_i64().unwrap(),
            25000 + payment + 600 + 1000
        );
        assert_eq!(
            outcome["deltas"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_i64().unwrap())
                .sum::<i64>(),
            1000
        );
    }
}

#[test]
fn draw_conditions_and_opponent_win_conditions_do_not_guess_hidden_hands() {
    let mut evidence = evidence("3m 7m 8m 1p 1p 2p 4p 5p 7s 8s 9s E C C");
    evidence["position"]["honba"] = json!(2);
    evidence["position"]["riichi_sticks"] = json!(1);
    let draws = super::super::execute("analyze_draw_outcomes", &evidence, "{}");
    assert_eq!(draws["ok"], true, "{draws}");
    assert_eq!(
        draws["analysis"]["combinations"].as_array().unwrap().len(),
        16
    );
    for case in draws["analysis"]["combinations"].as_array().unwrap() {
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
    let win = super::super::execute(
        "analyze_win_outcome",
        &evidence,
        r#"{"winner":1,"payer":0,"fu":30,"han":3}"#,
    );
    assert_eq!(win["ok"], true, "{win}");
    assert_eq!(
        win["analysis"]["outcome"]["deltas"],
        json!([-4500, 5500, 0, 0])
    );
    assert_eq!(
        super::super::execute(
            "analyze_win_outcome",
            &evidence,
            r#"{"winner":1,"payer":1,"fu":30,"han":3}"#
        )["error"]["code"],
        "invalid_score_scenario"
    );
}

#[test]
#[ignore = "使用 release 模式手动测量完整工具调用"]
fn benchmark_fact_comparisons() {
    for (tiles, args) in [
        (
            "3m 7m 8m 1p 1p 2p 4p 5p 7s 8s 9s E C C",
            json!({"first":"2p","second":"3m","draw":null}),
        ),
        (
            "1m 2m 3m 4m 5m 6m 7p 8p 9p 2s 3s 5p 5p E",
            json!({"first":"5p","second":"E","draw":null}),
        ),
        (
            "1m 2m 3m 4m 5m 6m 7p 8p 9p 2s 3s 5p 5p E",
            json!({"first":"5p","second":"E","draw":"1s"}),
        ),
    ] {
        let evidence = evidence(tiles);
        let mut times = Vec::new();
        for _ in 0..5 {
            let start = std::time::Instant::now();
            let result = run(&evidence, args.clone());
            times.push(start.elapsed().as_secs_f64() * 1000.0);
            assert!(serde_json::to_vec(&result).unwrap().len() < 256 * 1024);
        }
        times.sort_by(f64::total_cmp);
        eprintln!("facts {args}: median {:.2} ms", times[2]);
    }
}

#[test]
#[ignore = "使用 release 模式手动测量复杂役种路线"]
fn benchmark_yaku_progression() {
    let evidence = evidence("3m 7m 8m 1p 1p 2p 4p 5p 7s 8s 9s E C C");
    for yaku in ["honitsu", "iipeikou", "ryanpeikou"] {
        let start = std::time::Instant::now();
        let result = super::super::execute(
            "analyze_yaku_route",
            &evidence,
            &json!({"discard":"3m","yaku":yaku}).to_string(),
        );
        assert_eq!(result["ok"], true, "{result}");
        eprintln!(
            "route {yaku}: {:.2} ms",
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
}
