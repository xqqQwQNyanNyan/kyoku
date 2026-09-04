use std::array;

use convlog::tenhou::Log;
use convlog::tenhou_to_mjai;
use convlog::{Event, Tile as ConvlogTile};
use kyoku::mahjong::player_index::PlayerIndex;
use kyoku::mahjong::round::{DrawSource, RoundPhase, RoundResult};
use kyoku::mahjong::tile::Tile;
use kyoku::replay::inspector::{ReplayInspector, format_event, format_phase, format_tile};

const TENHOU_GAMES: &[(&str, &str)] = &[
    (
        "ranked_game",
        include_str!("../fixtures/tenhou/ranked_game.json"),
    ),
    ("rinshan", include_str!("../fixtures/tenhou/rinshan.json")),
    ("ryukyoku", include_str!("../fixtures/tenhou/ryukyoku.json")),
    (
        "four_reach",
        include_str!("../fixtures/tenhou/four_reach.json"),
    ),
    ("chankan", include_str!("../fixtures/tenhou/chankan.json")),
    (
        "complex_nakis",
        include_str!("../fixtures/tenhou/complex_nakis.json"),
    ),
    (
        "kyushukyuhai",
        include_str!("../fixtures/tenhou/kyushukyuhai.json"),
    ),
];

#[test]
fn replays_checked_in_tenhou_games() {
    for (name, fixture) in TENHOU_GAMES {
        let log = Log::from_json_str(fixture)
            .unwrap_or_else(|error| panic!("{name}: invalid tenhou JSON: {error}"));
        let events = tenhou_to_mjai(&log)
            .unwrap_or_else(|error| panic!("{name}: failed to convert to MJAI: {error}"));
        let mut inspector = ReplayInspector::new();

        for event in &events {
            inspector
                .apply(event)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
        }

        println!(
            "========== FIXTURE {name} ==========\n{}",
            inspector.output()
        );
        assert!(
            inspector.state().is_some(),
            "{name}: fixture must contain a round"
        );
    }
}

#[test]
fn replays_double_ron_fixture_with_accumulated_hora_result() {
    let log = Log::from_json_str(include_str!("../fixtures/tenhou/double_ron.json"))
        .expect("double-ron fixture must be valid Tenhou JSON");
    let events = tenhou_to_mjai(&log).expect("double-ron fixture must convert to MJAI");
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Hora { .. }))
            .count(),
        2
    );

    let mut inspector = ReplayInspector::new();
    for event in &events {
        inspector
            .apply(event)
            .expect("double-ron events must replay successfully");
    }

    let state = inspector.state().expect("fixture must contain a round");
    assert_eq!(
        state.phase(),
        RoundPhase::Ended(RoundResult::Hora {
            score_deltas: [13_000, 0, 2_000, -14_000],
        })
    );
    assert_eq!(
        array::from_fn(|index| state.players()[index].score()),
        [53_800, 26_300, 39_400, 500]
    );
}

#[test]
fn formats_tiles_events_and_phases_for_humans() {
    assert_eq!(format_tile(Tile::new(0).unwrap()), "1m");
    assert_eq!(format_tile(Tile::new(35).unwrap()), "5pr");
    assert_eq!(format_tile(Tile::new(27).unwrap()), "E");
    assert_eq!(format_tile(Tile::new(33).unwrap()), "C");
    assert_eq!(
        format_phase(RoundPhase::AfterDraw {
            player: PlayerIndex::new(0).unwrap(),
            source: DrawSource::Wall,
        }),
        "AfterDraw(P0, Wall)"
    );
    assert_eq!(
        format_event(&Event::Dahai {
            actor: 3,
            pai: ConvlogTile::try_from(26_u8).unwrap(),
            tsumogiri: true,
        }),
        "Dahai(P3 9s(tsumogiri))"
    );
}

#[test]
fn failure_reports_indices_context_and_last_valid_snapshot() {
    let mut inspector = ReplayInspector::new();
    inspector.apply(&minimal_start_kyoku()).unwrap();

    let error = inspector.apply(&Event::EndKyoku).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("[G001 K001]: EndKyoku"));
    assert!(message.contains("preceding events:"));
    assert!(message.contains("last valid state:"));
    assert!(message.contains("round=E1"));
    assert!(message.contains("cannot end round"));
}

#[test]
fn hidden_events_still_replay_and_remain_in_failure_diagnostics() {
    let mut inspector = ReplayInspector::new();
    inspector
        .apply_with_context(&minimal_start_kyoku(), false, None)
        .unwrap();
    assert!(inspector.output().is_empty());

    let error = inspector.apply(&Event::EndKyoku).unwrap_err();
    assert_eq!(error.global_index, 1);
    assert!(error.preceding_events[0].contains("StartKyoku"));
    assert!(error.last_valid_state.contains("round=E1"));
}

#[test]
fn a_lazily_printed_kyoku_header_uses_the_start_snapshot() {
    let mut inspector = ReplayInspector::new();
    inspector
        .apply_with_context(&minimal_start_kyoku(), false, None)
        .unwrap();
    inspector
        .apply_with_context(
            &Event::Tsumo {
                actor: 0,
                pai: ConvlogTile::try_from(0_u8).unwrap(),
            },
            false,
            None,
        )
        .unwrap();
    inspector
        .apply_with_context(
            &Event::Dora {
                dora_marker: ConvlogTile::try_from(1_u8).unwrap(),
            },
            true,
            None,
        )
        .unwrap();

    let header = inspector.output().lines().next().unwrap();
    assert!(header.contains("[G000 K000]"));
    assert!(header.contains("dora=[P]"));
}

#[test]
fn inspector_reports_headers_deltas_summaries_and_full_state() {
    let mut inspector = ReplayInspector::new();
    inspector.apply(&minimal_start_kyoku()).unwrap();
    inspector
        .apply(&Event::Tsumo {
            actor: 0,
            pai: ConvlogTile::try_from(0_u8).unwrap(),
        })
        .unwrap();
    inspector
        .apply(&Event::Dahai {
            actor: 0,
            pai: ConvlogTile::try_from(0_u8).unwrap(),
            tsumogiri: true,
        })
        .unwrap();
    inspector
        .apply(&Event::Hora {
            actor: 0,
            target: 0,
            deltas: Some([1_000, -1_000, 0, 0]),
            ura_markers: None,
        })
        .unwrap();
    inspector.apply(&Event::EndKyoku).unwrap();

    let output = inspector.output();
    assert!(output.contains("KYOKU E1 [G000 K000]"));
    assert!(output.contains("dealer=P0"));
    assert!(output.contains("phase Initial -> AfterDraw(P0, Wall)"));
    assert!(output.contains("P0 hand +[1m]"));
    assert!(output.contains("discard +[1m(tsumogiri)]"));
    assert!(
        output.contains(
            "result=Hora event_delta=[1000 -1000 0 0] accumulated_deltas=[1000 -1000 0 0] scores=[26000 24000 25000 25000]"
        )
    );
    assert!(output.contains(
        "kyoku_summary round=E1 result=Hora accumulated_deltas=[1000 -1000 0 0] final_scores=[26000 24000 25000 25000]"
    ));

    let snapshot = inspector.full_state();
    assert!(snapshot.contains("P0 score=26000"));
    assert!(snapshot.contains("hand="));
    assert!(snapshot.contains("melds="));
    assert!(snapshot.contains("discards=[1m(tsumogiri)]"));
}

#[test]
fn inspector_reports_ryuukyoku_summary() {
    let mut inspector = ReplayInspector::new();
    inspector.apply(&minimal_start_kyoku()).unwrap();
    inspector
        .apply(&Event::Ryukyoku {
            deltas: Some([0, 0, 0, 0]),
        })
        .unwrap();
    inspector.apply(&Event::EndKyoku).unwrap();

    assert!(
        inspector
            .output()
            .contains("result=Ryuukyoku event_delta=[0 0 0 0] scores=[25000 25000 25000 25000]")
    );
    assert!(inspector.output().contains(
        "kyoku_summary round=E1 result=Ryuukyoku settlement_deltas=[0 0 0 0] final_scores=[25000 25000 25000 25000]"
    ));
}

#[test]
fn inspector_distinguishes_each_hora_delta_from_the_accumulated_result() {
    let mut inspector = ReplayInspector::new();
    inspector.apply(&minimal_start_kyoku()).unwrap();
    inspector
        .apply(&Event::Tsumo {
            actor: 0,
            pai: ConvlogTile::try_from(0_u8).unwrap(),
        })
        .unwrap();
    inspector
        .apply(&Event::Hora {
            actor: 0,
            target: 1,
            deltas: Some([8_000, -8_000, 0, 0]),
            ura_markers: None,
        })
        .unwrap();
    inspector
        .apply(&Event::Hora {
            actor: 2,
            target: 1,
            deltas: Some([0, -2_000, 2_000, 0]),
            ura_markers: None,
        })
        .unwrap();
    inspector.apply(&Event::EndKyoku).unwrap();

    let output = inspector.output();
    assert!(
        output.contains("event_delta=[0 -2000 2000 0] accumulated_deltas=[8000 -10000 2000 0]")
    );
    assert!(
        output
            .contains("kyoku_summary round=E1 result=Hora accumulated_deltas=[8000 -10000 2000 0]")
    );
}

#[test]
fn inspector_prints_input_ryuukyoku_reason_without_adding_it_to_domain_state() {
    let mut inspector = ReplayInspector::new();
    inspector.apply(&minimal_start_kyoku()).unwrap();
    inspector
        .apply_with_context(
            &Event::Ryukyoku {
                deltas: Some([0; 4]),
            },
            true,
            Some("四家立直"),
        )
        .unwrap();
    inspector.apply(&Event::EndKyoku).unwrap();

    assert!(inspector.output().contains("Ryukyoku(reason=四家立直"));
    assert!(
        inspector
            .output()
            .contains("result=Ryuukyoku reason=四家立直")
    );
    assert!(inspector.output().contains(
        "kyoku_summary round=E1 result=Ryuukyoku reason=四家立直 settlement_deltas=[0 0 0 0]"
    ));
    assert_eq!(
        inspector.state().unwrap().phase(),
        RoundPhase::Ended(RoundResult::Ryukyoku)
    );
}

fn minimal_start_kyoku() -> Event {
    Event::StartKyoku {
        bakaze: ConvlogTile::try_from(27_u8).unwrap(),
        dora_marker: ConvlogTile::try_from(31_u8).unwrap(),
        kyoku: 1,
        honba: 0,
        kyotaku: 0,
        oya: 0,
        scores: [25_000; 4],
        tehais: array::from_fn(|_| array::from_fn(|_| ConvlogTile::try_from(0_u8).unwrap())),
    }
}
