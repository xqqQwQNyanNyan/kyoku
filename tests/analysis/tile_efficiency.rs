use std::array;

use kyoku::analysis::{
    AnalysisError, DrawCandidates, discard_efficiencies, discard_efficiency, effective_tile_kinds,
    unseen_count, winning_tile_kinds,
};
use kyoku::mahjong::hand::Hand;
use kyoku::mahjong::meld::Meld;
use kyoku::mahjong::player::{Discard, PlayerState};
use kyoku::mahjong::player_index::PlayerIndex;
use kyoku::mahjong::round::{RoundId, RoundPhase, RoundState, Wind};
use kyoku::mahjong::tile::{Tile, TileKind};

fn tile(value: u8) -> Tile {
    Tile::new(value).expect("test tile must be valid")
}

fn kind(value: u8) -> TileKind {
    TileKind::new(value).expect("test tile kind must be valid")
}

fn player(value: u8) -> PlayerIndex {
    PlayerIndex::new(value).expect("test player index must be valid")
}

fn hand(tiles: &[u8]) -> Hand {
    Hand::new(tiles.iter().copied().map(tile).collect(), vec![])
        .expect("test hand must have a valid size")
}

fn state_with_player_hand(hand: Hand) -> RoundState {
    let mut players = array::from_fn(|index| {
        PlayerState::new(
            Hand::new(vec![tile(31 + index as u8 % 3); 13], vec![])
                .expect("test hand must be valid"),
            25_000,
            vec![],
        )
    });
    players[0] = PlayerState::new(hand, 25_000, vec![]);

    RoundState::new(
        players,
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        0,
        vec![tile(30)],
        60,
        RoundPhase::AfterDraw {
            player: player(0),
            source: kyoku::mahjong::round::DrawSource::Wall,
        },
    )
}

#[test]
fn effective_tiles_reduce_shanten_and_require_a_thirteen_tile_hand() {
    let one_away = hand(&[0, 1, 2, 4, 9, 10, 11, 18, 19, 20, 27, 27, 28]);
    let candidates = effective_tile_kinds(&one_away).unwrap();

    assert!(candidates.contains(&kind(3)));
    assert!(candidates.contains(&kind(5)));
    assert!(candidates.contains(&kind(28)));

    let ready = hand(&[0, 1, 2, 9, 10, 11, 18, 19, 20, 21, 22, 23, 27]);
    assert_eq!(
        effective_tile_kinds(&ready),
        Err(AnalysisError::EffectiveTilesRequireShantenAtLeastOne { actual: 0 })
    );
}

#[test]
fn winning_tiles_cover_standard_special_and_open_hands() {
    let standard = hand(&[0, 1, 2, 9, 10, 11, 18, 19, 20, 21, 22, 23, 27]);
    assert_eq!(winning_tile_kinds(&standard).unwrap(), vec![kind(27)]);

    let chiitoitsu = hand(&[0, 0, 8, 8, 9, 9, 17, 17, 18, 18, 26, 26, 27]);
    assert_eq!(winning_tile_kinds(&chiitoitsu).unwrap(), vec![kind(27)]);

    let kokushi = hand(&[0, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 33]);
    assert_eq!(winning_tile_kinds(&kokushi).unwrap().len(), 13);

    let chi = Meld::Chi {
        tiles: [tile(0), tile(1), tile(2)],
        called: tile(0),
        from: player(1),
    };
    let open = Hand::new(
        vec![
            tile(9),
            tile(10),
            tile(11),
            tile(18),
            tile(19),
            tile(20),
            tile(21),
            tile(22),
            tile(23),
            tile(27),
        ],
        vec![chi],
    )
    .unwrap();
    assert_eq!(winning_tile_kinds(&open).unwrap(), vec![kind(27)]);
}

#[test]
fn a_fifth_copy_is_not_an_effective_tile() {
    let exhausted_shape = hand(&[0, 0, 0, 0, 9, 10, 11, 18, 19, 20, 24, 25, 26]);

    assert!(
        !effective_tile_kinds(&exhausted_shape)
            .unwrap()
            .contains(&kind(0))
    );
}

#[test]
fn unseen_count_uses_only_information_visible_to_the_player() {
    let own = hand(&[34, 0, 1, 2, 9, 10, 11, 18, 19, 20, 27, 28, 29]);
    let visible_five_meld = Meld::Chi {
        tiles: [tile(3), tile(4), tile(5)],
        called: tile(4),
        from: player(2),
    };

    let mut players = array::from_fn(|_| PlayerState::new(hand(&[31; 13]), 25_000, vec![]));
    players[0] = PlayerState::new(own, 25_000, vec![]);
    players[1] = PlayerState::new(
        Hand::new(vec![tile(6); 10], vec![visible_five_meld]).unwrap(),
        25_000,
        vec![
            Discard::new(tile(4), false, false, false),
            Discard::new(tile(4), false, false, true),
        ],
    );

    let state = RoundState::new(
        players,
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        0,
        vec![tile(34)],
        60,
        RoundPhase::Initial,
    );

    assert_eq!(unseen_count(&state, player(0), kind(4)), 0);
    assert_eq!(unseen_count(&state, player(0), kind(6)), 4);
}

#[test]
fn discard_efficiency_keeps_exhausted_winning_tiles() {
    let own = hand(&[0, 1, 2, 9, 10, 11, 18, 19, 20, 21, 22, 23, 27, 28]);
    let mut state = state_with_player_hand(own);
    let mut players = state.players().clone();
    players[1] = PlayerState::new(
        hand(&[31; 13]),
        25_000,
        vec![Discard::new(tile(27), false, false, false)],
    );
    players[2] = PlayerState::new(
        hand(&[32; 13]),
        25_000,
        vec![Discard::new(tile(27), false, false, false)],
    );
    state = RoundState::new(
        players,
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        0,
        vec![tile(27)],
        60,
        RoundPhase::Initial,
    );

    let result = discard_efficiency(&state, player(0), tile(28)).unwrap();
    assert_eq!(result.shanten, 0);
    assert_eq!(result.total_unseen, 0);
    assert_eq!(
        result.candidates,
        DrawCandidates::Winning(vec![kyoku::analysis::TileAvailability {
            kind: kind(27),
            unseen: 0,
        }])
    );
}

#[test]
fn discard_efficiency_reports_effective_tiles_and_their_availability() {
    let own = hand(&[0, 1, 2, 4, 9, 10, 11, 18, 19, 20, 27, 27, 28, 29]);
    let mut players = state_with_player_hand(own).players().clone();
    players[1] = PlayerState::new(
        hand(&[31; 13]),
        25_000,
        vec![Discard::new(tile(5), false, false, false)],
    );
    let state = RoundState::new(
        players,
        RoundId::new(Wind::East, 1).unwrap(),
        0,
        0,
        vec![tile(3)],
        60,
        RoundPhase::Initial,
    );

    let result = discard_efficiency(&state, player(0), tile(29)).unwrap();

    assert_eq!(result.shanten, 1);
    assert_eq!(
        result.candidates,
        DrawCandidates::Effective(vec![
            kyoku::analysis::TileAvailability {
                kind: kind(2),
                unseen: 3,
            },
            kyoku::analysis::TileAvailability {
                kind: kind(3),
                unseen: 3,
            },
            kyoku::analysis::TileAvailability {
                kind: kind(4),
                unseen: 3,
            },
            kyoku::analysis::TileAvailability {
                kind: kind(5),
                unseen: 3,
            },
            kyoku::analysis::TileAvailability {
                kind: kind(6),
                unseen: 4,
            },
            kyoku::analysis::TileAvailability {
                kind: kind(27),
                unseen: 2,
            },
            kyoku::analysis::TileAvailability {
                kind: kind(28),
                unseen: 3,
            },
        ])
    );
    assert_eq!(result.total_unseen, 21);
}

#[test]
fn discard_efficiency_rejects_an_absent_tile() {
    let state = state_with_player_hand(hand(&[0, 1, 2, 9, 10, 11, 18, 19, 20, 21, 22, 23, 27, 28]));

    assert_eq!(
        discard_efficiency(&state, player(0), tile(33)),
        Err(AnalysisError::DiscardNotFound { discard: tile(33) })
    );
}

#[test]
fn discard_efficiency_requires_a_fourteen_tile_hand() {
    let state = state_with_player_hand(hand(&[0, 1, 2, 9, 10, 11, 18, 19, 20, 21, 22, 23, 27]));

    assert_eq!(
        discard_efficiency(&state, player(0), tile(27)),
        Err(AnalysisError::InvalidHandSize {
            expected: 14,
            actual: 13,
        })
    );
}

#[test]
fn batch_discards_deduplicate_normal_tiles_but_keep_red_fives_separate() {
    let state = state_with_player_hand(hand(&[0, 0, 0, 1, 2, 3, 4, 34, 9, 10, 11, 18, 19, 20]));
    let results = discard_efficiencies(&state, player(0)).unwrap();

    assert_eq!(
        results
            .iter()
            .filter(|result| result.discard == tile(0))
            .count(),
        1
    );
    assert!(results.iter().any(|result| result.discard == tile(4)));
    assert!(results.iter().any(|result| result.discard == tile(34)));
    assert_eq!(results.len(), 12);
}
