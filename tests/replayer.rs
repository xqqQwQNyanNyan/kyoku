use std::array;

use convlog::Event;
use kyoku::mahjong::hand::HandMutationError;
use kyoku::mahjong::meld::Meld;
use kyoku::mahjong::player::RiichiState;
use kyoku::mahjong::player_index::PlayerIndex;
use kyoku::mahjong::round::{
    CallError, DrawError, DrawSource, EndKyokuError, KanKind, RiichiError, RoundPhase, RoundResult,
    Wind,
};
use kyoku::mahjong::tile::Tile;
use kyoku::replay::replayer::{ReplayError, Replayer};

fn mjai_tile(value: u8) -> convlog::Tile {
    convlog::Tile::try_from(value).expect("test mjai tile must be valid")
}

fn tile(value: u8) -> Tile {
    Tile::new(value).expect("test domain tile must be valid")
}

fn player(value: u8) -> PlayerIndex {
    PlayerIndex::new(value).expect("test player index must be valid")
}

fn start_kyoku(kyoku: u8) -> Event {
    Event::StartKyoku {
        bakaze: mjai_tile(27),
        dora_marker: mjai_tile(31),
        kyoku,
        honba: 2,
        kyotaku: 1,
        oya: kyoku - 1,
        scores: [25_000, 24_000, 26_000, 25_000],
        tehais: array::from_fn(|player| {
            array::from_fn(|index| mjai_tile(((player * 13 + index) % 34) as u8))
        }),
    }
}

fn start_kyoku_with_hands(tehais: [[u8; 13]; 4]) -> Event {
    Event::StartKyoku {
        bakaze: mjai_tile(27),
        dora_marker: mjai_tile(31),
        kyoku: 1,
        honba: 0,
        kyotaku: 0,
        oya: 0,
        scores: [25_000; 4],
        tehais: tehais.map(|hand| hand.map(mjai_tile)),
    }
}

fn replayer_waiting_for_call(caller: u8, caller_hand: [u8; 13]) -> Replayer {
    let mut hands = [[9; 13]; 4];
    hands[usize::from(caller)] = caller_hand;
    let mut replayer = Replayer::new();
    replayer.apply(&start_kyoku_with_hands(hands)).unwrap();
    replayer
        .apply(&Event::Tsumo {
            actor: 0,
            pai: mjai_tile(2),
        })
        .unwrap();
    replayer
        .apply(&Event::Dahai {
            actor: 0,
            pai: mjai_tile(2),
            tsumogiri: true,
        })
        .unwrap();
    replayer
}

#[test]
fn start_kyoku_creates_the_round_state() {
    let mut replayer = Replayer::new();

    replayer.apply(&start_kyoku(1)).unwrap();

    let state = replayer.state().expect("round must have started");
    assert_eq!(state.round().wind(), Wind::East);
    assert_eq!(state.round().number(), 1);
    assert_eq!(state.honba(), 2);
    assert_eq!(state.riichi_sticks(), 1);
    assert_eq!(state.dora_indicators(), [tile(31)]);
    assert_eq!(state.remaining_draws(), 70);
    assert_eq!(state.phase(), RoundPhase::Initial);
    assert_eq!(state.player(player(1)).score(), 24_000);
    assert_eq!(state.player(player(0)).hand().concealed().len(), 13);
}

#[test]
fn draw_and_discard_reconstruct_the_current_state() {
    let mut replayer = Replayer::new();
    replayer.apply(&start_kyoku(1)).unwrap();

    replayer
        .apply(&Event::Tsumo {
            actor: 0,
            pai: mjai_tile(20),
        })
        .unwrap();
    let after_draw = replayer.state().unwrap();
    assert_eq!(
        after_draw.phase(),
        RoundPhase::AfterDraw {
            player: player(0),
            source: DrawSource::Wall,
        }
    );
    assert_eq!(after_draw.remaining_draws(), 69);
    assert_eq!(after_draw.player(player(0)).hand().concealed().len(), 14);

    replayer
        .apply(&Event::Dahai {
            actor: 0,
            pai: mjai_tile(20),
            tsumogiri: true,
        })
        .unwrap();
    let after_discard = replayer.state().unwrap();
    assert_eq!(
        after_discard.phase(),
        RoundPhase::AfterDiscard { player: player(0) }
    );
    assert_eq!(after_discard.player(player(0)).hand().concealed().len(), 13);
    let discard = after_discard.player(player(0)).discards()[0];
    assert_eq!(discard.tile(), tile(20));
    assert!(discard.is_tsumogiri());
    assert!(!discard.is_riichi());
}

#[test]
fn ryukyoku_applies_deltas_and_end_kyoku_confirms_the_ended_round() {
    let mut replayer = Replayer::new();
    replayer.apply(&start_kyoku(1)).unwrap();
    replayer
        .apply(&Event::Ryukyoku {
            deltas: Some([-1_000, 3_000, -1_000, -1_000]),
        })
        .unwrap();

    let state = replayer.state().unwrap();
    assert_eq!(state.player(player(0)).score(), 24_000);
    assert_eq!(state.player(player(1)).score(), 27_000);
    assert_eq!(state.player(player(2)).score(), 25_000);
    assert_eq!(state.player(player(3)).score(), 24_000);
    assert_eq!(
        state.phase(),
        RoundPhase::AwaitingEnd(RoundResult::Ryukyoku)
    );
    assert_eq!(state.honba(), 2);
    assert_eq!(state.riichi_sticks(), 1);
    assert_eq!(state.remaining_draws(), 70);

    let settled = state.clone();
    replayer.apply(&Event::EndKyoku).unwrap();
    let ended = replayer.state().unwrap();
    assert_eq!(ended.phase(), RoundPhase::Ended(RoundResult::Ryukyoku));
    assert_eq!(ended.players(), settled.players());
    assert_eq!(ended.honba(), settled.honba());
    assert_eq!(ended.riichi_sticks(), settled.riichi_sticks());
    assert_eq!(ended.remaining_draws(), settled.remaining_draws());
    assert_eq!(ended.dora_indicators(), settled.dora_indicators());
}

#[test]
fn hora_applies_deltas_through_the_domain_and_waits_for_end_kyoku() {
    let mut replayer = Replayer::new();
    replayer.apply(&start_kyoku(1)).unwrap();
    replayer
        .apply(&Event::Tsumo {
            actor: 0,
            pai: mjai_tile(20),
        })
        .unwrap();

    replayer
        .apply(&Event::Hora {
            actor: 0,
            target: 0,
            deltas: Some([6_000, -2_000, -2_000, -2_000]),
            ura_markers: None,
        })
        .unwrap();

    let state = replayer.state().unwrap();
    assert_eq!(state.player(player(0)).score(), 31_000);
    assert_eq!(state.player(player(1)).score(), 22_000);
    assert_eq!(state.player(player(2)).score(), 24_000);
    assert_eq!(state.player(player(3)).score(), 23_000);
    assert_eq!(state.riichi_sticks(), 0);
    assert_eq!(state.phase(), RoundPhase::AwaitingEnd(RoundResult::Hora));

    replayer.apply(&Event::EndKyoku).unwrap();
    assert_eq!(
        replayer.state().unwrap().phase(),
        RoundPhase::Ended(RoundResult::Hora)
    );
}

#[test]
fn start_game_and_end_game_are_accepted_without_round_state() {
    let mut replayer = Replayer::new();

    replayer
        .apply(&Event::StartGame {
            names: array::from_fn(|index| format!("player-{index}")),
            kyoku_first: 0,
            aka_flag: true,
        })
        .unwrap();
    assert_eq!(replayer.state(), None);

    replayer.apply(&Event::EndGame).unwrap();
    assert_eq!(replayer.state(), None);
}

#[test]
fn ryukyoku_without_deltas_fails_without_mutation() {
    let mut replayer = Replayer::new();
    replayer.apply(&start_kyoku(1)).unwrap();
    let original = replayer.state().unwrap().clone();

    assert_eq!(
        replayer.apply(&Event::Ryukyoku { deltas: None }),
        Err(ReplayError::MissingScoreDeltas)
    );
    assert_eq!(replayer.state(), Some(&original));
    assert_eq!(
        replayer.apply(&Event::EndKyoku),
        Err(ReplayError::EndKyoku(EndKyokuError::InvalidPhase {
            phase: RoundPhase::Initial
        }))
    );
    assert_eq!(replayer.state(), Some(&original));
}

#[test]
fn end_kyoku_rejects_a_round_that_has_not_ended() {
    let mut replayer = Replayer::new();
    replayer.apply(&start_kyoku(1)).unwrap();
    let original = replayer.state().unwrap().clone();

    assert_eq!(
        replayer.apply(&Event::EndKyoku),
        Err(ReplayError::EndKyoku(EndKyokuError::InvalidPhase {
            phase: RoundPhase::Initial
        }))
    );
    assert_eq!(replayer.state(), Some(&original));
}

#[test]
fn end_kyoku_is_not_idempotent_and_ended_round_rejects_game_events() {
    let mut replayer = Replayer::new();
    replayer.apply(&start_kyoku(1)).unwrap();
    replayer
        .apply(&Event::Ryukyoku {
            deltas: Some([0; 4]),
        })
        .unwrap();
    replayer.apply(&Event::EndKyoku).unwrap();
    let phase = RoundPhase::Ended(RoundResult::Ryukyoku);
    let ended = replayer.state().unwrap().clone();

    assert_eq!(
        replayer.apply(&Event::EndKyoku),
        Err(ReplayError::EndKyoku(EndKyokuError::InvalidPhase { phase }))
    );
    assert_eq!(
        replayer.apply(&Event::Tsumo {
            actor: 0,
            pai: mjai_tile(20),
        }),
        Err(ReplayError::Draw(DrawError::InvalidPhase { phase }))
    );
    assert_eq!(replayer.state(), Some(&ended));
}

#[test]
fn reach_events_replay_the_pending_discard_and_acceptance_separately() {
    let mut replayer = Replayer::new();
    replayer
        .apply(&start_kyoku_with_hands([[9; 13]; 4]))
        .unwrap();
    replayer
        .apply(&Event::Tsumo {
            actor: 0,
            pai: mjai_tile(20),
        })
        .unwrap();
    let phase = replayer.state().unwrap().phase();

    replayer.apply(&Event::Reach { actor: 0 }).unwrap();
    let declared = replayer.state().unwrap();
    assert_eq!(declared.player(player(0)).riichi(), RiichiState::Declared);
    assert_eq!(declared.player(player(0)).score(), 25_000);
    assert_eq!(declared.riichi_sticks(), 0);
    assert_eq!(declared.phase(), phase);

    replayer
        .apply(&Event::Dahai {
            actor: 0,
            pai: mjai_tile(20),
            tsumogiri: true,
        })
        .unwrap();
    let discarded = replayer.state().unwrap();
    assert!(discarded.player(player(0)).discards()[0].is_riichi());
    assert_eq!(discarded.player(player(0)).riichi(), RiichiState::Declared);
    assert_eq!(discarded.player(player(0)).score(), 25_000);
    assert_eq!(discarded.riichi_sticks(), 0);

    replayer.apply(&Event::ReachAccepted { actor: 0 }).unwrap();
    let accepted = replayer.state().unwrap();
    assert_eq!(accepted.player(player(0)).riichi(), RiichiState::Accepted);
    assert_eq!(accepted.player(player(0)).score(), 24_000);
    assert_eq!(accepted.riichi_sticks(), 1);
}

#[test]
fn reach_discard_without_acceptance_keeps_the_payment_pending() {
    let mut replayer = Replayer::new();
    replayer
        .apply(&start_kyoku_with_hands([[9; 13]; 4]))
        .unwrap();
    replayer
        .apply(&Event::Tsumo {
            actor: 0,
            pai: mjai_tile(20),
        })
        .unwrap();
    replayer.apply(&Event::Reach { actor: 0 }).unwrap();
    replayer
        .apply(&Event::Dahai {
            actor: 0,
            pai: mjai_tile(20),
            tsumogiri: true,
        })
        .unwrap();

    let state = replayer.state().unwrap();
    assert!(state.player(player(0)).discards()[0].is_riichi());
    assert_eq!(state.player(player(0)).riichi(), RiichiState::Declared);
    assert_eq!(state.player(player(0)).score(), 25_000);
    assert_eq!(state.riichi_sticks(), 0);
}

#[test]
fn invalid_reach_transition_is_reported_by_the_domain() {
    let mut replayer = Replayer::new();
    replayer
        .apply(&start_kyoku_with_hands([[9; 13]; 4]))
        .unwrap();
    replayer
        .apply(&Event::Tsumo {
            actor: 0,
            pai: mjai_tile(20),
        })
        .unwrap();
    replayer.apply(&Event::Reach { actor: 0 }).unwrap();
    let original = replayer.state().unwrap().clone();

    assert_eq!(
        replayer.apply(&Event::ReachAccepted { actor: 0 }),
        Err(ReplayError::Riichi(RiichiError::InvalidPhase {
            phase: RoundPhase::AfterDraw {
                player: player(0),
                source: DrawSource::Wall,
            },
        }))
    );
    assert_eq!(replayer.state(), Some(&original));
}

#[test]
fn dora_event_appends_an_indicator_without_changing_the_phase() {
    let mut replayer = Replayer::new();
    replayer.apply(&start_kyoku(1)).unwrap();
    replayer
        .apply(&Event::Tsumo {
            actor: 0,
            pai: mjai_tile(20),
        })
        .unwrap();
    let phase = replayer.state().unwrap().phase();

    replayer
        .apply(&Event::Dora {
            dora_marker: mjai_tile(30),
        })
        .unwrap();

    assert_eq!(
        replayer.state().unwrap().dora_indicators(),
        [tile(31), tile(30)]
    );
    assert_eq!(replayer.state().unwrap().phase(), phase);
}

#[test]
fn a_new_start_kyoku_replaces_the_previous_round() {
    let mut replayer = Replayer::new();
    replayer.apply(&start_kyoku(1)).unwrap();
    replayer.apply(&start_kyoku(2)).unwrap();

    assert_eq!(replayer.state().unwrap().round().number(), 2);
    assert_eq!(replayer.state().unwrap().phase(), RoundPhase::Initial);
}

#[test]
fn events_without_a_round_and_unsupported_events_are_reported() {
    let mut replayer = Replayer::new();
    let error = replayer
        .apply(&Event::Tsumo {
            actor: 0,
            pai: mjai_tile(20),
        })
        .unwrap_err();
    assert_eq!(error, ReplayError::NoRound);

    replayer.apply(&start_kyoku(1)).unwrap();
    assert_eq!(
        replayer.apply(&Event::None),
        Err(ReplayError::UnsupportedEvent)
    );
}

#[test]
fn empty_wall_error_is_translated_from_the_domain() {
    let mut replayer = Replayer::new();
    replayer.apply(&start_kyoku(1)).unwrap();

    for _ in 0..70 {
        replayer
            .apply(&Event::Tsumo {
                actor: 0,
                pai: mjai_tile(20),
            })
            .unwrap();
        replayer
            .apply(&Event::Dahai {
                actor: 0,
                pai: mjai_tile(20),
                tsumogiri: true,
            })
            .unwrap();
    }

    let error = replayer
        .apply(&Event::Tsumo {
            actor: 0,
            pai: mjai_tile(20),
        })
        .unwrap_err();
    assert_eq!(error, ReplayError::NoRemainingDraws);
    assert_eq!(replayer.state().unwrap().remaining_draws(), 0);
    assert_eq!(
        replayer
            .state()
            .unwrap()
            .player(player(0))
            .hand()
            .effective_tile_count(),
        13
    );
}

#[test]
fn chi_and_pon_events_are_replayed_through_domain_calls() {
    let mut chi_hand = [9; 13];
    chi_hand[0] = 0;
    chi_hand[1] = 1;
    let mut chi_replayer = replayer_waiting_for_call(1, chi_hand);
    chi_replayer
        .apply(&Event::Chi {
            actor: 1,
            target: 0,
            pai: mjai_tile(2),
            consumed: [mjai_tile(0), mjai_tile(1)],
        })
        .unwrap();
    let chi_state = chi_replayer.state().unwrap();
    assert_eq!(
        chi_state.phase(),
        RoundPhase::AfterCall { player: player(1) }
    );
    assert!(chi_state.player(player(0)).discards()[0].is_called());
    assert!(matches!(
        chi_state.player(player(1)).hand().melds(),
        [Meld::Chi { from, .. }] if *from == player(0)
    ));

    let mut pon_hand = [9; 13];
    pon_hand[0] = 2;
    pon_hand[1] = 2;
    let mut pon_replayer = replayer_waiting_for_call(2, pon_hand);
    pon_replayer
        .apply(&Event::Pon {
            actor: 2,
            target: 0,
            pai: mjai_tile(2),
            consumed: [mjai_tile(2), mjai_tile(2)],
        })
        .unwrap();
    let pon_state = pon_replayer.state().unwrap();
    assert_eq!(
        pon_state.phase(),
        RoundPhase::AfterCall { player: player(2) }
    );
    assert!(pon_state.player(player(0)).discards()[0].is_called());
    assert!(matches!(
        pon_state.player(player(2)).hand().melds(),
        [Meld::Pon { from, .. }] if *from == player(0)
    ));
}

#[test]
fn invalid_call_event_is_reported_and_keeps_the_replay_snapshot() {
    let mut caller_hand = [9; 13];
    caller_hand[0] = 2;
    let mut replayer = replayer_waiting_for_call(2, caller_hand);
    let original = replayer.state().unwrap().clone();

    let error = replayer
        .apply(&Event::Pon {
            actor: 2,
            target: 0,
            pai: mjai_tile(2),
            consumed: [mjai_tile(2), mjai_tile(2)],
        })
        .unwrap_err();

    assert_eq!(
        error,
        ReplayError::Call(CallError::Hand {
            player: player(2),
            error: HandMutationError::TileNotFound { tile: tile(2) },
        })
    );
    assert_eq!(replayer.state(), Some(&original));
    assert!(!replayer.state().unwrap().player(player(0)).discards()[0].is_called());
}

#[test]
fn chi_from_the_wrong_player_is_rejected() {
    let mut caller_hand = [9; 13];
    caller_hand[0] = 0;
    caller_hand[1] = 1;
    let mut replayer = replayer_waiting_for_call(2, caller_hand);
    let original = replayer.state().unwrap().clone();

    assert_eq!(
        replayer.apply(&Event::Chi {
            actor: 2,
            target: 0,
            pai: mjai_tile(2),
            consumed: [mjai_tile(0), mjai_tile(1)],
        }),
        Err(ReplayError::Call(CallError::InvalidChiActor {
            expected: player(1),
            actual: player(2),
        }))
    );
    assert_eq!(replayer.state(), Some(&original));
}

#[test]
fn daiminkan_and_ankan_events_are_replayed_through_domain_calls() {
    let mut daiminkan_hand = [9; 13];
    daiminkan_hand[..3].fill(2);
    let mut daiminkan = replayer_waiting_for_call(2, daiminkan_hand);
    daiminkan
        .apply(&Event::Daiminkan {
            actor: 2,
            target: 0,
            pai: mjai_tile(2),
            consumed: [mjai_tile(2); 3],
        })
        .unwrap();
    let state = daiminkan.state().unwrap();
    assert!(state.player(player(0)).discards()[0].is_called());
    assert!(matches!(
        state.player(player(2)).hand().melds(),
        [Meld::Daiminkan { from, .. }] if *from == player(0)
    ));
    assert_eq!(
        state.phase(),
        RoundPhase::AfterKanDeclaration {
            player: player(2),
            kind: KanKind::Daiminkan,
        }
    );
    daiminkan
        .apply(&Event::Tsumo {
            actor: 2,
            pai: mjai_tile(7),
        })
        .unwrap();
    assert_eq!(
        daiminkan.state().unwrap().phase(),
        RoundPhase::AfterDraw {
            player: player(2),
            source: DrawSource::Rinshan,
        }
    );

    let mut hands = [[9; 13]; 4];
    hands[1][..3].fill(31);
    let mut ankan = Replayer::new();
    ankan.apply(&start_kyoku_with_hands(hands)).unwrap();
    ankan
        .apply(&Event::Tsumo {
            actor: 1,
            pai: mjai_tile(31),
        })
        .unwrap();
    ankan
        .apply(&Event::Ankan {
            actor: 1,
            consumed: [mjai_tile(31); 4],
        })
        .unwrap();
    assert!(matches!(
        ankan.state().unwrap().player(player(1)).hand().melds(),
        [Meld::Ankan { .. }]
    ));
    assert_eq!(
        ankan.state().unwrap().phase(),
        RoundPhase::AfterKanDeclaration {
            player: player(1),
            kind: KanKind::Ankan,
        }
    );
    ankan
        .apply(&Event::Dora {
            dora_marker: mjai_tile(30),
        })
        .unwrap();
    assert!(matches!(
        ankan.state().unwrap().phase(),
        RoundPhase::AfterKanDeclaration {
            player: phase_player,
            kind: KanKind::Ankan,
        } if phase_player == player(1)
    ));
    ankan
        .apply(&Event::Tsumo {
            actor: 1,
            pai: mjai_tile(8),
        })
        .unwrap();
    assert_eq!(
        ankan.state().unwrap().phase(),
        RoundPhase::AfterDraw {
            player: player(1),
            source: DrawSource::Rinshan,
        }
    );
}

#[test]
fn kakan_event_upgrades_the_existing_pon_in_place() {
    let mut pon_hand = [9; 13];
    pon_hand[..2].fill(2);
    let mut replayer = replayer_waiting_for_call(2, pon_hand);
    replayer
        .apply(&Event::Pon {
            actor: 2,
            target: 0,
            pai: mjai_tile(2),
            consumed: [mjai_tile(2); 2],
        })
        .unwrap();
    replayer
        .apply(&Event::Dahai {
            actor: 2,
            pai: mjai_tile(9),
            tsumogiri: false,
        })
        .unwrap();
    replayer
        .apply(&Event::Tsumo {
            actor: 2,
            pai: mjai_tile(2),
        })
        .unwrap();
    replayer
        .apply(&Event::Kakan {
            actor: 2,
            pai: mjai_tile(2),
            consumed: [mjai_tile(2); 3],
        })
        .unwrap();

    let state = replayer.state().unwrap();
    assert_eq!(state.player(player(2)).hand().melds().len(), 1);
    assert!(matches!(
        state.player(player(2)).hand().melds(),
        [Meld::Kakan { called, from, .. }]
            if *called == tile(2) && *from == player(0)
    ));
    assert_eq!(
        state.phase(),
        RoundPhase::AfterKanDeclaration {
            player: player(2),
            kind: KanKind::Kakan,
        }
    );
    replayer
        .apply(&Event::Tsumo {
            actor: 2,
            pai: mjai_tile(7),
        })
        .unwrap();
    replayer
        .apply(&Event::Dora {
            dora_marker: mjai_tile(30),
        })
        .unwrap();
    assert_eq!(
        replayer.state().unwrap().phase(),
        RoundPhase::AfterDraw {
            player: player(2),
            source: DrawSource::Rinshan,
        }
    );
}
