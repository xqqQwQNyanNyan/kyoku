use super::*;
use crate::analysis::{RiichiStatus, RonSource, WinMethod};

#[test]
fn scores_every_interpretation_before_choosing_highest_payment() {
    // 2m 可归属于顺子或刻子；顺子归属保留三暗刻，达到满贯。
    let hand = Hand::new(
        [1, 1, 1, 10, 10, 10, 19, 19, 19, 1, 2, 3, 13, 13]
            .into_iter()
            .map(|v| Tile::new(v).unwrap())
            .collect(),
        vec![],
    )
    .unwrap();
    let context = AgariContext {
        winning_tile: Tile::new(1).unwrap().kind(),
        win_method: WinMethod::Ron(RonSource::Discard),
        round_wind: Wind::East,
        seat_wind: Wind::South,
        riichi: RiichiStatus::None,
    };
    let values = best_win_values(&hand, &context, &[]).unwrap();
    assert!(!values.is_empty());
    assert!(values.iter().all(|v| total_payment(&v.payments) == 8000));
    assert!(values.iter().all(|v| v.yaku.contains(&Yaku::Sanankou)));
}

#[test]
fn rejects_thirteen_tile_scoring_and_preserves_chiitoitsu_wait() {
    let mut tiles = vec![0, 0, 2, 2, 4, 4, 9, 9, 11, 11, 13, 13, 27];
    let make = |tiles: &[u8]| {
        Hand::new(
            tiles.iter().map(|&v| Tile::new(v).unwrap()).collect(),
            vec![],
        )
        .unwrap()
    };
    let context = AgariContext {
        winning_tile: Tile::new(27).unwrap().kind(),
        win_method: WinMethod::Ron(RonSource::Discard),
        round_wind: Wind::East,
        seat_wind: Wind::South,
        riichi: RiichiStatus::None,
    };
    assert!(matches!(
        best_win_values(&make(&tiles), &context, &[]),
        Err(WinValueError::InvalidHandSize)
    ));
    tiles.push(27);
    let values = best_win_values(&make(&tiles), &context, &[]).unwrap();
    assert_eq!(values[0].value.fu, Some(25));
    assert_eq!(values[0].wait, "chiitoitsu_tanki");
    assert_eq!(total_payment(&values[0].payments), 1600);
}
