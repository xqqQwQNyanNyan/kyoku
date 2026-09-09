use super::*;
use crate::analysis::DrawCandidates;
use crate::mahjong::meld::Meld;

fn tile(value: u8) -> Tile {
    Tile::try_from(value).unwrap()
}
fn kind(value: u8) -> TileKind {
    TileKind::try_from(value).unwrap()
}

fn context() -> ComparisonContext {
    // Issue #9：2p 与 1p、4p 有连接，切 2p、3m、东的直接进张相同。
    ComparisonContext::new(
        Hand::new(
            [2, 6, 7, 9, 9, 10, 12, 13, 24, 25, 26, 27, 33, 33]
                .map(tile)
                .to_vec(),
            vec![],
        )
        .unwrap(),
        &[],
    )
    .unwrap()
}

#[test]
fn issue9_keeps_action_direction_and_overlapping_connections() {
    let comparison = context().compare(tile(10), tile(27), None).unwrap();
    assert!(!comparison.first.concealed.contains(&tile(10)));
    assert!(comparison.first.concealed.contains(&tile(27)));
    assert!(comparison.second.concealed.contains(&tile(10)));
    assert!(!comparison.second.concealed.contains(&tile(27)));
    assert_eq!(comparison.first.efficiency.shanten, 2);
    assert_eq!(
        comparison.first.efficiency.candidates,
        comparison.second.efficiency.candidates
    );
    let two_pin = comparison
        .connections_before
        .iter()
        .find(|c| c.kind == kind(10))
        .unwrap();
    assert_eq!(two_pin.sequence_neighbors, vec![kind(9), kind(12)]);
    let east = comparison
        .connections_before
        .iter()
        .find(|c| c.kind == kind(27))
        .unwrap();
    assert_eq!(east.copies, 1);
    assert!(east.sequence_neighbors.is_empty());
    assert_eq!(
        comparison
            .first
            .concealed
            .iter()
            .filter(|&&t| t == tile(9))
            .count(),
        2
    );
}

#[test]
fn reversing_candidates_reverses_results_and_never_mutates_context() {
    let context = context();
    let original = context.hand.clone();
    let forward = context.compare(tile(10), tile(2), Some(kind(3))).unwrap();
    let reverse = context.compare(tile(2), tile(10), Some(kind(3))).unwrap();
    assert_eq!(forward.first.efficiency, reverse.second.efficiency);
    assert_eq!(
        forward.first.followup.unwrap().next_discards,
        reverse.second.followup.unwrap().next_discards
    );
    assert_eq!(context.hand, original);
}

#[test]
fn discarded_tiles_stay_visible_and_hypothetical_draw_consumes_one_copy() {
    let context = context();
    let comparison = context.compare(tile(10), tile(27), Some(kind(27))).unwrap();
    let followup = comparison.first.followup.unwrap();
    for efficiency in followup.next_discards {
        let candidates = match efficiency.candidates {
            DrawCandidates::Effective(v) | DrawCandidates::Winning(v) => v,
        };
        for candidate in candidates {
            let expected = context.unseen[candidate.kind.as_u8() as usize]
                - u8::from(candidate.kind == kind(27));
            assert_eq!(candidate.unseen, expected);
        }
    }
    // 切东后再摸东、再切 2p，牌形与先切 2p 相同，但东的未见枚数少一张。
    let followup = comparison.second.followup.unwrap();
    let discard_two_pin = followup
        .next_discards
        .iter()
        .find(|d| d.discard == tile(10))
        .unwrap();
    assert_eq!(discard_two_pin.shanten, comparison.first.efficiency.shanten);
}

#[test]
fn red_five_choices_preserve_actual_tiles_but_share_shape_results() {
    let hand = Hand::new(
        [0, 1, 2, 3, 4, 34, 9, 10, 11, 18, 19, 20, 27, 27]
            .map(tile)
            .to_vec(),
        vec![],
    )
    .unwrap();
    let comparison = ComparisonContext::new(hand, &[])
        .unwrap()
        .compare(tile(34), tile(4), None)
        .unwrap();
    assert!(comparison.first.concealed.contains(&tile(4)));
    assert!(!comparison.first.concealed.contains(&tile(34)));
    assert!(comparison.second.concealed.contains(&tile(34)));
    assert_eq!(
        comparison.first.efficiency.candidates,
        comparison.second.efficiency.candidates
    );
}

#[test]
fn exhausted_draws_invalid_counts_and_absent_discards_are_errors() {
    let mut context = context();
    context.unseen[27] = 0;
    assert!(matches!(
        context.compare(tile(10), tile(27), Some(kind(27))),
        Err(ComparisonError::ExhaustedDraw { .. })
    ));
    assert!(matches!(
        context.compare(tile(10), tile(10), None),
        Err(ComparisonError::SameDiscard)
    ));
    assert!(matches!(
        context.compare(tile(0), tile(27), None),
        Err(ComparisonError::Analysis(
            AnalysisError::DiscardNotFound { .. }
        ))
    ));
    let hand = Hand::new(vec![tile(0); 14], vec![]).unwrap();
    assert!(matches!(
        ComparisonContext::new(hand, &[]),
        Err(ComparisonError::TooManyCopies { .. })
    ));
}

#[test]
fn fixed_melds_are_preserved_and_count_the_fourth_kan_tile() {
    let hand = Hand::new(
        [9, 10, 11, 18, 19, 20, 24, 25, 26, 27, 28]
            .map(tile)
            .to_vec(),
        vec![Meld::Ankan {
            tiles: [tile(0); 4],
        }],
    )
    .unwrap();
    let context = ComparisonContext::new(hand, &[]).unwrap();
    assert_eq!(context.unseen[0], 0);
    assert!(matches!(
        context.compare(tile(27), tile(28), Some(kind(0))),
        Err(ComparisonError::ExhaustedDraw { .. })
    ));
    let result = context.compare(tile(27), tile(28), None).unwrap();
    assert_eq!(result.first.concealed.len(), 10);
    assert_eq!(result.first.efficiency.shanten, 0);
    let mut hand = context.hand.clone();
    hand.discard(tile(27)).unwrap();
    assert!(matches!(
        ComparisonContext::new(hand, &[]),
        Err(ComparisonError::Analysis(
            AnalysisError::InvalidHandSize { .. }
        ))
    ));
}

#[test]
fn suit_renaming_preserves_shape_comparisons() {
    let rename = |t: Tile| {
        let value = t.as_u8();
        tile(if value < 9 {
            value + 9
        } else if value < 18 {
            value - 9
        } else {
            value
        })
    };
    let context = context();
    let renamed = ComparisonContext::new(
        Hand::new(
            context
                .hand
                .concealed()
                .iter()
                .copied()
                .map(rename)
                .collect(),
            vec![],
        )
        .unwrap(),
        &[],
    )
    .unwrap();
    let original = context.compare(tile(10), tile(2), None).unwrap();
    let changed = renamed
        .compare(rename(tile(10)), rename(tile(2)), None)
        .unwrap();
    for (original, changed) in [
        (original.first, changed.first),
        (original.second, changed.second),
    ] {
        assert_eq!(original.efficiency.shanten, changed.efficiency.shanten);
        assert_eq!(
            original.efficiency.total_unseen,
            changed.efficiency.total_unseen
        );
        let kinds = |candidates: DrawCandidates| match candidates {
            DrawCandidates::Effective(v) | DrawCandidates::Winning(v) => v,
        };
        let mut expected: Vec<_> = kinds(original.efficiency.candidates)
            .iter()
            .map(|t| (rename(tile(t.kind.as_u8())).kind(), t.unseen))
            .collect();
        expected.sort();
        let actual: Vec<_> = kinds(changed.efficiency.candidates)
            .iter()
            .map(|t| (t.kind, t.unseen))
            .collect();
        assert_eq!(expected, actual);
    }
}

#[test]
fn completed_shape_is_marked_separately_from_discard_options() {
    let hand = Hand::new(
        [0, 1, 2, 9, 10, 11, 18, 19, 20, 24, 25, 26, 27, 28]
            .map(tile)
            .to_vec(),
        vec![],
    )
    .unwrap();
    let comparison = ComparisonContext::new(hand, &[])
        .unwrap()
        .compare(tile(28), tile(27), Some(kind(27)))
        .unwrap();
    assert!(comparison.first.followup.unwrap().completed_shape);
    assert!(!comparison.second.followup.unwrap().completed_shape);
}
