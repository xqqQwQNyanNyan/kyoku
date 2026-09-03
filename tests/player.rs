use kyoku::mahjong::hand::Hand;
use kyoku::mahjong::player::{Discard, PlayerState};
use kyoku::mahjong::tile::Tile;

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
