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

#[test]
fn chi_and_pon_consume_concealed_tiles_and_append_sorted_melds() {
    let mut chi_hand =
        Hand::new([vec![tile(0), tile(1)], vec![tile(9); 11]].concat(), vec![]).unwrap();
    chi_hand
        .chi(tile(2), player(0), [tile(1), tile(0)])
        .unwrap();
    assert_eq!(chi_hand.concealed(), vec![tile(9); 11]);
    assert_eq!(
        chi_hand.melds(),
        [Meld::Chi {
            tiles: [tile(0), tile(1), tile(2)],
            called: tile(2),
            from: player(0),
        }]
    );

    let mut pon_hand = Hand::new(
        [vec![tile(34), tile(4)], vec![tile(9); 11]].concat(),
        vec![],
    )
    .unwrap();
    pon_hand
        .pon(tile(4), player(3), [tile(4), tile(34)])
        .unwrap();
    assert_eq!(
        pon_hand.melds(),
        [Meld::Pon {
            tiles: [tile(34), tile(4), tile(4)],
            called: tile(4),
            from: player(3),
        }]
    );
}

#[test]
fn invalid_calls_leave_the_hand_unchanged() {
    let mut hand = Hand::new(
        [vec![tile(0), tile(1), tile(2)], vec![tile(9); 10]].concat(),
        vec![],
    )
    .unwrap();
    let original = hand.clone();

    assert!(matches!(
        hand.chi(tile(8), player(0), [tile(0), tile(1)]),
        Err(HandMutationError::InvalidChi { .. })
    ));
    assert_eq!(hand, original);

    assert!(matches!(
        hand.pon(tile(2), player(0), [tile(2), tile(1)]),
        Err(HandMutationError::InvalidPon { .. })
    ));
    assert_eq!(hand, original);

    assert_eq!(
        hand.pon(tile(2), player(0), [tile(2), tile(2)]),
        Err(HandMutationError::TileNotFound { tile: tile(2) })
    );
    assert_eq!(hand, original);
}

#[test]
fn daiminkan_and_ankan_consume_four_matching_tiles() {
    let mut daiminkan = Hand::new(
        [vec![tile(34), tile(4), tile(4)], vec![tile(9); 10]].concat(),
        vec![],
    )
    .unwrap();
    daiminkan
        .daiminkan(tile(4), player(3), [tile(4), tile(34), tile(4)])
        .unwrap();
    assert_eq!(daiminkan.concealed(), vec![tile(9); 10]);
    assert_eq!(
        daiminkan.melds(),
        [Meld::Daiminkan {
            tiles: [tile(34), tile(4), tile(4), tile(4)],
            called: tile(4),
            from: player(3),
        }]
    );
    assert_eq!(daiminkan.effective_tile_count(), 13);

    let mut ankan = Hand::new([vec![tile(31); 4], vec![tile(9); 10]].concat(), vec![]).unwrap();
    ankan
        .ankan([tile(31), tile(31), tile(31), tile(31)])
        .unwrap();
    assert_eq!(ankan.concealed(), vec![tile(9); 10]);
    assert_eq!(
        ankan.melds(),
        [Meld::Ankan {
            tiles: [tile(31); 4]
        }]
    );
    assert_eq!(ankan.effective_tile_count(), 13);
}

#[test]
fn kakan_replaces_the_matching_pon_and_preserves_its_metadata() {
    let chi = Meld::Chi {
        tiles: [tile(0), tile(1), tile(2)],
        called: tile(2),
        from: player(3),
    };
    let original_pon = Meld::Pon {
        tiles: [tile(34), tile(4), tile(4)],
        called: tile(34),
        from: player(1),
    };
    let mut hand = Hand::new(
        [vec![tile(4)], vec![tile(9); 7]].concat(),
        vec![chi, original_pon],
    )
    .unwrap();

    hand.kakan(tile(4), [tile(4), tile(34), tile(4)]).unwrap();

    assert_eq!(hand.concealed(), vec![tile(9); 7]);
    assert_eq!(
        hand.melds(),
        [
            chi,
            Meld::Kakan {
                tiles: [tile(34), tile(4), tile(4), tile(4)],
                called: tile(34),
                from: player(1),
            },
        ]
    );
    assert_eq!(hand.melds().len(), 2);
    assert_eq!(hand.effective_tile_count(), 13);
}

#[test]
fn invalid_kans_leave_the_hand_unchanged() {
    let mut daiminkan = Hand::new([vec![tile(4); 3], vec![tile(9); 10]].concat(), vec![]).unwrap();
    let original = daiminkan.clone();
    assert!(matches!(
        daiminkan.daiminkan(tile(5), player(0), [tile(4); 3]),
        Err(HandMutationError::InvalidDaiminkan { .. })
    ));
    assert_eq!(daiminkan, original);

    let mut ankan = Hand::new([vec![tile(31); 3], vec![tile(9); 11]].concat(), vec![]).unwrap();
    let original = ankan.clone();
    assert_eq!(
        ankan.ankan([tile(31); 4]),
        Err(HandMutationError::TileNotFound { tile: tile(31) })
    );
    assert_eq!(ankan, original);

    let pon = Meld::Pon {
        tiles: [tile(34), tile(4), tile(4)],
        called: tile(34),
        from: player(1),
    };
    let mut kakan = Hand::new([vec![tile(4)], vec![tile(9); 10]].concat(), vec![pon]).unwrap();
    let original = kakan.clone();
    assert_eq!(
        kakan.kakan(tile(4), [tile(4); 3]),
        Err(HandMutationError::PonNotFound {
            tiles: [tile(4); 3]
        })
    );
    assert_eq!(kakan, original);
}
