use kyoku::analysis::agari::{AgariPattern, patterns};
use kyoku::mahjong::hand::Hand;
use kyoku::mahjong::tile::Tile;

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
