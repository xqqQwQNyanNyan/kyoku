use super::*;
use crate::mahjong::{
    hand::Hand,
    player::PlayerState,
    round::{DrawSource, Wind},
};
use crate::mortal::Candidate;
use std::path::Path;

fn player(index: u8) -> PlayerIndex {
    PlayerIndex::new(index).unwrap()
}
fn tile(value: u8) -> Tile {
    Tile::new(value).unwrap()
}

fn state() -> RoundState {
    let mut players = std::array::from_fn(|_| {
        PlayerState::new(
            Hand::new((0..13).map(tile).collect(), vec![]).unwrap(),
            25_000,
            vec![],
        )
    });
    players[0] = PlayerState::new(
        Hand::new(
            [0, 1, 2, 34, 4, 9, 10, 11, 18, 19, 20, 27, 27, 28]
                .map(tile)
                .to_vec(),
            vec![],
        )
        .unwrap(),
        25_000,
        vec![],
    );
    RoundState::new(
        players,
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        0,
        vec![tile(31)],
        69,
        RoundPhase::AfterDraw {
            player: player(0),
            source: DrawSource::Wall,
        },
    )
}

fn decision(actions: &[Action]) -> Decision {
    Decision {
        recommended: Event::None,
        candidates: actions
            .iter()
            .map(|&action| Candidate {
                action,
                q_value: 0.5,
            })
            .collect(),
        kan_candidates: vec![],
        shanten: None,
        at_furiten: None,
    }
}

#[test]
fn discard_analysis_preserves_red_five_and_respects_candidate_subset() {
    let state = state();
    let decision = decision(&[
        Action::Discard(tile(34)),
        Action::Discard(tile(4)),
        Action::Riichi,
    ]);
    let result = analyze_discards(&state, player(0), 2, Some(&decision)).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(
        result[0],
        discard_efficiency(&state, player(0), tile(34)).unwrap()
    );
    assert_eq!(
        result[1],
        discard_efficiency(&state, player(0), tile(4)).unwrap()
    );
    let restricted = decision_for_discard(tile(28));
    let result = analyze_discards(&state, player(0), 2, Some(&restricted)).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].discard, tile(28));
}

fn decision_for_discard(tile: Tile) -> Decision {
    decision(&[Action::Discard(tile)])
}

#[test]
fn no_discard_opportunity_is_empty_and_bad_candidate_has_context() {
    let state = state();
    assert!(
        analyze_discards(&state, player(0), 2, None)
            .unwrap()
            .is_empty()
    );
    assert!(
        analyze_discards(&state, player(0), 2, Some(&decision(&[Action::Pass])))
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        analyze_discards(&state, player(1), 2, Some(&decision_for_discard(tile(0)))),
        Err(ReviewError::UnexpectedDiscard { event_index: 2, .. })
    ));
    assert!(matches!(
        analyze_discards(&state, player(0), 2, Some(&decision_for_discard(tile(33)))),
        Err(ReviewError::Analysis {
            event_index: 2,
            source: AnalysisError::DiscardNotFound { .. },
            ..
        })
    ));
}

#[test]
fn visible_position_and_efficiency_do_not_depend_on_opponent_concealed_tiles() {
    let original = state();
    let mut players = original.players().clone();
    players[1] = PlayerState::new(
        Hand::new((15..28).map(tile).collect(), vec![]).unwrap(),
        25_000,
        vec![],
    );
    let changed = RoundState::new(
        players,
        original.round(),
        0,
        0,
        vec![tile(31)],
        69,
        original.phase(),
    );
    assert_eq!(
        visible_position(&original, player(0)),
        visible_position(&changed, player(0))
    );
    let decision = decision_for_discard(tile(28));
    assert_eq!(
        analyze_discards(&original, player(0), 2, Some(&decision)).unwrap(),
        analyze_discards(&changed, player(0), 2, Some(&decision)).unwrap()
    );
    assert_eq!(
        visible_position(&changed, player(1)).concealed,
        changed.player(player(1)).hand().concealed()
    );
}

fn events() -> Vec<Event> {
    let mjai = |value: u8| convlog::Tile::try_from(value).unwrap();
    vec![
        Event::StartKyoku {
            bakaze: mjai(27),
            dora_marker: mjai(31),
            kyoku: 1,
            honba: 0,
            kyotaku: 0,
            oya: 0,
            scores: [25_000; 4],
            tehais: std::array::from_fn(|_| std::array::from_fn(|index| mjai(index as u8))),
        },
        Event::Tsumo {
            actor: 0,
            pai: mjai(28),
        },
        // 目标后的非法事件应完全不进入回放或推理。
        Event::Dahai {
            actor: 0,
            pai: mjai(33),
            tsumogiri: false,
        },
    ]
}

#[test]
fn called_position_keeps_public_meld_and_river_and_allows_discard_analysis() {
    let mjai = |value: u8| convlog::Tile::try_from(value).unwrap();
    let mut replay = Replayer::new();
    replay.apply(&events()[0]).unwrap();
    replay
        .apply(&Event::Tsumo {
            actor: 0,
            pai: mjai(2),
        })
        .unwrap();
    replay
        .apply(&Event::Dahai {
            actor: 0,
            pai: mjai(2),
            tsumogiri: true,
        })
        .unwrap();
    replay
        .apply(&Event::Chi {
            actor: 1,
            target: 0,
            pai: mjai(2),
            consumed: [mjai(0), mjai(1)],
        })
        .unwrap();
    let state = replay.state().unwrap();
    let position = visible_position(state, player(1));
    assert_eq!(position.phase, RoundPhase::AfterCall { player: player(1) });
    assert_eq!(position.concealed.len(), 11);
    assert!(position.players[0].discards[0].is_called());
    assert_eq!(position.players[1].melds[0].called(), Some(tile(2)));
    let result =
        analyze_discards(state, player(1), 3, Some(&decision_for_discard(tile(5)))).unwrap();
    assert_eq!(
        result[0],
        discard_efficiency(state, player(1), tile(5)).unwrap()
    );
}

#[test]
fn invalid_request_is_rejected_before_starting_engine() {
    let config = MortalConfig {
        python: Path::new("/nonexistent-review-python"),
        runtime: Path::new("."),
        checkpoint: Path::new("unused"),
    };
    assert!(matches!(
        review_at(&[], player(0), 0, &config),
        Err(ReviewError::EventOutOfRange { event_count: 0, .. })
    ));
    assert!(matches!(
        review_at(&[Event::EndGame], player(0), 0, &config),
        Err(ReviewError::NoRound { event_index: 0 })
    ));
    assert!(matches!(
        review_at(&events(), player(0), 2, &config),
        Err(ReviewError::Replay { event_index: 2, .. })
    ));
    assert!(matches!(
        review_at(&events(), player(0), 1, &config),
        Err(ReviewError::Start(MortalError::Spawn(_)))
    ));
}

#[cfg(unix)]
mod process {
    use super::*;
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Engine {
        directory: PathBuf,
        executable: PathBuf,
    }
    impl Engine {
        fn new(body: &str) -> Self {
            let directory = std::env::temp_dir().join(format!(
                "kyoku-review-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&directory).unwrap();
            let executable = directory.join("engine");
            let ready = serde_json::json!({"version":4,"tag":"test-only","sha256":"0".repeat(64)});
            fs::write(
                &executable,
                format!("#!/bin/sh\nprintf '%s\\n' '{ready}'\n{body}\n"),
            )
            .unwrap();
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
            Self {
                directory,
                executable,
            }
        }
        fn config(&self) -> MortalConfig<'_> {
            MortalConfig {
                python: &self.executable,
                runtime: Path::new("."),
                checkpoint: Path::new("unused"),
            }
        }
    }
    impl Drop for Engine {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    fn two_responses(last: &str, exit_code: u8) -> Engine {
        Engine::new(&format!(
            r#"
IFS= read -r event
printf '%s\n' '{{"type":"none","meta":{{"mask_bits":0}}}}'
IFS= read -r event
printf '%s\n' '{last}'
if IFS= read -r event; then exit 9; fi
exit {exit_code}
"#
        ))
    }

    #[test]
    fn review_aligns_event_position_analysis_and_model_without_reading_future() {
        let engine = two_responses(
            r#"{"type":"dahai","actor":0,"pai":"S","tsumogiri":true,"meta":{"mask_bits":268435456,"q_values":[0.75]}}"#,
            0,
        );
        let review = review_at(&events(), player(0), 1, &engine.config()).unwrap();
        assert_eq!(review.event_index, 1);
        assert_eq!(review.position.concealed.len(), 14);
        assert_eq!(review.position.remaining_draws, 69);
        assert_eq!(review.model.tag, "test-only");
        assert_eq!(review.discards.len(), 1);
        assert_eq!(review.discards[0].discard, tile(28));
        let decision = review.decision.unwrap();
        assert!(matches!(
            decision.recommended,
            Event::Dahai { actor: 0, .. }
        ));
        assert_eq!(decision.candidates[0].q_value, 0.75);
    }

    #[test]
    fn no_opportunity_and_explicit_pass_remain_distinct() {
        let engine = two_responses(r#"{"type":"none","meta":{"mask_bits":0}}"#, 0);
        let result = review_at(&events(), player(0), 1, &engine.config()).unwrap();
        assert!(result.decision.is_none());
        assert!(result.discards.is_empty());
        // 和牌 Q 更高时仍保留引擎最终选择的跳过。
        let engine = two_responses(
            r#"{"type":"none","meta":{"mask_bits":43980465111040,"q_values":[0.9,0.1]}}"#,
            0,
        );
        let result = review_at(&events(), player(0), 1, &engine.config()).unwrap();
        let decision = result.decision.unwrap();
        assert!(matches!(decision.recommended, Event::None));
        assert_eq!(decision.candidates[0].action, Action::Win);
        assert!(result.discards.is_empty());
    }

    #[test]
    fn inference_and_shutdown_failures_do_not_return_partial_reviews() {
        let engine = two_responses("broken", 0);
        assert!(matches!(
            review_at(&events(), player(0), 1, &engine.config()),
            Err(ReviewError::Inference {
                event_index: 1,
                source: MortalError::Json(_),
                ..
            })
        ));
        let engine = two_responses(r#"{"type":"none","meta":{"mask_bits":0}}"#, 7);
        assert!(matches!(
            review_at(&events(), player(0), 1, &engine.config()),
            Err(ReviewError::Finish(MortalError::Exit(_)))
        ));
    }
}
