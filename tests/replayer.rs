use std::array;

use convlog::Event;
use kyoku::mahjong::hand::HandMutationError;
use kyoku::mahjong::meld::Meld;
use kyoku::mahjong::player_index::PlayerIndex;
use kyoku::mahjong::round::{CallError, RoundPhase, Wind};
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
        RoundPhase::AfterDraw { player: player(0) }
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
        replayer.apply(&Event::Reach { actor: 0 }),
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
