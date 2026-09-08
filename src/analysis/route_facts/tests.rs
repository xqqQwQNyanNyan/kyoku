use super::*;

fn hand(tiles: &[u8]) -> Hand {
    Hand::new(
        tiles.iter().map(|&tile| Tile::new(tile).unwrap()).collect(),
        vec![],
    )
    .unwrap()
}

fn unseen(hand: &Hand) -> [u8; 34] {
    super::super::visible_hand::unseen_tiles(hand, &[]).unwrap()
}

#[test]
fn exhausted_honor_triplets_change_distance_even_when_numbered_draws_remain() {
    let hand = hand(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 27, 27, 31, 31]);
    let mut unseen = unseen(&hand);
    unseen[27..].fill(0);
    let result = analyze(&hand, &unseen, Yaku::Honitsu, false).unwrap();
    assert_eq!(result.distance.shape, Some(0));
    assert_eq!(result.distance.available, Some(2));
    assert!(result.progression.is_none());
}

#[test]
fn honitsu_needs_an_honor_including_its_chiitoitsu_family() {
    let hand = hand(&[0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6]);
    let mut unseen = unseen(&hand);
    unseen[27..].fill(0);
    let result = analyze(&hand, &unseen, Yaku::Honitsu, true).unwrap();
    assert!(result.distance.shape.is_some());
    assert_eq!(result.distance.available, None);
    assert!(result.progression.unwrap().is_empty());
    assert!(
        analyze(&hand, &unseen, Yaku::Chinitsu, false)
            .unwrap()
            .distance
            .available
            .is_some()
    );
}

#[test]
fn kokushi_missing_kind_and_chiitoitsu_missing_pairs_are_unavailable() {
    let hand = hand(&[0, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 32]);
    let mut unseen = unseen(&hand);
    unseen[33] = 0;
    assert_eq!(
        analyze(&hand, &unseen, Yaku::Kokushi, false)
            .unwrap()
            .distance
            .available,
        None
    );
    unseen.fill(0);
    assert_eq!(
        analyze(&hand, &unseen, Yaku::Chiitoitsu, false)
            .unwrap()
            .distance
            .available,
        None
    );
}

#[test]
fn progression_preserves_each_actual_discard_and_consumes_drawn_copy() {
    let hand = hand(&[0, 1, 2, 3, 4, 34, 9, 11, 18, 19, 20, 27, 27]);
    let unseen = unseen(&hand);
    let report = analyze(&hand, &unseen, Yaku::Ittsu, true).unwrap();
    let distance = report.distance.available.unwrap();
    let mut saw_progress = false;
    for draw in report.progression.unwrap() {
        assert_eq!(draw.unseen, unseen[draw.tile.as_u8() as usize]);
        let mut drawn = hand.clone();
        drawn.draw(Tile::new(draw.tile.as_u8()).unwrap()).unwrap();
        let mut remaining = unseen;
        remaining[draw.tile.as_u8() as usize] -= 1;
        for choice in draw.discards {
            saw_progress = true;
            assert!(choice.distance < distance);
            assert_eq!(choice.ordinary.discard, choice.tile);
            let mut after = drawn.clone();
            after.discard(choice.tile).unwrap();
            let checked = analyze(&after, &remaining, Yaku::Ittsu, false).unwrap();
            assert_eq!(checked.distance.available, Some(choice.distance));
        }
    }
    assert!(saw_progress);
}

#[test]
fn fixed_meld_copies_cannot_supply_the_concealed_pair() {
    use crate::mahjong::{meld::Meld, player_index::PlayerIndex};
    let east = Tile::new(27).unwrap();
    let hand = Hand::new(
        [27, 31, 31, 31, 32, 32, 32, 33, 33, 33]
            .map(|tile| Tile::new(tile).unwrap())
            .to_vec(),
        vec![Meld::Pon {
            tiles: [east; 3],
            called: east,
            from: PlayerIndex::new(1).unwrap(),
        }],
    )
    .unwrap();
    let mut unseen = [0; 34];
    unseen[30] = 2;
    let result = analyze(&hand, &unseen, Yaku::Tsuuiisou, false).unwrap();
    assert_eq!(result.distance.shape, Some(0));
    assert_eq!(result.distance.available, Some(1));
    unseen[27] = 1;
    assert!(matches!(
        analyze(&hand, &unseen, Yaku::Tsuuiisou, false),
        Err(RouteError::InvalidAvailability { .. })
    ));
}

#[test]
fn invalid_counts_and_unsupported_routes_are_distinct_errors() {
    let hand = hand(&[0, 1, 2, 3, 4, 5, 9, 10, 11, 18, 19, 20, 27]);
    assert!(matches!(
        analyze(&hand, &[4; 34], Yaku::Honitsu, false),
        Err(RouteError::InvalidAvailability { .. })
    ));
    assert!(matches!(
        analyze(&hand, &unseen(&hand), Yaku::Pinfu, false),
        Err(RouteError::Yaku(_))
    ));
}
