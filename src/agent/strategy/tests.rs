use super::*;
use crate::{
    agent::{AgentContext, comparison},
    mahjong::player_index::PlayerIndex,
};

fn position(tiles: &str) -> Value {
    let log = convlog::tenhou::Log::from_json_str(include_str!(
        "../../../fixtures/tenhou/ranked_game.json"
    ))
    .unwrap();
    let events = convlog::tenhou_to_mjai(&log).unwrap();
    let mut evidence = AgentContext::from_events(&events, PlayerIndex::new(0).unwrap(), 2)
        .unwrap()
        .evidence()
        .clone();
    evidence["position"]["concealed"] = json!(tiles.split_whitespace().collect::<Vec<_>>());
    evidence["position"]["dora_indicators"] = json!([]);
    evidence["position"]["history"] = Value::Null;
    evidence["discards"] = json!(
        tiles
            .split_whitespace()
            .map(|t| json!({"discard":t}))
            .collect::<Vec<_>>()
    );
    evidence["analysis_status"] = json!("available");
    evidence
}

fn run(name: &str, evidence: &Value, args: Value) -> Value {
    let result = execute(name, evidence, &args.to_string());
    assert_eq!(result["ok"], true, "{name}: {result}");
    result["analysis"].clone()
}

#[test]
fn all_improvements_cover_every_available_kind_and_preserve_counterexamples() {
    let evidence = position("3m 7m 8m 1p 1p 2p 4p 5p 7s 8s 9s E C C");
    let result = comparison::execute_all(&evidence, r#"{"first":"2p","second":"3m"}"#);
    assert_eq!(result["ok"], true, "{result}");
    let c = &result["comparison"];
    assert_eq!(c["coverage"]["total_unseen"], 122);
    assert_eq!(c["coverage"]["by_draw"].as_object().unwrap().len(), 34);
    assert_eq!(c["first"]["improvements"]["4m"]["best_unseen"], 28);
    assert_eq!(c["second"]["improvements"]["4m"]["best_unseen"], 20);
    assert_eq!(
        c["coverage"]["by_draw"]["4m"]["favored_by_direct_efficiency"],
        "first"
    );
    assert_eq!(c["coverage"]["is_probability"], false);
    assert_eq!(
        c["initial_discard_difference"]["unseen_first_minus_second"],
        0
    );
    assert!(c.get("difference").is_none());
    let detailed = comparison::execute(&evidence, r#"{"first":"2p","second":"3m","draw":"4m"}"#);
    for side in ["first", "second"] {
        let summary = &c[side]["improvements"]["4m"];
        let full = &detailed["comparison"][side]["followup"];
        assert_eq!(summary["best_shanten"], full["best_shanten_after_discard"]);
        assert_eq!(
            summary["best_discard_names"],
            full["best_discards_by_direct_efficiency"]
        );
        for draw in c[side]["improvements"].as_object().unwrap().values() {
            assert_eq!(draw.as_object().unwrap().len(), 6);
            assert!(draw.get("best_discards").is_none());
            assert!(draw.get("next_discards").is_none());
        }
    }
    // 固定样本的摘要应保持小于20 KiB，防止重新塞入完整进张列表。
    assert!(serde_json::to_vec(&result).unwrap().len() < 20 * 1024);
    let sum: u64 = [
        "first_favored_unseen",
        "second_favored_unseen",
        "equal_metrics_unseen",
    ]
    .iter()
    .map(|k| c["coverage"][k].as_u64().unwrap())
    .sum();
    assert_eq!(sum, 122);
    let mut exhausted = evidence.clone();
    exhausted["position"]["players"][1]["discards"] =
        json!(vec![json!({"tile":"4m","called":false}); 4]);
    assert_eq!(
        comparison::execute(&exhausted, r#"{"first":"2p","second":"3m","draw":"4m"}"#)["error"]["code"],
        "exhausted_draw"
    );
}

#[test]
fn conditional_scoring_compares_dama_riichi_and_red_bonus_without_ura() {
    let mut evidence = position("1m 2m 3m 4m 5m 6m 7p 8p 9p 2s 3s 5p 5pr E");
    // 此处不是首巡，避免把普通立直样例变成两立直。
    evidence["position"]["players"][0]["discards"] = json!([{"tile":"N","called":false}]);
    let report = run("analyze_waits", &evidence, json!({"discard":"E"}));
    assert_eq!(report["shanten"], 0);
    assert_eq!(report["can_declare_riichi_under_current_conditions"], true);
    let wait = &report["waits"]["1s"]["scenarios"];
    let dama = &wait["dama"]["ron"]["best_interpretations"][0];
    assert_eq!(dama["fu"], 30);
    assert_eq!(dama["total_han"], 2);
    assert_eq!(dama["payments"]["amount"], 2900);
    assert_eq!(dama["bonus"]["aka_dora"], 1);
    assert_eq!(dama["bonus"]["ura_dora"], 0);
    assert_eq!(dama["wait_type"], "ryanmen");
    assert_eq!(
        wait["declare_riichi"]["ron"]["best_interpretations"][0]["payments"]["amount"],
        5800
    );
    assert_eq!(wait["dama"]["tsumo"]["best_interpretations"][0]["fu"], 20);
}

#[test]
fn riichi_history_distinguishes_double_riichi_and_pending_acceptance() {
    let mut evidence = position("1m 2m 3m 4m 5m 6m 7p 8p 9p 2s 3s 5p 5p E");
    evidence["position"]["history"] =
        json!([{"event_index":2,"player":0,"kind":"draw","tile":null}]);
    let report = run("analyze_waits", &evidence, json!({"discard":"E"}));
    let yaku = report["waits"]["1s"]["scenarios"]["declare_riichi"]["ron"]["best_interpretations"]
        [0]["yaku"]
        .as_array()
        .unwrap();
    assert!(yaku.contains(&json!("DoubleRiichi")));
    assert!(!yaku.contains(&json!("Ippatsu")));
    evidence["event_index"] = json!(3);
    evidence["position"]["players"][0]["riichi"] = json!("declared");
    evidence["position"]["history"]
        .as_array_mut()
        .unwrap()
        .push(json!({"event_index":3,"player":0,"kind":"riichi_declared","tile":null}));
    let report = run("analyze_waits", &evidence, json!({"discard":"E"}));
    assert_eq!(report["scope"]["assumes_pending_riichi_is_accepted"], true);
    assert!(
        report["waits"]["1s"]["scenarios"]
            .get("established_riichi")
            .is_none()
    );
    assert_eq!(
        report["waits"]["1s"]["scenarios"]["pending_riichi_if_accepted"]["ron"]["has_yaku"],
        true
    );
}

#[test]
fn own_discard_furiten_blocks_the_whole_wait_including_called_discards() {
    let mut evidence = position("1m 2m 3m 4m 5m 6m 7p 8p 9p 2s 3s 5p 5p");
    evidence["position"]["players"][0]["discards"] = json!([{"tile":"1s","called":true}]);
    evidence["position"]["players"][1]["melds"] =
        json!([{"kind":"chi","tiles":["1s","2s","3s"],"called":"1s","from":0}]);
    evidence["position"]["players"][2]["discards"] = json!([
        {"tile":"4s","called":false},{"tile":"4s","called":false},{"tile":"4s","called":false},{"tile":"4s","called":false}
    ]);
    let report = run("analyze_waits", &evidence, json!({"discard":null}));
    assert_eq!(report["discard_furiten"]["blocked"], true);
    assert_eq!(
        report["discard_furiten"]["intersecting_waits"],
        json!(["1s"])
    );
    assert_eq!(report["waits"]["4s"]["unseen"], 0);
    for tile in ["1s", "4s"] {
        let methods = &report["waits"][tile]["scenarios"]["without_riichi"];
        assert_eq!(methods["ron"]["blocked_by_discard_furiten"], true);
        assert_eq!(methods["tsumo"]["blocked_by_discard_furiten"], false);
        assert_eq!(methods["ron"]["has_yaku"], true);
    }
}

#[test]
fn open_no_yaku_shape_does_not_gain_a_win_from_dora() {
    let mut evidence = position("4m 5m 6m 7p 8p 9p 2s 3s 5p 5p");
    evidence["position"]["players"][0]["melds"] =
        json!([{"kind":"chi","tiles":["1m","2m","3m"],"called":"1m","from":3}]);
    evidence["position"]["dora_indicators"] = json!(["4p"]);
    let report = run("analyze_waits", &evidence, json!({"discard":null}));
    assert_eq!(report["shanten"], 0);
    assert_eq!(report["can_declare_riichi_under_current_conditions"], false);
    assert_eq!(
        run(
            "analyze_yaku_route",
            &evidence,
            json!({"discard":null,"yaku":"chiitoitsu"})
        )["reachable"],
        false
    );
    for method in ["ron", "tsumo"] {
        assert_eq!(
            report["waits"]["1s"]["scenarios"]["without_riichi"][method]["has_yaku"],
            false
        );
    }
    assert_eq!(
        run(
            "analyze_yaku_route",
            &evidence,
            json!({"discard":null,"yaku":"tanyao"})
        )["reachable"],
        false
    );
    assert_eq!(
        execute(
            "analyze_yaku_route",
            &evidence,
            r#"{"discard":null,"yaku":"pinfu"}"#
        )["error"]["code"],
        "unsupported_yaku"
    );
}

#[test]
fn defense_uses_opponent_specific_evidence_and_excludes_open_ron_window() {
    let mut evidence = position("1m 2m 3m 4m 5m 6m 7m 8m 9m 1p 2p 3p E E");
    evidence["event_index"] = json!(6);
    evidence["position"]["phase"] = json!({"kind":"after_discard","player":3});
    evidence["position"]["players"][1]["riichi"] = json!("accepted");
    evidence["position"]["players"][1]["discards"] = json!([{"tile":"N","called":false}]);
    evidence["position"]["players"][2]["discards"] = json!([{"tile":"4m","called":false}]);
    evidence["position"]["players"][3]["discards"] = json!([{"tile":"7m","called":false}]);
    evidence["position"]["history"] = json!([
        {"event_index":0,"player":1,"kind":"riichi_declared","tile":null},
        {"event_index":1,"player":1,"kind":"discard","tile":"N"},
        {"event_index":2,"player":1,"kind":"riichi_accepted","tile":null},
        {"event_index":3,"player":2,"kind":"draw","tile":null},
        {"event_index":4,"player":2,"kind":"discard","tile":"4m"},
        {"event_index":5,"player":3,"kind":"draw","tile":null},
        {"event_index":6,"player":3,"kind":"discard","tile":"7m"}
    ]);
    let report = run("analyze_defense", &evidence, json!({}));
    assert_eq!(
        report["opponents"]["1"]["tiles"]["4m"]["known_safe_against_ron_from_this_player"],
        true
    );
    assert_eq!(
        report["opponents"]["1"]["tiles"]["7m"]["known_safe_against_ron_from_this_player"],
        false
    );
    assert_eq!(
        report["opponents"]["1"]["tiles"]["7m"]["suji_sources"],
        json!(["4m"])
    );
    assert_eq!(
        report["opponents"]["3"]["tiles"]["7m"]["in_opponent_river"],
        true
    );
    assert_eq!(
        report["opponents"]["2"]["tiles"]["4m"]["in_opponent_river"],
        true
    );
    let mut missing = evidence.clone();
    missing["position"]["history"] = Value::Null;
    assert_eq!(
        run("analyze_defense", &missing, json!({}))["opponents"]["1"]["tiles"]["4m"]["known_safe_against_ron_from_this_player"],
        false
    );
    let mut polluted = evidence.clone();
    polluted["actual_future"] = json!(["4m"]);
    polluted["position"]["players"][1]["concealed"] = json!(["4m"]);
    assert_eq!(run("analyze_defense", &polluted, json!({})), report);
    polluted["position"]["history"][6]["tile"] = json!("8m");
    assert_eq!(
        execute("analyze_defense", &polluted, "{}")["error"]["code"],
        "invalid_position"
    );
}

#[test]
fn middle_suji_requires_both_endpoints_and_never_becomes_a_safety_guarantee() {
    let mut evidence = position("1m 2m 3m 4m 5m 6m 7p 8p 9p 2s 3s 5p 5p");
    evidence["position"]["players"][1]["discards"] = json!([{"tile":"1m","called":false}]);
    let partial = run("analyze_defense", &evidence, json!({}));
    let tile = &partial["opponents"]["1"]["tiles"]["4m"];
    assert_eq!(tile["suji_covers_both_possible_ryanmen_sides"], false);
    assert_eq!(tile["missing_suji_endpoints"], json!(["7m"]));
    evidence["position"]["players"][1]["discards"]
        .as_array_mut()
        .unwrap()
        .push(json!({"tile":"7m","called":false}));
    let complete = run("analyze_defense", &evidence, json!({}));
    let tile = &complete["opponents"]["1"]["tiles"]["4m"];
    assert_eq!(tile["suji_covers_both_possible_ryanmen_sides"], true);
    assert_eq!(tile["known_safe_against_ron_from_this_player"], false);
}

fn offer(evidence: &mut Value, names: &[&str]) {
    evidence["mortal"] = json!({"status":"available","decision":{"candidates":names.iter().map(|kind|json!({"action":{"kind":kind},"q_value":0.0})).collect::<Vec<_>>(),"kan_candidates":[],"recommended":{"type":"none"}}});
}

fn winning_pass_position(riichi: bool) -> Value {
    let mut evidence = position("1m 2m 3m 4m 5m 6m 7p 8p 9p 2s 3s 5p 5p");
    evidence["position"]["phase"] = json!({"kind":"after_discard","player":3});
    evidence["position"]["players"][3]["discards"] = json!([{"tile":"1s","called":false}]);
    evidence["position"]["players"][0]["riichi"] =
        json!(if riichi { "accepted" } else { "not_declared" });
    offer(&mut evidence, &["win", "pass"]);
    evidence["mortal"]["decision"]["at_furiten"] = json!(false);
    evidence
}

fn assert_pass_blocks_all_ron_but_keeps_tsumo(report: &Value) {
    assert_eq!(report["passes_current_winning_tile"], true);
    assert_eq!(report["furiten_before_pass"], false);
    assert_eq!(report["ron_blocked_by_furiten_after_pass"], true);
    assert_eq!(report["waits"].as_object().unwrap().len(), 2);
    for wait in report["waits"].as_object().unwrap().values() {
        for scenario in wait["scenarios"].as_object().unwrap().values() {
            assert_eq!(scenario["ron"]["blocked_by_furiten_after_pass"], true);
            assert_eq!(scenario["tsumo"]["blocked_by_furiten_after_pass"], false);
            assert_eq!(scenario["tsumo"]["has_yaku"], true);
        }
    }
}

#[test]
fn passing_ron_before_riichi_causes_temporary_furiten_for_the_whole_wait() {
    let mut evidence = winning_pass_position(false);
    for phase in ["after_discard", "after_kan_declaration"] {
        evidence["position"]["phase"] = json!({"kind":phase,"player":3,"kan_kind":"kakan"});
        let report = run("analyze_actions", &evidence, json!({}));
        let pass = &report["pass"];
        assert_pass_blocks_all_ron_but_keeps_tsumo(pass);
        assert_eq!(pass["temporary_furiten_after_pass"], true);
        assert_eq!(pass["riichi_furiten_after_pass"], false);
    }
}

#[test]
fn passing_ron_after_riichi_keeps_furiten_until_the_end_of_the_hand() {
    let report = run("analyze_actions", &winning_pass_position(true), json!({}));
    let pass = &report["pass"];
    assert_pass_blocks_all_ron_but_keeps_tsumo(pass);
    assert_eq!(pass["temporary_furiten_after_pass"], false);
    assert_eq!(pass["riichi_furiten_after_pass"], true);
}

#[test]
fn passing_without_ron_does_not_clear_known_or_unknown_existing_furiten() {
    let mut evidence = winning_pass_position(false);
    evidence["position"]["players"][3]["discards"][0]["tile"] = json!("2m");
    offer(&mut evidence, &["pass"]);
    for prior in [json!(false), json!(true), Value::Null] {
        evidence["mortal"]["decision"]["at_furiten"] = prior.clone();
        let report = run("analyze_actions", &evidence, json!({}));
        assert_eq!(report["pass"]["passes_current_winning_tile"], false);
        assert_eq!(report["pass"]["passed_tile_completes_shape"], false);
        assert_eq!(report["pass"]["ron_blocked_by_furiten_after_pass"], prior);
    }
    evidence["position"]["players"][0]["discards"] = json!([{"tile":"4s","called":false}]);
    let report = run("analyze_actions", &evidence, json!({}));
    assert_eq!(report["pass"]["furiten_before_pass"], true);
    assert_eq!(report["pass"]["ron_blocked_by_furiten_after_pass"], true);
    assert!(report["pass"]["temporary_furiten_after_pass"].is_null());
}

#[test]
fn no_yaku_wait_is_not_mistaken_for_a_furiten_free_pass() {
    let mut evidence = position("4m 5m 6m 7p 8p 9p 2s 3s 5p 5p");
    evidence["position"]["players"][0]["melds"] =
        json!([{"kind":"chi","tiles":["1m","2m","3m"],"called":"1m","from":3}]);
    evidence["position"]["phase"] = json!({"kind":"after_discard","player":3});
    evidence["position"]["players"][3]["discards"] = json!([{"tile":"1s","called":false}]);
    offer(&mut evidence, &["pass"]);
    evidence["mortal"]["decision"]["at_furiten"] = json!(true);
    let report = run("analyze_actions", &evidence, json!({}));
    let pass = &report["pass"];
    assert_eq!(pass["passes_current_winning_tile"], false);
    assert_eq!(pass["passed_tile_completes_shape"], true);
    assert_eq!(pass["temporary_furiten_after_pass"], true);
    assert_eq!(pass["ron_blocked_by_furiten_after_pass"], true);
    assert_eq!(
        pass["waits"]["1s"]["scenarios"]["without_riichi"]["ron"]["has_yaku"],
        false
    );
}

fn action_detail(
    evidence: &Value,
    action: &str,
    variant: Option<&str>,
    discard: Option<&str>,
    draw: Option<&str>,
) -> Value {
    run(
        "analyze_action_details",
        evidence,
        json!({"action":action,"variant":variant,"discard":discard,"draw":draw}),
    )["result"]
        .clone()
}

#[test]
fn chi_summary_and_selected_detail_preserve_kuikae_and_safe_inventory() {
    let mut evidence = position("2m 3m 4m 5m 6m 7m 1p 2p 3p 5p 5pr 7s 8s");
    evidence["position"]["phase"] = json!({"kind":"after_discard","player":3});
    evidence["position"]["players"][3]["discards"] = json!([{"tile":"2m","called":false}]);
    evidence["position"]["players"][1]["discards"] = json!([{"tile":"7s","called":false}]);
    offer(&mut evidence, &["chi_low", "pass"]);
    evidence["mortal"]["decision"]["at_furiten"] = json!(false);
    let report = run("analyze_actions", &evidence, json!({}));
    let branch = &report["chi_low"]["variants"]["3m_4m"];
    assert_eq!(branch["consumed"], json!(["3m", "4m"]));
    assert_eq!(branch["forbidden_kuikae_discards"], json!(["2m", "5m"]));
    assert!(branch["next_discards"].get("2m").is_none());
    assert!(branch["next_discards"].get("5m").is_none());
    assert!(branch["next_discards"]["7s"].get("waits").is_none());
    assert!(
        branch["next_discards"]["7s"]
            .get("safe_inventory")
            .is_none()
    );
    let cut = action_detail(&evidence, "chi_low", Some("3m_4m"), Some("7s"), None);
    assert_eq!(cut["closed"], false);
    assert_eq!(cut["safe_inventory"]["by_player"]["1"]["copies"], 0);
    let kept = action_detail(&evidence, "chi_low", Some("3m_4m"), Some("8s"), None);
    assert_eq!(
        kept["safe_inventory"]["by_player"]["1"]["tiles"],
        json!(["7s"])
    );
    let invalid = execute(
        "analyze_action_details",
        &evidence,
        r#"{"action":"chi_low","variant":"3m_4m","discard":"5m","draw":null}"#,
    );
    assert_eq!(invalid["ok"], false);
    assert_eq!(report["pass"]["passes_current_winning_tile"], false);
}

#[test]
fn selected_pon_keeps_structure_and_each_opponents_safe_inventory() {
    let mut evidence = position("5mr 5m 7m 8p 1s 3s 4s 5s 7s W W P P");
    evidence["position"]["phase"] = json!({"kind":"after_discard","player":3});
    evidence["position"]["players"][1]["melds"] =
        json!([{"kind":"daiminkan","tiles":["C","C","C","C"],"called":"C","from":3}]);
    evidence["position"]["players"][1]["discards"] = json!([{"tile":"1s","called":false}]);
    evidence["position"]["players"][3]["discards"] = json!([{"tile":"P","called":false}]);
    offer(&mut evidence, &["pon", "pass"]);
    let pass = action_detail(&evidence, "pass", None, None, None);
    assert_eq!(pass["shanten"], 3);
    assert_eq!(pass["concealed_after"].as_array().unwrap().len(), 13);
    assert_eq!(pass["closed"], true);
    assert_eq!(pass["structure"]["isolated_kinds"], json!(["8p"]));
    let cut = action_detail(&evidence, "pon", Some("P_P"), Some("1s"), None);
    assert_eq!(cut["safe_inventory"]["by_player"]["1"]["copies"], 0);
    let kept = action_detail(&evidence, "pon", Some("P_P"), Some("4s"), None);
    assert_eq!(kept["shanten"], 2);
    assert_eq!(
        kept["safe_inventory"]["by_player"]["1"]["tiles"],
        json!(["1s"])
    );
    assert_eq!(kept["safe_inventory"]["by_player"]["3"]["copies"], 0);
    let defense = run("analyze_defense", &evidence, json!({}));
    assert_eq!(
        defense["opponents"]["1"]["riichi_blocked_by_open_melds"],
        true
    );
    evidence["position"]["players"][1]["melds"] =
        json!([{"kind":"ankan","tiles":["C","C","C","C"],"called":null,"from":null}]);
    assert_eq!(
        run("analyze_defense", &evidence, json!({}))["opponents"]["1"]["riichi_blocked_by_open_melds"],
        false
    );
}

#[test]
fn pon_preserves_red_consumption_variants_and_rejects_post_call_riichi() {
    let mut evidence = position("5p 5p 5pr 1m 2m 3m 4m 5m 6m 7s 8s 9s E");
    evidence["position"]["phase"] = json!({"kind":"after_discard","player":2});
    evidence["position"]["players"][2]["discards"] = json!([{"tile":"5p","called":false}]);
    offer(&mut evidence, &["pon", "pass"]);
    let report = run("analyze_actions", &evidence, json!({}));
    let variants = &report["pon"]["variants"];
    assert_eq!(variants.as_object().unwrap().len(), 2);
    assert_eq!(
        variants["5p_5p"]["forbidden_kuikae_discards"],
        json!(["5pr"])
    );
    assert_eq!(
        variants["5pr_5p"]["forbidden_kuikae_discards"],
        json!(["5p"])
    );
    for variant in ["5p_5p", "5pr_5p"] {
        let hand = action_detail(&evidence, "pon", Some(variant), Some("E"), None);
        assert_eq!(hand["can_declare_riichi_under_current_conditions"], false);
        assert_eq!(hand["known_bonus"]["aka_dora"], 1);
    }
    // 副露与暗牌一起保留赤牌，错误变体不会偷偷回退到其他合法组合。
    assert_eq!(
        execute(
            "analyze_action_details",
            &evidence,
            r#"{"action":"pon","variant":"missing","discard":"E","draw":null}"#
        )["ok"],
        false
    );
}

#[test]
fn kan_summary_does_not_expand_draws_and_selected_draw_preserves_riichi_lock() {
    let mut evidence = position("1m 1m 1m 1m 2m 3m 4m 5p 5p 6p 7p 8p E E");
    evidence["position"]["players"][0]["riichi"] = json!("accepted");
    offer(&mut evidence, &["kan"]);
    evidence["mortal"]["decision"]["kan_candidates"] = json!([{"tile":"1m","q_value":0.1}]);
    let report = run("analyze_actions", &evidence, json!({}));
    let kan = &report["kan"]["variants"]["ankan_1m"];
    assert_eq!(kan["consumed"], json!(["1m", "1m", "1m", "1m"]));
    assert_eq!(kan["closed_after"], true);
    assert!(kan["replacement_draws"].as_object().unwrap().is_empty());
    let detail = action_detail(&evidence, "kan", Some("ankan_1m"), None, Some("E"));
    assert_eq!(detail["replacement_draws"].as_object().unwrap().len(), 1);
    assert_eq!(
        detail["replacement_draws"]["E"]["best_discards"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["E"]
    );
    assert_eq!(
        execute(
            "analyze_action_details",
            &evidence,
            r#"{"action":"kan","variant":"ankan_1m","discard":null,"draw":"1m"}"#
        )["error"]["code"],
        "exhausted_draw"
    );
}

#[test]
fn yaku_distance_does_not_expand_progression_until_requested() {
    let evidence = position("3m 7m 8m 1p 1p 2p 4p 5p 7s 8s 9s E C C");
    let args = json!({"discard":"2p","yaku":"honitsu"});
    let distance = run("analyze_yaku_route", &evidence, args.clone());
    assert_eq!(distance["progression"], Value::Null);
    assert_eq!(distance["scope"]["progression_available"], false);
    let progression = run("analyze_yaku_progression", &evidence, args);
    assert_eq!(
        distance["available_shanten"],
        progression["available_shanten"]
    );
    assert!(progression["progression"].is_array());
}

#[test]
fn score_targets_distinguish_direct_hit_ties_and_table_sticks() {
    let mut evidence = position("1m 2m 3m 4m 5m 6m 7p 8p 9p 2s 3s 5p 5p");
    evidence["position"]["players"][1]["score"] = json!(30000);
    let result = run("analyze_score_targets", &evidence, json!({"target":1}));
    assert_eq!(
        result["ron_by_payer"]["1"]["minimum_ron_payment_excluding_honba"],
        2500
    );
    assert_eq!(
        result["ron_by_payer"]["2"]["minimum_ron_payment_excluding_honba"],
        5000
    );
    evidence["position"]["honba"] = json!(2);
    evidence["position"]["riichi_sticks"] = json!(1);
    let result = run("analyze_score_targets", &evidence, json!({"target":1}));
    assert_eq!(
        result["ron_by_payer"]["1"]["minimum_ron_payment_excluding_honba"],
        1400
    );
    assert_eq!(
        result["ron_by_payer"]["2"]["minimum_ron_payment_excluding_honba"],
        3400
    );
    evidence["player"] = json!(1);
    evidence["position"]["players"][0]["score"] = json!(35000);
    evidence["position"]["honba"] = json!(0);
    evidence["position"]["riichi_sticks"] = json!(0);
    let result = run("analyze_score_targets", &evidence, json!({"target":0}));
    assert_eq!(
        result["ron_by_payer"]["0"]["minimum_ron_payment_excluding_honba"],
        2600
    );
    assert_eq!(result["tie_favors_self"], false);
}

#[test]
fn hand_tool_works_without_mortal_and_invalid_arguments_are_rejected() {
    let mut evidence = position("1m 2m 3m 4m 5m 6m 7p 8p 9p 2s 3s 5p 5p");
    evidence["analysis_status"] = json!("not_analyzed");
    evidence["discards"] = json!([]);
    let result = execute("analyze_hand", &evidence, r#"{"discard":null}"#);
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["analysis"]["shanten"], 0);
    for (name, args) in [
        ("analyze_hand", "{}"),
        ("analyze_hand", r#"{"discard":0}"#),
        ("analyze_defense", r#"{"player":2}"#),
        ("analyze_score_targets", r#"{"target":0}"#),
    ] {
        assert_eq!(execute(name, &evidence, args)["ok"], false, "{name}");
    }
}

#[test]
fn split_tools_can_be_called_in_steps_and_only_final_text_is_returned() {
    let evidence = position("1m 2m 3m 4m 5m 6m 7p 8p 9p 2s 3s 5p 5pr E");
    let definitions = super::super::tool_definitions();
    assert!(
        !definitions
            .iter()
            .any(|tool| tool["name"] == "compare_discard_facts")
    );
    let mut step = 0;
    let (answer, _) = super::super::answer(&evidence, &[], false, "切东以后听什么、多少点？", |input, _| {
        step += 1;
        if step > 1 {
            let output = input.iter().rev().find(|item| item["type"] == "function_call_output").unwrap();
            let result: Value = serde_json::from_str(output["output"].as_str().unwrap()).unwrap();
            assert_eq!(result["ok"],true);
            if step == 2 { assert!(result["analysis"].get("waits").is_none()); }
            if step == 3 { assert!(result["analysis"]["waits"].is_object()); }
        }
        let text = json!({"type":"message","role":"assistant","status":"completed",
            "content":[{"type":"output_text","text":if step == 3 { "听1s、4s，分别比较荣和与自摸打点。" } else { "尚未核验的中间解释" }}]});
        let mut output = vec![text];
        if step < 3 {
            output.push(json!({"type":"function_call","status":"completed","call_id":format!("step-{step}"),
                "name":if step==1 {"analyze_hand"} else {"analyze_waits"},"arguments":"{\"discard\":\"E\"}"}));
        }
        Ok(json!({"status":"completed","output":output}))
    }).unwrap();
    assert_eq!(step, 3);
    assert_eq!(answer, "听1s、4s，分别比较荣和与自摸打点。");
    assert!(!answer.contains("中间解释"));
}
