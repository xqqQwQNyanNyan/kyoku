use std::array;

use kyoku::mahjong::hand::Hand;
use kyoku::mahjong::player::PlayerState;
use kyoku::mahjong::player_index::PlayerIndex;
use kyoku::mahjong::round::{KanKind, RoundId, RoundPhase, RoundState, Wind};
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
        RoundPhase::AfterDraw { player },
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
    assert_eq!(RoundPhase::Ended.player(), None);
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
