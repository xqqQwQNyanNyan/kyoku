use kyoku::mahjong::hand::{Hand, HandMutationError};
use kyoku::mahjong::meld::Meld;
use kyoku::mahjong::player_index::PlayerIndex;
use kyoku::mahjong::tile::Tile;

fn tile(value: u8) -> Tile {
    Tile::new(value).expect("test tile must be valid")
}

fn player(value: u8) -> PlayerIndex {
    PlayerIndex::new(value).expect("test player index must be valid")
}

fn pon(value: u8) -> Meld {
    Meld::Pon {
        tiles: [tile(value); 3],
        called: tile(value),
        from: player(1),
    }
}

#[test]
fn constructor_accepts_thirteen_and_fourteen_effective_tiles() {
    let thirteen = Hand::new(vec![tile(0); 13], vec![]).unwrap();
    let fourteen = Hand::new(vec![tile(0); 11], vec![pon(27)]).unwrap();

    assert_eq!(thirteen.effective_tile_count(), 13);
    assert_eq!(fourteen.effective_tile_count(), 14);
}

#[test]
fn constructor_sorts_concealed_tiles_by_domain_order() {
    let hand = Hand::new(
        vec![
            tile(5),
            tile(4),
            tile(34),
            tile(3),
            tile(0),
            tile(0),
            tile(0),
            tile(0),
            tile(0),
            tile(0),
            tile(0),
            tile(0),
            tile(0),
        ],
        vec![],
    )
    .unwrap();

    assert_eq!(
        hand.concealed(),
        [
            tile(0),
            tile(0),
            tile(0),
            tile(0),
            tile(0),
            tile(0),
            tile(0),
            tile(0),
            tile(0),
            tile(3),
            tile(34),
            tile(4),
            tile(5),
        ]
    );
}

#[test]
fn melds_keep_their_input_order() {
    let first = pon(27);
    let second = pon(31);
    let hand = Hand::new(vec![tile(0); 7], vec![first, second]).unwrap();

    assert_eq!(hand.melds(), [first, second]);
}

#[test]
fn kans_count_as_three_effective_tiles() {
    let kan = Meld::Ankan {
        tiles: [tile(31); 4],
    };
    let hand = Hand::new(vec![tile(0); 10], vec![kan]).unwrap();

    assert_eq!(hand.effective_tile_count(), 13);
    assert_eq!(hand.melds()[0].tiles().len(), 4);
}

#[test]
fn constructor_rejects_every_other_effective_size_with_context() {
    for concealed_count in [0, 12, 15] {
        let error = Hand::new(vec![tile(0); concealed_count], vec![]).unwrap_err();

        assert_eq!(error.concealed_count(), concealed_count);
        assert_eq!(error.meld_count(), 0);
        assert_eq!(error.effective_tile_count(), concealed_count);
    }

    let error = Hand::new(vec![tile(0); 6], vec![pon(27), pon(28)]).unwrap_err();
    assert_eq!(error.concealed_count(), 6);
    assert_eq!(error.meld_count(), 2);
    assert_eq!(error.effective_tile_count(), 12);
    assert_eq!(
        error.to_string(),
        "invalid hand size 12; expected 13 or 14 effective tiles (6 concealed, 2 melds)"
    );
}

#[test]
fn constructor_does_not_apply_additional_legality_checks() {
    let hand = Hand::new(vec![tile(0); 13], vec![]).unwrap();

    assert_eq!(hand.concealed(), vec![tile(0); 13]);
}

#[test]
fn draw_and_discard_update_concealed_tiles_in_order() {
    let mut hand = Hand::new(vec![tile(4); 13], vec![]).unwrap();

    hand.draw(tile(34)).unwrap();
    assert_eq!(hand.effective_tile_count(), 14);
    assert_eq!(hand.concealed()[0], tile(34));

    hand.discard(tile(34)).unwrap();
    assert_eq!(hand.effective_tile_count(), 13);
    assert_eq!(hand.concealed(), vec![tile(4); 13]);
}

#[test]
fn mutations_preserve_size_and_tile_membership_invariants() {
    let mut thirteen = Hand::new(vec![tile(0); 13], vec![]).unwrap();
    let error = thirteen.discard(tile(0)).unwrap_err();
    assert!(matches!(error, HandMutationError::InvalidSize(_)));
    assert_eq!(thirteen.effective_tile_count(), 13);

    let mut fourteen = Hand::new(vec![tile(0); 14], vec![]).unwrap();
    let error = fourteen.draw(tile(1)).unwrap_err();
    assert!(matches!(error, HandMutationError::InvalidSize(_)));
    assert_eq!(fourteen.effective_tile_count(), 14);

    let error = fourteen.discard(tile(1)).unwrap_err();
    assert_eq!(error, HandMutationError::TileNotFound { tile: tile(1) });
    assert_eq!(fourteen.concealed(), vec![tile(0); 14]);
}
