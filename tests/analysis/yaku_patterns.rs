use kyoku::analysis::agari::{AgariGroup, AgariPattern, patterns};
use kyoku::mahjong::hand::Hand;
use kyoku::mahjong::meld::Meld;
use kyoku::mahjong::tile::{Tile, TileKind};

fn hand(tiles: &[u8]) -> Hand {
    Hand::new(
        tiles.iter().map(|&tile| Tile::new(tile).unwrap()).collect(),
        vec![],
    )
    .unwrap()
}

#[test]
fn ambiguous_standard_hand_keeps_multiple_patterns() {
    let patterns: Vec<_> = patterns(&hand(&[0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6]))
        .into_iter()
        .filter(|pattern| matches!(pattern, AgariPattern::Standard { .. }))
        .collect();
    assert!(patterns.len() > 1);
    assert!(
        patterns
            .iter()
            .all(|pattern| matches!(pattern, AgariPattern::Standard { .. }))
    );
}

#[test]
fn special_patterns_are_included() {
    let chiitoitsu = patterns(&hand(&[0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6]));
    assert!(
        chiitoitsu
            .iter()
            .any(|pattern| matches!(pattern, AgariPattern::Chiitoitsu { .. }))
    );

    let kokushi = patterns(&hand(&[0, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 33, 0]));
    assert!(
        kokushi
            .iter()
            .any(|pattern| matches!(pattern, AgariPattern::Kokushi { .. }))
    );
}

#[test]
fn duplicate_patterns_are_removed() {
    let result = patterns(&hand(&[0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4]));
    let unique = result.iter().collect::<std::collections::HashSet<_>>();
    assert_eq!(result.len(), unique.len());
}

#[test]
fn pair_can_be_completed_after_all_four_groups() {
    for pair in [25, 27, 31, 33] {
        let mut tiles = vec![0, 1, 2, 2, 3, 4, 10, 11, 12, 21, 22, 23];
        tiles.extend([pair, pair]);
        let result = patterns(&hand(&tiles));
        assert_eq!(
            result,
            vec![AgariPattern::Standard {
                groups: [0, 2, 10, 21]
                    .map(|start| AgariGroup::Sequence {
                        start: TileKind::new(start).unwrap(),
                        open: false,
                    })
                    .to_vec(),
                pair: TileKind::new(pair).unwrap(),
            }]
        );
    }
    assert!(patterns(&hand(&[0, 1, 2, 2, 3, 4, 10, 11, 12, 21, 22, 23, 31])).is_empty());
    assert!(patterns(&hand(&[0, 1, 2, 2, 3, 4, 10, 11, 12, 21, 22, 23, 31, 33])).is_empty());
}

#[test]
fn four_fixed_kans_still_allow_the_remaining_pair() {
    let melds: Vec<_> = [0, 9, 18, 27]
        .map(|tile| Meld::Ankan {
            tiles: [Tile::new(tile).unwrap(); 4],
        })
        .to_vec();
    let completed = Hand::new(vec![Tile::new(31).unwrap(); 2], melds.clone()).unwrap();
    let result = patterns(&completed);
    assert_eq!(result.len(), 1);
    assert!(matches!(&result[0], AgariPattern::Standard { groups, pair }
        if groups.len() == 4 && pair.as_u8() == 31));
    let waiting = Hand::new(vec![Tile::new(31).unwrap()], melds).unwrap();
    assert!(patterns(&waiting).is_empty());
}
