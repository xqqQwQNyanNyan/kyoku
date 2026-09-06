use kyoku::analysis::agari::{
    AgariError, AgariGroup, AgariPattern, WinningPosition, interpretations, patterns,
};
use kyoku::analysis::{AgariContext, RiichiStatus, RonSource, WinMethod, detect_yaku};
use kyoku::mahjong::hand::Hand;
use kyoku::mahjong::round::Wind;
use kyoku::mahjong::tile::{Tile, TileKind};

fn kind(tile: u8) -> TileKind {
    TileKind::new(tile).unwrap()
}

fn sequence(start: u8, open: bool) -> AgariGroup {
    AgariGroup::Sequence {
        start: kind(start),
        open,
    }
}

fn standard(groups: [AgariGroup; 4], pair: u8) -> AgariPattern {
    AgariPattern::Standard {
        groups: groups.to_vec(),
        pair: kind(pair),
    }
}

#[test]
fn equal_yaku_do_not_merge_edge_and_ryanmen_interpretations() {
    let hand = Hand::new(
        [0, 1, 2, 2, 3, 4, 10, 11, 12, 21, 22, 23, 31, 31]
            .map(|tile| Tile::new(tile).unwrap())
            .to_vec(),
        vec![],
    )
    .unwrap();
    let patterns = patterns(&hand);
    assert_eq!(patterns.len(), 1);
    let pattern = &patterns[0];
    let interpretations = interpretations(pattern, kind(2)).unwrap();
    assert_eq!(interpretations.len(), 2);
    let context = AgariContext {
        winning_tile: kind(2),
        win_method: WinMethod::Ron(RonSource::Discard),
        round_wind: Wind::East,
        seat_wind: Wind::South,
        riichi: RiichiStatus::None,
    };
    for (interpretation, index, start) in [(&interpretations[0], 0, 0), (&interpretations[1], 1, 2)]
    {
        assert!(std::ptr::eq(interpretation.pattern(), pattern));
        assert_eq!(interpretation.winning_tile(), kind(2));
        assert_eq!(
            interpretation.winning_position(),
            WinningPosition::Group(index)
        );
        let AgariPattern::Standard { groups, .. } = interpretation.pattern() else {
            panic!("expected standard pattern");
        };
        // 保留 123m 的末张与 345m 的首张位置，后续算符才能分别判定边张和两面。
        assert_eq!(groups[index], sequence(start, false));
        assert_eq!(detect_yaku(interpretation, &context).unwrap(), vec![]);
    }
    assert_ne!(interpretations[0], interpretations[1]);
}

#[test]
fn identical_sequences_keep_distinct_original_group_indices() {
    let pattern = standard([sequence(1, false); 4], 13);
    let interpretations = interpretations(&pattern, kind(1)).unwrap();
    assert_eq!(interpretations.len(), 4);
    for (index, interpretation) in interpretations.iter().enumerate() {
        assert_eq!(
            interpretation.winning_position(),
            WinningPosition::Group(index)
        );
    }
}

#[test]
fn pair_and_group_positions_remain_distinct() {
    let pattern = standard(
        [
            sequence(1, false),
            sequence(4, false),
            sequence(10, false),
            sequence(21, false),
        ],
        1,
    );
    let interpretations = interpretations(&pattern, kind(1)).unwrap();
    assert_eq!(interpretations.len(), 2);
    assert_eq!(interpretations[0].winning_position(), WinningPosition::Pair);
    assert_eq!(
        interpretations[1].winning_position(),
        WinningPosition::Group(0)
    );
}

#[test]
fn fixed_groups_are_skipped_without_changing_group_indices() {
    for completing in [
        sequence(2, false),
        AgariGroup::Triplet {
            tile: kind(2),
            open: false,
        },
    ] {
        let pattern = standard(
            [
                sequence(0, true),
                completing,
                sequence(12, false),
                AgariGroup::Kan {
                    tile: kind(20),
                    open: false,
                },
            ],
            31,
        );
        let before = pattern.clone();
        let interpretations = interpretations(&pattern, kind(2)).unwrap();
        assert_eq!(interpretations.len(), 1);
        assert_eq!(
            interpretations[0].winning_position(),
            WinningPosition::Group(1)
        );
        assert_eq!(pattern, before);
    }
}

#[test]
fn absent_tiles_and_tiles_only_in_fixed_groups_have_no_interpretation() {
    for first in [
        sequence(3, false),
        sequence(0, true),
        AgariGroup::Triplet {
            tile: kind(0),
            open: true,
        },
        AgariGroup::Kan {
            tile: kind(0),
            open: false,
        },
        AgariGroup::Kan {
            tile: kind(0),
            open: true,
        },
    ] {
        let pattern = standard(
            [
                first,
                sequence(9, false),
                sequence(12, false),
                sequence(21, false),
            ],
            31,
        );
        assert_eq!(
            interpretations(&pattern, kind(0)),
            Err(AgariError::WinningTileMismatch {
                winning_tile: kind(0)
            }),
        );
    }
}

#[test]
fn special_patterns_keep_their_kind_and_winning_tile() {
    let pairs = [0, 3, 5, 10, 12, 19, 21];
    let chiitoitsu = AgariPattern::Chiitoitsu {
        pairs: pairs.map(kind).to_vec(),
    };
    for tile in pairs {
        let interpretations = interpretations(&chiitoitsu, kind(tile)).unwrap();
        assert_eq!(interpretations.len(), 1);
        assert_eq!(
            interpretations[0].winning_position(),
            WinningPosition::Chiitoitsu
        );
        assert_eq!(interpretations[0].winning_tile(), kind(tile));
    }
    let kokushi = AgariPattern::Kokushi { pair: kind(0) };
    for tile in [0, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 33] {
        let interpretations = interpretations(&kokushi, kind(tile)).unwrap();
        assert_eq!(interpretations.len(), 1);
        assert_eq!(
            interpretations[0].winning_position(),
            WinningPosition::Kokushi
        );
        assert_eq!(interpretations[0].winning_tile(), kind(tile));
        assert_eq!(interpretations[0].pattern(), &kokushi);
    }
    for pattern in [chiitoitsu, kokushi] {
        assert_eq!(
            interpretations(&pattern, kind(1)),
            Err(AgariError::WinningTileMismatch {
                winning_tile: kind(1)
            }),
        );
    }
}
