use kyoku::mahjong::hand::Hand;
use kyoku::mahjong::player::{Discard, PlayerRiichiError, PlayerState, RiichiState};
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
    assert_eq!(state.riichi(), RiichiState::NotDeclared);
}

#[test]
fn draw_and_discard_keep_the_hand_and_river_in_sync() {
    let mut state = PlayerState::new(hand(), 25_000, vec![]);

    state.draw(tile(1)).unwrap();
    assert_eq!(state.hand().effective_tile_count(), 14);

    state.discard(tile(1), true).unwrap();
    assert_eq!(state.hand().effective_tile_count(), 13);
    assert_eq!(state.discards().len(), 1);
    assert_eq!(state.discards()[0].tile(), tile(1));
    assert!(state.discards()[0].is_tsumogiri());
    assert!(!state.discards()[0].is_riichi());
}

#[test]
fn declaring_riichi_changes_only_the_player_state() {
    let mut state = PlayerState::new(hand(), 25_000, vec![]);
    state.declare_riichi().unwrap();

    assert_eq!(state.riichi(), RiichiState::Declared);
    assert_eq!(state.score(), 25_000);
}

#[test]
fn repeated_player_riichi_declaration_leaves_the_state_unchanged() {
    let mut state = PlayerState::new(hand(), 25_000, vec![]);
    state.declare_riichi().unwrap();
    let declared = state.clone();
    assert_eq!(
        state.declare_riichi(),
        Err(PlayerRiichiError::InvalidDeclaration {
            state: RiichiState::Declared,
        })
    );
    assert_eq!(state, declared);
}

#[test]
fn player_with_fewer_than_1000_points_cannot_declare_riichi() {
    let mut state = PlayerState::new(hand(), 999, vec![]);
    let original = state.clone();
    assert_eq!(
        state.declare_riichi(),
        Err(PlayerRiichiError::InsufficientPoints { score: 999 })
    );
    assert_eq!(state, original);

    let mut exact = PlayerState::new(hand(), 1_000, vec![]);
    exact.declare_riichi().unwrap();
    assert_eq!(exact.riichi(), RiichiState::Declared);
    assert_eq!(exact.score(), 1_000);
}
