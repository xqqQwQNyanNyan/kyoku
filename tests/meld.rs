use kyoku::mahjong::meld::Meld;
use kyoku::mahjong::player::PlayerIndex;
use kyoku::mahjong::tile::Tile;

fn tile(value: u8) -> Tile {
    Tile::new(value).expect("test tile must be valid")
}

fn player(value: u8) -> PlayerIndex {
    PlayerIndex::new(value).expect("test player index must be valid")
}

#[test]
fn tiles_returns_caller_sorted_tiles_without_reordering() {
    let chi = Meld::Chi {
        tiles: [tile(3), tile(34), tile(5)],
        called: tile(5),
        from: player(0),
    };
    let pon = Meld::Pon {
        tiles: [tile(34), tile(4), tile(4)],
        called: tile(4),
        from: player(0),
    };

    assert_eq!(chi.tiles(), [tile(3), tile(34), tile(5)]);
    assert_eq!(pon.tiles(), [tile(34), tile(4), tile(4)]);
}

#[test]
fn called_and_from_are_absent_only_for_ankan() {
    let source = player(2);
    let open_melds = [
        Meld::Chi {
            tiles: [tile(0), tile(1), tile(2)],
            called: tile(0),
            from: source,
        },
        Meld::Pon {
            tiles: [tile(27); 3],
            called: tile(27),
            from: source,
        },
        Meld::Daiminkan {
            tiles: [tile(31); 4],
            called: tile(31),
            from: source,
        },
        Meld::Kakan {
            tiles: [tile(4), tile(4), tile(4), tile(34)],
            called: tile(34),
            from: source,
        },
    ];

    for meld in open_melds {
        assert_eq!(
            meld.called(),
            Some(match meld {
                Meld::Chi { called, .. }
                | Meld::Pon { called, .. }
                | Meld::Daiminkan { called, .. }
                | Meld::Kakan { called, .. } => called,
                Meld::Ankan { .. } => unreachable!(),
            })
        );
        assert_eq!(meld.from(), Some(source));
    }

    let ankan = Meld::Ankan {
        tiles: [tile(31); 4],
    };
    assert_eq!(ankan.called(), None);
    assert_eq!(ankan.from(), None);
}

#[test]
fn openness_and_kan_classification_cover_all_variants() {
    let source = player(1);
    let cases = [
        (
            Meld::Chi {
                tiles: [tile(0), tile(1), tile(2)],
                called: tile(1),
                from: source,
            },
            true,
            false,
        ),
        (
            Meld::Pon {
                tiles: [tile(27); 3],
                called: tile(27),
                from: source,
            },
            true,
            false,
        ),
        (
            Meld::Daiminkan {
                tiles: [tile(27); 4],
                called: tile(27),
                from: source,
            },
            true,
            true,
        ),
        (
            Meld::Ankan {
                tiles: [tile(27); 4],
            },
            false,
            true,
        ),
        (
            Meld::Kakan {
                tiles: [tile(27); 4],
                called: tile(27),
                from: source,
            },
            true,
            true,
        ),
    ];

    for (meld, is_open, is_kan) in cases {
        assert_eq!(meld.is_open(), is_open);
        assert_eq!(meld.is_kan(), is_kan);
    }
}

#[test]
fn kakan_keeps_the_original_pon_source() {
    let kakan = Meld::Kakan {
        tiles: [tile(34), tile(4), tile(4), tile(4)],
        called: tile(34),
        from: player(3),
    };

    assert_eq!(kakan.called(), Some(tile(34)));
    assert_eq!(kakan.from(), Some(player(3)));
}
