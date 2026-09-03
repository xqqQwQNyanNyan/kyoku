use kyoku::mahjong::hand::Hand;
use kyoku::mahjong::player::{Discard, PlayerIndex, PlayerState};
use kyoku::mahjong::tile::Tile;

fn player(value: u8) -> PlayerIndex {
    PlayerIndex::new(value).expect("test player index must be valid")
}

fn tile(value: u8) -> Tile {
    Tile::new(value).expect("test tile must be valid")
}

fn hand() -> Hand {
    Hand::new(vec![tile(0); 13], vec![]).expect("test hand must be valid")
}

#[test]
fn discard_exposes_its_tile_and_flags() {
    let discard = Discard::new(tile(34), true, true, true);

    assert_eq!(discard.tile(), tile(34));
    assert!(discard.is_tsumogiri());
    assert!(discard.is_riichi());
    assert!(discard.is_called());

    let ordinary_discard = Discard::new(tile(0), false, false, false);
    assert!(!ordinary_discard.is_tsumogiri());
    assert!(!ordinary_discard.is_riichi());
    assert!(!ordinary_discard.is_called());
}

#[test]
fn player_state_exposes_hand_score_and_discard_order() {
    let hand = hand();
    let first = Discard::new(tile(0), false, false, true);
    let second = Discard::new(tile(1), true, true, false);
    let state = PlayerState::new(hand.clone(), -1_000, vec![first, second]);

    assert_eq!(state.hand(), &hand);
    assert_eq!(state.score(), -1_000);
    assert_eq!(state.discards(), [first, second]);
}

#[test]
fn constructor_accepts_only_four_player_indices() {
    for value in 0..=3 {
        assert_eq!(PlayerIndex::new(value), Some(player(value)));
    }

    assert_eq!(PlayerIndex::new(4), None);
    assert_eq!(PlayerIndex::new(u8::MAX), None);
}

#[test]
fn integer_conversion_reports_the_invalid_value() {
    assert_eq!(PlayerIndex::try_from(3), Ok(player(3)));

    let error = PlayerIndex::try_from(4).unwrap_err();
    assert_eq!(error.value(), 4);
    assert_eq!(error.to_string(), "invalid player index 4; expected 0..=3");
}

#[test]
fn get_id_returns_the_original_value() {
    for value in 0..=3 {
        assert_eq!(player(value).get_id(), value);
    }
}
