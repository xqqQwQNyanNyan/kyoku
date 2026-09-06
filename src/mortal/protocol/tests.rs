use super::*;

fn player() -> PlayerIndex {
    PlayerIndex::new(0).unwrap()
}

fn response(action: Value, bits: u64, q_values: Value) -> Value {
    let mut action = action;
    action["meta"] = json!({"mask_bits": bits, "q_values": q_values});
    action
}

#[test]
fn compact_values_follow_mask_order_and_keep_red_fives_separate() {
    let raw = response(
        json!({"type":"dahai","actor":0,"pai":"5mr","tsumogiri":false}),
        (1 << 4) | (1 << 34) | (1 << 37),
        json!([-0.8, 0.7, 0.2]),
    );
    let result = decision(&raw.to_string(), player()).unwrap().unwrap();
    assert_eq!(
        result.candidates,
        vec![
            Candidate {
                action: Action::Discard(Tile::new(4).unwrap()),
                q_value: -0.8
            },
            Candidate {
                action: Action::Discard(Tile::new(34).unwrap()),
                q_value: 0.7
            },
            Candidate {
                action: Action::Riichi,
                q_value: 0.2
            },
        ]
    );
}

#[test]
fn pass_is_a_decision_but_no_opportunity_is_not() {
    assert!(
        decision(r#"{"type":"none","meta":{"mask_bits":0}}"#, player())
            .unwrap()
            .is_none()
    );
    let raw = response(
        json!({"type":"none"}),
        (1 << 41) | (1 << 45),
        json!([-0.4, 0.3]),
    );
    let result = decision(&raw.to_string(), player()).unwrap().unwrap();
    assert_eq!(result.recommended, Event::None);
    assert_eq!(result.candidates[1].action, Action::Pass);
}

#[test]
fn kan_selection_keeps_its_own_q_values() {
    let mut raw = response(
        json!({"type":"ankan","actor":0,"consumed":["1m","1m","1m","1m"]}),
        (1 << 0) | (1 << 42),
        json!([0.1, 0.8]),
    );
    raw["meta"]["kan_select"] = json!({"mask_bits": 3, "q_values": [0.6, 0.2]});
    let result = decision(&raw.to_string(), player()).unwrap().unwrap();
    assert_eq!(
        result.candidates[1],
        Candidate {
            action: Action::Kan,
            q_value: 0.8
        }
    );
    assert_eq!(
        result.kan_candidates,
        vec![
            KanCandidate {
                tile: TileKind::new(0).unwrap(),
                q_value: 0.6
            },
            KanCandidate {
                tile: TileKind::new(1).unwrap(),
                q_value: 0.2
            },
        ]
    );
}

#[test]
fn every_main_action_label_is_decoded() {
    let bits = (1u64 << 46) - 1;
    let raw = response(json!({"type":"none"}), bits, json!(vec![0.0; 46]));
    let result = decision(&raw.to_string(), player()).unwrap().unwrap();
    assert_eq!(result.candidates.len(), 46);
    assert_eq!(result.candidates[38].action, Action::ChiLow);
    assert_eq!(result.candidates[39].action, Action::ChiMiddle);
    assert_eq!(result.candidates[40].action, Action::ChiHigh);
    assert_eq!(result.candidates[43].action, Action::Win);
    assert_eq!(result.candidates[44].action, Action::AbortiveDraw);
}

#[test]
fn invalid_external_responses_are_errors() {
    let cases = [
        (
            response(json!({"type":"none"}), 1 << 46, json!([1.0])),
            ProtocolError::InvalidMask {
                bits: 1 << 46,
                action_count: 46,
            },
        ),
        (
            response(json!({"type":"none"}), 1 << 45, json!([])),
            ProtocolError::QValueCount {
                expected: 1,
                actual: 0,
            },
        ),
        (
            response(json!({"type":"none"}), 1 << 45, json!([1.0, 2.0])),
            ProtocolError::QValueCount {
                expected: 1,
                actual: 2,
            },
        ),
        (
            response(json!({"type":"reach","actor":1}), 1 << 37, json!([1.0])),
            ProtocolError::WrongActor {
                expected: 0,
                actual: 1,
            },
        ),
        (
            response(json!({"type":"reach","actor":0}), 1 << 45, json!([1.0])),
            ProtocolError::RecommendationOutsideMask,
        ),
        (
            response(json!({"type":"end_game"}), 1 << 45, json!([1.0])),
            ProtocolError::UnexpectedAction,
        ),
        (
            response(
                json!({"type":"dahai","actor":0,"pai":"?","tsumogiri":false}),
                1,
                json!([1.0]),
            ),
            ProtocolError::UnexpectedAction,
        ),
        (json!({"type":"none"}), ProtocolError::MissingMetadata),
    ];
    for (raw, expected) in cases {
        let result = decision(&raw.to_string(), player());
        assert!(
            matches!(result, Err(MortalError::Protocol(ref actual)) if *actual == expected),
            "{raw}: {result:?}"
        );
    }
    assert!(matches!(
        decision("not JSON", player()),
        Err(MortalError::Json(_))
    ));
}

#[test]
fn invalid_chi_shape_is_rejected_and_red_consumed_tile_is_accepted() {
    let invalid = response(
        json!({"type":"chi","actor":0,"target":3,"pai":"7m","consumed":["9m","1p"]}),
        1 << 38,
        json!([1.0]),
    );
    assert!(matches!(
        decision(&invalid.to_string(), player()),
        Err(MortalError::Protocol(ProtocolError::UnexpectedAction))
    ));
    let valid = response(
        json!({"type":"chi","actor":0,"target":3,"pai":"4m","consumed":["5mr","6m"]}),
        1 << 38,
        json!([1.0]),
    );
    assert!(decision(&valid.to_string(), player()).unwrap().is_some());
}

#[test]
fn masks_only_opponents_private_information_without_changing_source_event() {
    let raw = json!({
        "type":"start_kyoku", "bakaze":"E", "kyoku":1, "honba":0, "kyotaku":0,
        "oya":0, "scores":vec![25000;4], "dora_marker":"3s", "tehais": vec![vec!["1m";13];4],
    });
    let event: Event = serde_json::from_value(raw.clone()).unwrap();
    let masked = visible_event(&event, player()).unwrap();
    assert_eq!(masked["tehais"][0], raw["tehais"][0]);
    for actor in 1..4 {
        assert_eq!(masked["tehais"][actor], json!(vec!["?"; 13]));
    }
    assert_eq!(serde_json::to_value(event).unwrap(), raw);
    for actor in 0..4 {
        let event: Event =
            serde_json::from_value(json!({"type":"tsumo","actor":actor,"pai":"5mr"})).unwrap();
        assert_eq!(
            visible_event(&event, player()).unwrap()["pai"],
            if actor == 0 { "5mr" } else { "?" }
        );
        let public: Event = serde_json::from_value(
            json!({"type":"dahai","actor":actor,"pai":"5mr","tsumogiri":true}),
        )
        .unwrap();
        assert_eq!(visible_event(&public, player()).unwrap()["pai"], "5mr");
    }
}
