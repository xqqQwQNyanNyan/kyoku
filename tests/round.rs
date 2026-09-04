use std::array;

use kyoku::mahjong::hand::Hand;
use kyoku::mahjong::hand::HandMutationError;
use kyoku::mahjong::meld::Meld;
use kyoku::mahjong::player::{
    Discard, DiscardCallError, PlayerRiichiError, PlayerState, RiichiState, ScoreMutationError,
};
use kyoku::mahjong::player_index::PlayerIndex;
use kyoku::mahjong::round::{
    CallError, DiscardError, DoraError, DrawError, DrawSource, EndKyokuError, HoraError, KanKind,
    RiichiError, RoundId, RoundPhase, RoundResult, RoundState, RyuukyokuError, Wind,
};
use kyoku::mahjong::tile::Tile;

fn tile(value: u8) -> Tile {
    Tile::new(value).expect("test tile must be valid")
}

fn player(value: u8) -> PlayerIndex {
    PlayerIndex::new(value).expect("test player index must be valid")
}

fn players() -> [PlayerState; 4] {
    array::from_fn(|index| {
        let hand = Hand::new(vec![tile(index as u8); 13], vec![]).expect("test hand must be valid");
        PlayerState::new(hand, 25_000 + index as i32, vec![])
    })
}

fn call_state() -> RoundState {
    let mut players = players();
    players[0] = PlayerState::new(
        Hand::new(vec![tile(9); 13], vec![]).unwrap(),
        25_000,
        vec![Discard::new(tile(2), false, false, false)],
    );
    players[1] = PlayerState::new(
        Hand::new([vec![tile(0), tile(1)], vec![tile(9); 11]].concat(), vec![]).unwrap(),
        25_000,
        vec![],
    );
    players[2] = PlayerState::new(
        Hand::new([vec![tile(2)], vec![tile(9); 12]].concat(), vec![]).unwrap(),
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
        RoundPhase::AfterDiscard { player: player(0) },
    )
}

#[test]
fn round_id_accepts_only_four_player_round_numbers() {
    for number in 1..=4 {
        let round = RoundId::new(Wind::East, number).expect("round number must be valid");
        assert_eq!(round.number(), number);
    }

    assert_eq!(RoundId::new(Wind::East, 0), None);
    assert_eq!(RoundId::new(Wind::East, 5), None);
    assert_eq!(RoundId::new(Wind::East, u8::MAX), None);
}

#[test]
fn round_id_exposes_wind_and_derives_dealer_from_number() {
    let cases = [
        (Wind::East, 1, player(0)),
        (Wind::South, 2, player(1)),
        (Wind::West, 3, player(2)),
        (Wind::North, 4, player(3)),
    ];

    for (wind, number, dealer) in cases {
        let round = RoundId::new(wind, number).expect("round number must be valid");
        assert_eq!(round.wind(), wind);
        assert_eq!(round.dealer(), dealer);
    }
}

#[test]
fn round_phase_reports_a_player_only_when_the_variant_stores_one() {
    let player = player(2);
    let phases = [
        RoundPhase::AfterDraw {
            player,
            source: DrawSource::Wall,
        },
        RoundPhase::AfterDiscard { player },
        RoundPhase::AfterCall { player },
        RoundPhase::AfterKanDeclaration {
            player,
            kind: KanKind::Daiminkan,
        },
        RoundPhase::AfterKanDeclaration {
            player,
            kind: KanKind::Ankan,
        },
        RoundPhase::AfterKanDeclaration {
            player,
            kind: KanKind::Kakan,
        },
    ];

    for phase in phases {
        assert_eq!(phase.player(), Some(player));
    }
    assert_eq!(RoundPhase::Initial.player(), None);
    assert_eq!(RoundPhase::AwaitingEnd(RoundResult::Hora).player(), None);
    assert_eq!(RoundPhase::Ended(RoundResult::Hora).player(), None);
}

#[test]
fn round_state_exposes_snapshot_fields() {
    let players = players();
    let round = RoundId::new(Wind::South, 3).expect("round number must be valid");
    let dora_indicators = vec![tile(0), tile(34)];
    let phase = RoundPhase::AfterDiscard { player: player(1) };
    let state = RoundState::new(
        players.clone(),
        round,
        2,
        1,
        dora_indicators.clone(),
        36,
        phase,
    );

    assert_eq!(state.players(), &players);
    assert_eq!(state.player(player(3)), &players[3]);
    assert_eq!(state.round(), round);
    assert_eq!(state.honba(), 2);
    assert_eq!(state.riichi_sticks(), 1);
    assert_eq!(state.dora_indicators(), dora_indicators);
    assert_eq!(state.remaining_draws(), 36);
    assert_eq!(state.phase(), phase);
}

#[test]
fn round_start_draw_and_discard_update_owned_state() {
    let round = RoundId::new(Wind::East, 1).unwrap();
    let mut state = RoundState::start(players(), round, 2, 1, tile(31));

    assert_eq!(state.remaining_draws(), 70);
    assert_eq!(state.phase(), RoundPhase::Initial);
    assert_eq!(state.dora_indicators(), [tile(31)]);

    state.draw(player(0), tile(10)).unwrap();
    assert_eq!(state.remaining_draws(), 69);
    assert_eq!(
        state.phase(),
        RoundPhase::AfterDraw {
            player: player(0),
            source: DrawSource::Wall,
        }
    );
    assert_eq!(state.player(player(0)).hand().effective_tile_count(), 14);

    state.discard(player(0), tile(10), true).unwrap();
    assert_eq!(
        state.phase(),
        RoundPhase::AfterDiscard { player: player(0) }
    );
    assert_eq!(state.player(player(0)).hand().effective_tile_count(), 13);
    assert_eq!(state.player(player(0)).discards()[0].tile(), tile(10));
}

#[test]
fn ryuukyoku_applies_score_deltas_and_preserves_non_phase_round_state() {
    let phase = RoundPhase::AfterDiscard { player: player(2) };
    let mut state = RoundState::new(
        players(),
        RoundId::new(Wind::South, 2).unwrap(),
        3,
        2,
        vec![tile(31), tile(4)],
        17,
        phase,
    );
    let original = state.clone();

    state.ryuukyoku([-1_000, 3_000, -1_000, -1_000]).unwrap();

    assert_eq!(state.player(player(0)).score(), 24_000);
    assert_eq!(state.player(player(1)).score(), 28_001);
    assert_eq!(state.player(player(2)).score(), 24_002);
    assert_eq!(state.player(player(3)).score(), 24_003);
    assert_eq!(
        state.phase(),
        RoundPhase::AwaitingEnd(RoundResult::Ryukyoku)
    );
    assert_eq!(state.honba(), original.honba());
    assert_eq!(state.riichi_sticks(), original.riichi_sticks());
    assert_eq!(state.remaining_draws(), original.remaining_draws());
    assert_eq!(state.dora_indicators(), original.dora_indicators());
    for index in 0..4 {
        let player = player(index);
        assert_eq!(state.player(player).hand(), original.player(player).hand());
        assert_eq!(
            state.player(player).discards(),
            original.player(player).discards()
        );
        assert_eq!(
            state.player(player).riichi(),
            original.player(player).riichi()
        );
    }
}

#[test]
fn zero_delta_ryuukyoku_only_prepares_the_round_to_end() {
    let mut state = RoundState::start(
        players(),
        RoundId::new(Wind::East, 1).unwrap(),
        1,
        2,
        tile(31),
    );
    let original = state.clone();

    state.ryuukyoku([0; 4]).unwrap();

    assert_eq!(state.players(), original.players());
    assert_eq!(state.honba(), original.honba());
    assert_eq!(state.riichi_sticks(), original.riichi_sticks());
    assert_eq!(state.remaining_draws(), original.remaining_draws());
    assert_eq!(state.dora_indicators(), original.dora_indicators());
    assert_eq!(
        state.phase(),
        RoundPhase::AwaitingEnd(RoundResult::Ryukyoku)
    );
}

#[test]
fn ryuukyoku_waits_for_end_kyoku() {
    let phase = RoundPhase::AfterDraw {
        player: player(0),
        source: DrawSource::Wall,
    };
    let mut state = RoundState::new(
        players(),
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        0,
        vec![tile(31)],
        20,
        phase,
    );

    state.ryuukyoku([0; 4]).unwrap();
    assert_eq!(
        state.phase(),
        RoundPhase::AwaitingEnd(RoundResult::Ryukyoku)
    );
}

#[test]
fn end_kyoku_confirms_hora_and_ryuukyoku_without_other_mutation() {
    let mut hora = RoundState::start(
        players(),
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        0,
        tile(31),
    );
    hora.draw(player(0), tile(10)).unwrap();
    hora.hora([6_000, -2_000, -2_000, -2_000]).unwrap();

    let hora_settled = hora.clone();
    hora.end_kyoku().unwrap();
    assert_eq!(hora.phase(), RoundPhase::Ended(RoundResult::Hora));
    assert_eq!(hora.players(), hora_settled.players());
    assert_eq!(hora.honba(), hora_settled.honba());
    assert_eq!(hora.riichi_sticks(), hora_settled.riichi_sticks());
    assert_eq!(hora.remaining_draws(), hora_settled.remaining_draws());
    assert_eq!(hora.dora_indicators(), hora_settled.dora_indicators());

    let mut ryuukyoku = RoundState::start(
        players(),
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        0,
        tile(31),
    );
    ryuukyoku.ryuukyoku([0; 4]).unwrap();

    let ryuukyoku_settled = ryuukyoku.clone();
    ryuukyoku.end_kyoku().unwrap();
    assert_eq!(ryuukyoku.phase(), RoundPhase::Ended(RoundResult::Ryukyoku));
    assert_eq!(ryuukyoku.players(), ryuukyoku_settled.players());
    assert_eq!(ryuukyoku.honba(), ryuukyoku_settled.honba());
    assert_eq!(ryuukyoku.riichi_sticks(), ryuukyoku_settled.riichi_sticks());
    assert_eq!(
        ryuukyoku.remaining_draws(),
        ryuukyoku_settled.remaining_draws()
    );
    assert_eq!(
        ryuukyoku.dora_indicators(),
        ryuukyoku_settled.dora_indicators()
    );
}

#[test]
fn end_kyoku_rejects_invalid_phases_without_mutation() {
    let round = RoundId::new(Wind::East, 1).unwrap();
    let invalid_phases = [
        RoundPhase::Initial,
        RoundPhase::AfterDraw {
            player: player(0),
            source: DrawSource::Wall,
        },
        RoundPhase::Ended(RoundResult::Hora),
    ];

    for phase in invalid_phases {
        let mut state = RoundState::new(players(), round, 0, 0, vec![tile(31)], 30, phase);
        let original = state.clone();

        assert_eq!(
            state.end_kyoku(),
            Err(EndKyokuError::InvalidPhase { phase })
        );
        assert_eq!(state, original);
    }
}

#[test]
fn terminal_round_phases_reject_draw_discard_and_dora_without_mutation() {
    fn assert_rejected(state: &mut RoundState, phase: RoundPhase) {
        let original = state.clone();
        assert_eq!(
            state.draw(player(0), tile(11)),
            Err(DrawError::InvalidPhase { phase })
        );
        assert_eq!(
            state.discard(player(0), tile(10), true),
            Err(DiscardError::InvalidPhase { phase })
        );
        assert_eq!(
            state.reveal_dora(tile(30)),
            Err(DoraError::InvalidPhase { phase })
        );
        assert_eq!(state, &original);
    }

    let mut state = RoundState::start(
        players(),
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        0,
        tile(31),
    );
    state.draw(player(0), tile(10)).unwrap();
    state.hora([6_000, -2_000, -2_000, -2_000]).unwrap();
    assert_rejected(&mut state, RoundPhase::AwaitingEnd(RoundResult::Hora));

    state.end_kyoku().unwrap();
    assert_rejected(&mut state, RoundPhase::Ended(RoundResult::Hora));
}

#[test]
fn ryuukyoku_rejects_an_ended_round_without_mutation() {
    let phase = RoundPhase::Ended(RoundResult::Hora);
    let mut state = RoundState::new(
        players(),
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        1,
        vec![tile(31)],
        20,
        phase,
    );
    let original = state.clone();

    assert_eq!(
        state.ryuukyoku([-1_000, 3_000, -1_000, -1_000]),
        Err(RyuukyokuError::InvalidPhase { phase })
    );
    assert_eq!(state, original);
}

#[test]
fn failed_ryuukyoku_score_update_does_not_partially_mutate_the_round() {
    let mut round_players = players();
    round_players[3] = PlayerState::new(
        Hand::new(vec![tile(3); 13], vec![]).unwrap(),
        i32::MAX,
        vec![],
    );
    let mut state = RoundState::new(
        round_players,
        RoundId::new(Wind::East, 1).unwrap(),
        2,
        1,
        vec![tile(31)],
        20,
        RoundPhase::AfterDiscard { player: player(0) },
    );
    let original = state.clone();

    assert_eq!(
        state.ryuukyoku([1_000, 1_000, 1_000, 1]),
        Err(RyuukyokuError::Score {
            player: player(3),
            error: ScoreMutationError::Overflow {
                score: i32::MAX,
                delta: 1,
            },
        })
    );
    assert_eq!(state, original);
}

#[test]
fn hora_after_draw_applies_all_score_deltas_and_waits_for_end_kyoku() {
    let mut state = RoundState::start(
        players(),
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        0,
        tile(31),
    );
    state.draw(player(0), tile(10)).unwrap();

    state.hora([6_000, -2_000, -2_000, -2_000]).unwrap();

    assert_eq!(state.player(player(0)).score(), 31_000);
    assert_eq!(state.player(player(1)).score(), 23_001);
    assert_eq!(state.player(player(2)).score(), 23_002);
    assert_eq!(state.player(player(3)).score(), 23_003);
    assert_eq!(state.phase(), RoundPhase::AwaitingEnd(RoundResult::Hora));
}

#[test]
fn hora_after_discard_waits_for_end_kyoku() {
    let mut state = RoundState::start(
        players(),
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        0,
        tile(31),
    );
    state.draw(player(0), tile(10)).unwrap();
    state.discard(player(0), tile(10), true).unwrap();

    state.hora([-8_000, 8_000, 0, 0]).unwrap();

    assert_eq!(state.player(player(0)).score(), 17_000);
    assert_eq!(state.player(player(1)).score(), 33_001);
    assert_eq!(state.phase(), RoundPhase::AwaitingEnd(RoundResult::Hora));
}

#[test]
fn hora_accepts_aggregated_multi_ron_score_deltas() {
    let mut state = RoundState::new(
        players(),
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        0,
        vec![tile(31)],
        30,
        RoundPhase::AfterDiscard { player: player(3) },
    );

    state.hora([12_000, 8_000, 4_000, -24_000]).unwrap();

    assert_eq!(state.player(player(0)).score(), 37_000);
    assert_eq!(state.player(player(1)).score(), 33_001);
    assert_eq!(state.player(player(2)).score(), 29_002);
    assert_eq!(state.player(player(3)).score(), 1_003);
    assert_eq!(state.phase(), RoundPhase::AwaitingEnd(RoundResult::Hora));
}

#[test]
fn hora_rejects_invalid_phases_without_mutation() {
    let round = RoundId::new(Wind::East, 1).unwrap();
    let invalid_phases = [
        RoundPhase::Initial,
        RoundPhase::AfterCall { player: player(1) },
        RoundPhase::AwaitingEnd(RoundResult::Ryukyoku),
        RoundPhase::Ended(RoundResult::Hora),
    ];

    for phase in invalid_phases {
        let mut state = RoundState::new(players(), round, 0, 0, vec![tile(31)], 30, phase);
        let original = state.clone();

        assert_eq!(
            state.hora([3_000, -1_000, -1_000, -1_000]),
            Err(HoraError::InvalidPhase { phase })
        );
        assert_eq!(state, original);
    }
}

#[test]
fn failed_hora_score_update_does_not_partially_mutate_the_round() {
    let mut round_players = players();
    round_players[3] = PlayerState::new(
        Hand::new(vec![tile(3); 13], vec![]).unwrap(),
        i32::MAX,
        vec![],
    );
    let mut state = RoundState::new(
        round_players,
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        2,
        vec![tile(31)],
        30,
        RoundPhase::AfterDiscard { player: player(0) },
    );
    let original = state.clone();

    assert_eq!(
        state.hora([1_000, 1_000, 1_000, 1]),
        Err(HoraError::Score {
            player: player(3),
            error: ScoreMutationError::Overflow {
                score: i32::MAX,
                delta: 1,
            },
        })
    );
    assert_eq!(state, original);
    assert_eq!(state.riichi_sticks(), 2);
}

#[test]
fn hora_after_riichi_declaration_discard_does_not_accept_riichi() {
    let mut state = RoundState::start(
        players(),
        RoundId::new(Wind::East, 1).unwrap(),
        3,
        2,
        tile(31),
    );
    state.draw(player(0), tile(10)).unwrap();
    state.declare_riichi(player(0)).unwrap();
    state.discard(player(0), tile(10), true).unwrap();

    state.hora([-8_000, 10_000, 0, 0]).unwrap();

    assert_eq!(state.player(player(0)).riichi(), RiichiState::Declared);
    assert_eq!(state.player(player(0)).score(), 17_000);
    assert_eq!(state.player(player(1)).score(), 35_001);
    assert_eq!(state.riichi_sticks(), 0);
    assert_eq!(state.honba(), 3);
    assert_eq!(state.phase(), RoundPhase::AwaitingEnd(RoundResult::Hora));

    let ended = state.clone();
    assert_eq!(
        state.accept_riichi(player(0)),
        Err(RiichiError::InvalidPhase {
            phase: RoundPhase::AwaitingEnd(RoundResult::Hora),
        })
    );
    assert_eq!(state, ended);
}

#[test]
fn hora_after_kakan_declaration_supports_chankan() {
    let mut state = RoundState::new(
        players(),
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        0,
        vec![tile(31)],
        30,
        RoundPhase::AfterKanDeclaration {
            player: player(0),
            kind: KanKind::Kakan,
        },
    );

    state.hora([-8_000, 8_000, 0, 0]).unwrap();

    assert_eq!(state.phase(), RoundPhase::AwaitingEnd(RoundResult::Hora));
}

#[test]
fn hora_after_ankan_declaration_supports_kokushi_chankan() {
    let mut state = RoundState::new(
        players(),
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        0,
        vec![tile(31)],
        30,
        RoundPhase::AfterKanDeclaration {
            player: player(0),
            kind: KanKind::Ankan,
        },
    );

    state.hora([-8_000, 8_000, 0, 0]).unwrap();

    assert_eq!(state.phase(), RoundPhase::AwaitingEnd(RoundResult::Hora));
}

#[test]
fn hora_after_daiminkan_declaration_is_rejected_without_mutation() {
    let phase = RoundPhase::AfterKanDeclaration {
        player: player(0),
        kind: KanKind::Daiminkan,
    };
    let mut state = RoundState::new(
        players(),
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        2,
        vec![tile(31)],
        30,
        phase,
    );
    let original = state.clone();

    assert_eq!(
        state.hora([-8_000, 10_000, 0, 0]),
        Err(HoraError::InvalidPhase { phase })
    );
    assert_eq!(state, original);
    assert_eq!(state.player(player(0)).score(), 25_000);
    assert_eq!(state.phase(), phase);
    assert_eq!(state.riichi_sticks(), 2);
}

#[test]
fn draw_with_an_empty_wall_leaves_the_round_unchanged() {
    let round = RoundId::new(Wind::East, 1).unwrap();
    let mut state = RoundState::new(
        players(),
        round,
        0,
        0,
        vec![tile(31)],
        0,
        RoundPhase::Initial,
    );
    let original = state.clone();

    let error = state.draw(player(0), tile(10)).unwrap_err();

    assert_eq!(error, DrawError::NoRemainingDraws);
    assert_eq!(state, original);
}

#[test]
fn riichi_declaration_and_acceptance_update_the_round_at_distinct_steps() {
    let round = RoundId::new(Wind::East, 1).unwrap();
    let mut state = RoundState::start(players(), round, 0, 2, tile(31));
    state.draw(player(0), tile(10)).unwrap();
    let phase = state.phase();

    state.declare_riichi(player(0)).unwrap();
    assert_eq!(state.player(player(0)).riichi(), RiichiState::Declared);
    assert_eq!(state.player(player(0)).score(), 25_000);
    assert_eq!(state.riichi_sticks(), 2);
    assert_eq!(state.phase(), phase);

    state.discard(player(0), tile(10), true).unwrap();
    assert!(state.player(player(0)).discards()[0].is_riichi());
    assert_eq!(state.player(player(0)).riichi(), RiichiState::Declared);
    assert_eq!(state.player(player(0)).score(), 25_000);
    assert_eq!(state.riichi_sticks(), 2);

    let phase = state.phase();
    state.accept_riichi(player(0)).unwrap();
    assert_eq!(state.player(player(0)).riichi(), RiichiState::Accepted);
    assert_eq!(state.player(player(0)).score(), 24_000);
    assert_eq!(state.riichi_sticks(), 3);
    assert_eq!(state.phase(), phase);
}

#[test]
fn riichi_requires_1000_points_without_changing_the_round_on_failure() {
    let mut round_players = players();
    round_players[0] = PlayerState::new(Hand::new(vec![tile(0); 13], vec![]).unwrap(), 999, vec![]);
    let mut state = RoundState::start(
        round_players,
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        0,
        tile(31),
    );
    state.draw(player(0), tile(10)).unwrap();
    let original = state.clone();

    assert_eq!(
        state.declare_riichi(player(0)),
        Err(RiichiError::Player {
            player: player(0),
            error: PlayerRiichiError::InsufficientPoints { score: 999 },
        })
    );
    assert_eq!(state, original);
}

#[test]
fn riichi_acceptance_requires_the_declaration_discard() {
    let round = RoundId::new(Wind::East, 1).unwrap();
    let mut state = RoundState::start(players(), round, 0, 0, tile(31));
    state.draw(player(0), tile(10)).unwrap();
    state.declare_riichi(player(0)).unwrap();
    let declared = state.clone();

    assert_eq!(
        state.accept_riichi(player(0)),
        Err(RiichiError::InvalidPhase {
            phase: RoundPhase::AfterDraw {
                player: player(0),
                source: DrawSource::Wall,
            },
        })
    );
    assert_eq!(state, declared);

    state.discard(player(0), tile(10), true).unwrap();
    state.accept_riichi(player(0)).unwrap();
    let accepted = state.clone();
    assert_eq!(
        state.accept_riichi(player(0)),
        Err(RiichiError::Player {
            player: player(0),
            error: PlayerRiichiError::InvalidAcceptance {
                state: RiichiState::Accepted,
            },
        })
    );
    assert_eq!(state, accepted);
}

#[test]
fn riichi_declaration_discard_is_marked_only_once() {
    let round = RoundId::new(Wind::East, 1).unwrap();
    let mut state = RoundState::start(players(), round, 0, 0, tile(31));
    state.draw(player(0), tile(10)).unwrap();
    state.declare_riichi(player(0)).unwrap();
    state.discard(player(0), tile(10), true).unwrap();

    state.draw(player(0), tile(11)).unwrap();
    state.discard(player(0), tile(11), true).unwrap();

    let discards = state.player(player(0)).discards();
    assert!(discards[0].is_riichi());
    assert!(!discards[1].is_riichi());
}

#[test]
fn called_riichi_declaration_discard_keeps_both_flags() {
    let mut round_players = players();
    round_players[2] = PlayerState::new(
        Hand::new([vec![tile(10); 2], vec![tile(9); 11]].concat(), vec![]).unwrap(),
        25_000,
        vec![],
    );
    let mut state = RoundState::start(
        round_players,
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        0,
        tile(31),
    );
    state.draw(player(0), tile(10)).unwrap();
    state.declare_riichi(player(0)).unwrap();
    state.discard(player(0), tile(10), true).unwrap();
    state.accept_riichi(player(0)).unwrap();

    state
        .pon(player(2), player(0), tile(10), [tile(10); 2])
        .unwrap();

    let declaration = state.player(player(0)).discards()[0];
    assert!(declaration.is_riichi());
    assert!(declaration.is_called());
}

#[test]
fn failed_riichi_acceptance_does_not_partially_mutate_the_round() {
    let mut state = RoundState::start(
        players(),
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        u8::MAX,
        tile(31),
    );
    state.draw(player(0), tile(10)).unwrap();
    state.declare_riichi(player(0)).unwrap();
    state.discard(player(0), tile(10), true).unwrap();
    let original = state.clone();

    assert_eq!(
        state.accept_riichi(player(0)),
        Err(RiichiError::TooManySticks)
    );
    assert_eq!(state, original);
}

#[test]
fn chi_updates_the_caller_river_and_phase_together() {
    let mut state = call_state();

    state
        .chi(player(1), player(0), tile(2), [tile(0), tile(1)])
        .unwrap();

    assert_eq!(state.phase(), RoundPhase::AfterCall { player: player(1) });
    assert!(state.player(player(0)).discards()[0].is_called());
    assert_eq!(
        state.player(player(1)).hand().concealed(),
        vec![tile(9); 11]
    );
    assert_eq!(
        state.player(player(1)).hand().melds(),
        [Meld::Chi {
            tiles: [tile(0), tile(1), tile(2)],
            called: tile(2),
            from: player(0),
        }]
    );
}

#[test]
fn pon_is_available_to_any_other_player() {
    let mut state = call_state();
    state = RoundState::new(
        {
            let mut players = state.players().clone();
            players[2] = PlayerState::new(
                Hand::new([vec![tile(2), tile(2)], vec![tile(9); 11]].concat(), vec![]).unwrap(),
                25_000,
                vec![],
            );
            players
        },
        state.round(),
        state.honba(),
        state.riichi_sticks(),
        state.dora_indicators().to_vec(),
        state.remaining_draws(),
        state.phase(),
    );

    state
        .pon(player(2), player(0), tile(2), [tile(2), tile(2)])
        .unwrap();

    assert_eq!(state.phase(), RoundPhase::AfterCall { player: player(2) });
    assert!(state.player(player(0)).discards()[0].is_called());
    assert!(matches!(
        state.player(player(2)).hand().melds(),
        [Meld::Pon { from, .. }] if *from == player(0)
    ));
}

#[test]
fn invalid_call_contexts_are_rejected_without_mutation() {
    let mut invalid_phase = call_state();
    invalid_phase = RoundState::new(
        invalid_phase.players().clone(),
        invalid_phase.round(),
        invalid_phase.honba(),
        invalid_phase.riichi_sticks(),
        invalid_phase.dora_indicators().to_vec(),
        invalid_phase.remaining_draws(),
        RoundPhase::AfterCall { player: player(1) },
    );
    let original = invalid_phase.clone();
    assert_eq!(
        invalid_phase.pon(player(2), player(0), tile(2), [tile(2), tile(2)]),
        Err(CallError::InvalidPhase {
            phase: RoundPhase::AfterCall { player: player(1) },
        })
    );
    assert_eq!(invalid_phase, original);

    let cases = [
        (
            call_state().chi(player(2), player(0), tile(2), [tile(0), tile(1)]),
            CallError::InvalidChiActor {
                expected: player(1),
                actual: player(2),
            },
        ),
        (
            call_state().pon(player(2), player(3), tile(2), [tile(2), tile(2)]),
            CallError::WrongTarget {
                expected: player(0),
                actual: player(3),
            },
        ),
    ];

    for (result, expected) in cases {
        assert_eq!(result, Err(expected));
    }

    let mut state = call_state();
    let original = state.clone();
    let error = state
        .pon(player(2), player(0), tile(3), [tile(3), tile(3)])
        .unwrap_err();
    assert_eq!(
        error,
        CallError::Discard {
            player: player(0),
            error: DiscardCallError::TileMismatch {
                discarded: tile(2),
                called: tile(3),
            },
        }
    );
    assert_eq!(state, original);
}

#[test]
fn caller_hand_failure_rolls_back_the_entire_call() {
    let mut state = call_state();
    let original = state.clone();

    let error = state
        .pon(player(2), player(0), tile(2), [tile(2), tile(2)])
        .unwrap_err();

    assert_eq!(
        error,
        CallError::Hand {
            player: player(2),
            error: HandMutationError::TileNotFound { tile: tile(2) },
        }
    );
    assert_eq!(state, original);
}

#[test]
fn daiminkan_updates_the_discard_hand_and_phase_atomically() {
    let mut state = call_state();
    state = RoundState::new(
        {
            let mut players = state.players().clone();
            players[2] = PlayerState::new(
                Hand::new([vec![tile(2); 3], vec![tile(9); 10]].concat(), vec![]).unwrap(),
                25_000,
                vec![],
            );
            players
        },
        state.round(),
        state.honba(),
        state.riichi_sticks(),
        state.dora_indicators().to_vec(),
        state.remaining_draws(),
        state.phase(),
    );

    state
        .daiminkan(player(2), player(0), tile(2), [tile(2); 3])
        .unwrap();

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

    state.draw(player(2), tile(7)).unwrap();
    assert_eq!(
        state.phase(),
        RoundPhase::AfterDraw {
            player: player(2),
            source: DrawSource::Rinshan,
        }
    );
    let phase = state.phase();
    state.reveal_dora(tile(30)).unwrap();
    assert_eq!(state.dora_indicators(), [tile(31), tile(30)]);
    assert_eq!(state.phase(), phase);
}

#[test]
fn ankan_and_kakan_require_the_actor_to_have_just_drawn() {
    let mut ankan_players = players();
    ankan_players[1] = PlayerState::new(
        Hand::new([vec![tile(31); 4], vec![tile(9); 10]].concat(), vec![]).unwrap(),
        25_000,
        vec![],
    );
    let mut ankan = RoundState::new(
        ankan_players,
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        0,
        vec![tile(30)],
        50,
        RoundPhase::AfterDraw {
            player: player(1),
            source: DrawSource::Wall,
        },
    );
    ankan.ankan(player(1), [tile(31); 4]).unwrap();
    assert_eq!(
        ankan.phase(),
        RoundPhase::AfterKanDeclaration {
            player: player(1),
            kind: KanKind::Ankan,
        }
    );
    ankan.reveal_dora(tile(29)).unwrap();
    assert_eq!(
        ankan.phase(),
        RoundPhase::AfterKanDeclaration {
            player: player(1),
            kind: KanKind::Ankan,
        }
    );
    ankan.draw(player(1), tile(8)).unwrap();
    assert_eq!(
        ankan.phase(),
        RoundPhase::AfterDraw {
            player: player(1),
            source: DrawSource::Rinshan,
        }
    );

    let pon = Meld::Pon {
        tiles: [tile(4); 3],
        called: tile(4),
        from: player(0),
    };
    let mut kakan_players = players();
    kakan_players[2] = PlayerState::new(
        Hand::new([vec![tile(4)], vec![tile(9); 10]].concat(), vec![pon]).unwrap(),
        25_000,
        vec![],
    );
    let mut kakan = RoundState::new(
        kakan_players,
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        0,
        vec![tile(30)],
        50,
        RoundPhase::AfterDraw {
            player: player(2),
            source: DrawSource::Wall,
        },
    );
    kakan.kakan(player(2), tile(4), [tile(4); 3]).unwrap();
    assert!(matches!(
        kakan.player(player(2)).hand().melds(),
        [Meld::Kakan { from, .. }] if *from == player(0)
    ));
    assert_eq!(
        kakan.phase(),
        RoundPhase::AfterKanDeclaration {
            player: player(2),
            kind: KanKind::Kakan,
        }
    );
    kakan.draw(player(2), tile(8)).unwrap();
    assert_eq!(
        kakan.phase(),
        RoundPhase::AfterDraw {
            player: player(2),
            source: DrawSource::Rinshan,
        }
    );
    let phase = kakan.phase();
    kakan.reveal_dora(tile(29)).unwrap();
    assert_eq!(kakan.phase(), phase);

    let mut wrong_actor = RoundState::new(
        players(),
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        0,
        vec![tile(30)],
        50,
        RoundPhase::AfterDraw {
            player: player(0),
            source: DrawSource::Wall,
        },
    );
    let original = wrong_actor.clone();
    assert_eq!(
        wrong_actor.ankan(player(1), [tile(1); 4]),
        Err(CallError::WrongActor {
            expected: player(0),
            actual: player(1),
        })
    );
    assert_eq!(wrong_actor, original);
}

#[test]
fn consecutive_kans_allow_interleaved_dora_events_and_rinshan_draws() {
    let actor = player(2);
    let target = player(0);
    let first_kakan_pon = Meld::Pon {
        tiles: [tile(4); 3],
        called: tile(4),
        from: player(1),
    };
    let second_kakan_pon = Meld::Pon {
        tiles: [tile(13); 3],
        called: tile(13),
        from: player(3),
    };
    let mut round_players = players();
    round_players[usize::from(target.get_id())] = PlayerState::new(
        Hand::new(vec![tile(9); 13], vec![]).unwrap(),
        25_000,
        vec![Discard::new(tile(22), false, false, false)],
    );
    round_players[usize::from(actor.get_id())] = PlayerState::new(
        Hand::new(
            [vec![tile(22); 3], vec![tile(31); 4]].concat(),
            vec![first_kakan_pon, second_kakan_pon],
        )
        .unwrap(),
        25_000,
        vec![],
    );
    let mut state = RoundState::new(
        round_players,
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        0,
        vec![tile(27)],
        50,
        RoundPhase::AfterDiscard { player: target },
    );

    state
        .daiminkan(actor, target, tile(22), [tile(22); 3])
        .unwrap();
    state.draw(actor, tile(4)).unwrap();
    assert_eq!(
        state.phase(),
        RoundPhase::AfterDraw {
            player: actor,
            source: DrawSource::Rinshan,
        }
    );

    state.kakan(actor, tile(4), [tile(4); 3]).unwrap();
    assert_eq!(
        state.phase(),
        RoundPhase::AfterKanDeclaration {
            player: actor,
            kind: KanKind::Kakan,
        }
    );
    state.reveal_dora(tile(28)).unwrap();
    state.draw(actor, tile(13)).unwrap();
    state.reveal_dora(tile(29)).unwrap();

    state.ankan(actor, [tile(31); 4]).unwrap();
    state.reveal_dora(tile(30)).unwrap();
    assert_eq!(
        state.phase(),
        RoundPhase::AfterKanDeclaration {
            player: actor,
            kind: KanKind::Ankan,
        }
    );
    state.draw(actor, tile(7)).unwrap();

    state.kakan(actor, tile(13), [tile(13); 3]).unwrap();
    state.draw(actor, tile(8)).unwrap();
    state.reveal_dora(tile(31)).unwrap();
    assert_eq!(
        state.phase(),
        RoundPhase::AfterDraw {
            player: actor,
            source: DrawSource::Rinshan,
        }
    );
    assert_eq!(
        state.dora_indicators(),
        [tile(27), tile(28), tile(29), tile(30), tile(31)]
    );

    state.discard(actor, tile(8), true).unwrap();
    assert_eq!(state.phase(), RoundPhase::AfterDiscard { player: actor });
    assert_eq!(state.player(actor).hand().melds().len(), 4);
}
