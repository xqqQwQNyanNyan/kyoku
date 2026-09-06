use super::*;

fn player() -> PlayerIndex {
    PlayerIndex::new(0).unwrap()
}

fn tile(id: u8) -> convlog::Tile {
    convlog::Tile::try_from(id).unwrap()
}

fn discard(actor: u8, id: u8) -> Event {
    Event::Dahai {
        actor,
        pai: tile(id),
        tsumogiri: true,
    }
}

fn events() -> Vec<Event> {
    vec![
        Event::StartKyoku {
            bakaze: tile(27),
            dora_marker: tile(31),
            kyoku: 1,
            honba: 0,
            kyotaku: 0,
            oya: 0,
            scores: [25_000; 4],
            tehais: std::array::from_fn(|_| std::array::from_fn(|i| tile(i as u8))),
        },
        Event::Tsumo {
            actor: 0,
            pai: tile(28),
        },
        Event::Reach { actor: 0 },
        discard(0, 28),
        Event::ReachAccepted { actor: 0 },
        Event::Tsumo {
            actor: 1,
            pai: tile(27),
        },
        discard(1, 27),
        Event::Tsumo {
            actor: 2,
            pai: tile(28),
        },
    ]
}

#[test]
fn actual_actions_separate_riichi_discard_pass_and_incomplete_logs() {
    let events = events();
    assert_eq!(
        recorded_action(&events, 1, player()),
        RecordedAction::Taken {
            event_index: 2,
            action: Event::Reach { actor: 0 }
        }
    );
    assert_eq!(
        recorded_action(&events, 2, player()),
        RecordedAction::Taken {
            event_index: 3,
            action: discard(0, 28)
        }
    );
    assert_eq!(
        recorded_action(&events, 6, player()),
        RecordedAction::Passed
    );
    assert_eq!(
        recorded_action(&events[..7], 6, player()),
        RecordedAction::Unresolved
    );
    let events = [
        discard(1, 2),
        Event::Pon {
            actor: 2,
            target: 1,
            pai: tile(2),
            consumed: [tile(2); 2],
        },
    ];
    assert_eq!(
        recorded_action(&events, 0, player()),
        RecordedAction::Unresolved
    );
    let events = [
        discard(1, 2),
        Event::Chi {
            actor: 0,
            target: 1,
            pai: tile(2),
            consumed: [tile(0), tile(1)],
        },
        discard(0, 3),
    ];
    assert_eq!(
        recorded_action(&events, 0, player()),
        RecordedAction::Taken {
            event_index: 1,
            action: events[1].clone()
        }
    );
    assert_eq!(
        recorded_action(&events, 1, player()),
        RecordedAction::Taken {
            event_index: 2,
            action: events[2].clone()
        }
    );
}

#[test]
fn double_ron_and_kan_windows_do_not_guess_a_pass_or_expose_settlement() {
    let hora = |actor| Event::Hora {
        actor,
        target: 1,
        deltas: Some([0; 4]),
        ura_markers: Some(vec![tile(27)]),
    };
    let events = [discard(1, 2), hora(2), hora(0), Event::EndKyoku];
    assert_eq!(
        recorded_action(&events, 0, player()),
        RecordedAction::Taken {
            event_index: 2,
            action: Event::Hora {
                actor: 0,
                target: 1,
                deltas: None,
                ura_markers: None
            }
        }
    );
    let events = [discard(1, 2), hora(2), Event::EndKyoku];
    assert_eq!(
        recorded_action(&events, 0, player()),
        RecordedAction::Unresolved
    );
    for kan in [
        Event::Kakan {
            actor: 1,
            pai: tile(2),
            consumed: [tile(2); 3],
        },
        Event::Ankan {
            actor: 1,
            consumed: [tile(2); 4],
        },
    ] {
        let events = [
            kan,
            Event::Dora {
                dora_marker: tile(27),
            },
            Event::Tsumo {
                actor: 1,
                pai: tile(28),
            },
        ];
        assert_eq!(
            recorded_action(&events, 0, player()),
            RecordedAction::Passed
        );
    }
    let events = [
        discard(1, 2),
        Event::Ryukyoku {
            deltas: Some([0; 4]),
        },
    ];
    assert_eq!(
        recorded_action(&events, 0, player()),
        RecordedAction::Unresolved
    );
}

#[cfg(unix)]
mod process {
    use super::*;
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        sync::atomic::{AtomicUsize, Ordering},
    };

    static NEXT: AtomicUsize = AtomicUsize::new(0);
    const NONE: &str = r#"{"type":"none","meta":{"mask_bits":0}}"#;
    const PASS: &str =
        r#"{"type":"none","meta":{"mask_bits":43980465111040,"q_values":[0.9,0.1]}}"#;
    const DISCARD: &str = r#"{"type":"dahai","actor":0,"pai":"S","tsumogiri":true,"meta":{"mask_bits":268435456,"q_values":[0.5]}}"#;

    struct Engine(PathBuf);
    impl Engine {
        fn new(responses: &[&str], exit: u8) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "kyoku-game-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&dir).unwrap();
            let mut script = format!(
                "#!/bin/sh\ncd -- \"$(dirname -- \"$0\")\"\nprintf 'start\\n' >> starts\nprintf '%s\\n' '{{\"version\":4,\"tag\":\"test\",\"sha256\":\"{}\"}}'\nn=0\nwhile IFS= read -r event; do\nprintf '%s\\n' \"$event\" >> events\ncase $n in\n",
                "0".repeat(64)
            );
            for (index, response) in responses.iter().enumerate() {
                script.push_str(&format!("{index}) printf '%s\\n' '{response}' ;;\n"));
            }
            script.push_str(&format!(
                "*) exit 9 ;;\nesac\nn=$((n+1))\ndone\nexit {exit}\n"
            ));
            fs::write(dir.join("engine"), script).unwrap();
            fs::set_permissions(dir.join("engine"), fs::Permissions::from_mode(0o700)).unwrap();
            Self(dir)
        }
        fn review(&self, events: &[Event]) -> Result<GameReview, ReviewError> {
            review_game(
                events,
                player(),
                &MortalConfig {
                    python: &self.0.join("engine"),
                    runtime: Path::new("."),
                    checkpoint: Path::new("unused"),
                },
            )
        }
    }
    impl Drop for Engine {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn game_caches_every_decision_with_one_engine_and_matches_single_position() {
        let engine = Engine::new(&[NONE, DISCARD, DISCARD, NONE, NONE, NONE, PASS, NONE], 0);
        let game = engine.review(&events()).unwrap();
        assert_eq!(
            game.decisions()
                .iter()
                .map(|point| point.review.event_index)
                .collect::<Vec<_>>(),
            [1, 2, 6]
        );
        assert!(game.at_event(0).is_none());
        assert!(game.at_event(usize::MAX).is_none());
        assert_eq!(game.at_event(6).unwrap().actual, RecordedAction::Passed);
        assert_eq!(game.at_event(6).unwrap().turn, 2);
        assert!(matches!(
            game.at_event(6)
                .unwrap()
                .review
                .decision
                .as_ref()
                .unwrap()
                .recommended,
            Event::None
        ));
        let single_engine = Engine::new(&[NONE, DISCARD], 0);
        let single = review_at(
            &events(),
            player(),
            1,
            &MortalConfig {
                python: &single_engine.0.join("engine"),
                runtime: Path::new("."),
                checkpoint: Path::new("unused"),
            },
        )
        .unwrap();
        let cached = &game.at_event(1).unwrap().review;
        assert_eq!(cached.position, single.position);
        assert_eq!(cached.discards, single.discards);
        assert_eq!(cached.model.sha256, single.model.sha256);
        assert_eq!(
            cached.decision.as_ref().unwrap().recommended,
            single.decision.unwrap().recommended
        );
        let sent: Vec<serde_json::Value> = fs::read_to_string(engine.0.join("events"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(sent.len(), events().len());
        assert_eq!(sent[5]["pai"], "?");
        assert_eq!(sent[0]["tehais"][1][0], "?");
        fs::remove_file(engine.0.join("engine")).unwrap();
        for event in [6, 1, 2, 6, 1] {
            assert!(game.at_event(event).is_some());
        }
        assert_eq!(
            fs::read_to_string(engine.0.join("starts")).unwrap(),
            "start\n"
        );
    }

    #[test]
    fn invalid_log_and_engine_failures_do_not_return_partial_caches() {
        let engine = Engine::new(&[NONE, DISCARD], 0);
        let mut invalid = events();
        invalid.push(discard(0, 33));
        assert!(matches!(
            engine.review(&invalid),
            Err(ReviewError::Replay { event_index: 8, .. })
        ));
        assert!(!engine.0.join("starts").exists());
        assert!(engine.review(&[]).unwrap().decisions().is_empty());
        assert!(!engine.0.join("starts").exists());
        let broken = Engine::new(&[NONE, "broken"], 0);
        assert!(matches!(
            broken.review(&events()[..2]),
            Err(ReviewError::Inference { event_index: 1, .. })
        ));
        let failed_exit = Engine::new(&[NONE, DISCARD], 7);
        assert!(matches!(
            failed_exit.review(&events()[..2]),
            Err(ReviewError::Finish(_))
        ));
        let no_decisions = Engine::new(&[NONE, NONE], 0);
        assert!(
            no_decisions
                .review(&events()[..2])
                .unwrap()
                .decisions()
                .is_empty()
        );
    }
}
