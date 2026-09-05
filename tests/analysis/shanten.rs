use kyoku::analysis::{chiitoitsu_shanten, kokushi_shanten, shanten, standard_shanten};

fn counts(tiles: &[(usize, u8)]) -> [u8; 34] {
    let mut counts = [0; 34];
    for &(tile, count) in tiles {
        counts[tile] = count;
    }
    counts
}

#[test]
fn completed_hand_is_minus_one() {
    let hand = counts(&[
        (0, 1),
        (1, 1),
        (2, 1),
        (3, 1),
        (4, 1),
        (5, 1),
        (6, 1),
        (7, 1),
        (8, 1),
        (9, 3),
        (27, 2),
    ]);

    assert_eq!(standard_shanten(&hand), -1);
}

#[test]
fn ready_hand_is_zero() {
    let hand = counts(&[
        (0, 1),
        (1, 1),
        (2, 1),
        (3, 1),
        (4, 1),
        (5, 1),
        (6, 1),
        (7, 1),
        (8, 1),
        (9, 3),
        (27, 1),
    ]);

    assert_eq!(standard_shanten(&hand), 0);
}

#[test]
fn one_away_hand_is_one() {
    let hand = counts(&[
        (0, 1),
        (1, 1),
        (2, 1),
        (3, 1),
        (4, 1),
        (5, 1),
        (6, 1),
        (7, 1),
        (8, 1),
        (9, 2),
        (27, 1),
        (28, 1),
    ]);

    assert_eq!(standard_shanten(&hand), 1);
}

#[test]
fn triplet_hand_is_complete() {
    let hand = counts(&[(0, 3), (8, 3), (9, 3), (26, 3), (27, 2)]);

    assert_eq!(standard_shanten(&hand), -1);
}

#[test]
fn repeated_sequences_are_allowed() {
    let hand = counts(&[
        (0, 2),
        (1, 2),
        (2, 2),
        (12, 1),
        (13, 1),
        (14, 1),
        (24, 1),
        (25, 1),
        (26, 1),
        (27, 2),
    ]);

    assert_eq!(standard_shanten(&hand), -1);
}

#[test]
fn overlapping_sequences_are_allowed() {
    // 123m 234m 345m 678p 99s。
    let hand = counts(&[
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 2),
        (4, 1),
        (14, 1),
        (15, 1),
        (16, 1),
        (26, 2),
    ]);

    assert_eq!(standard_shanten(&hand), -1);
}

#[test]
fn honors_cannot_form_sequences() {
    // 123m 456m 789m、东南西、北北。三张不同的字牌不能组成第四个面子。
    let hand = counts(&[
        (0, 1),
        (1, 1),
        (2, 1),
        (3, 1),
        (4, 1),
        (5, 1),
        (6, 1),
        (7, 1),
        (8, 1),
        (27, 1),
        (28, 1),
        (29, 1),
        (30, 2),
    ]);

    assert_eq!(standard_shanten(&hand), 1);
}

#[test]
fn sequences_cannot_cross_suit_boundaries() {
    // 123s 456s 789s、8m 9m 1p、东东。万子与饼子的边界不能组成第四个面子。
    let hand = counts(&[
        (7, 1),
        (8, 1),
        (9, 1),
        (18, 1),
        (19, 1),
        (20, 1),
        (21, 1),
        (22, 1),
        (23, 1),
        (24, 1),
        (25, 1),
        (26, 1),
        (27, 2),
    ]);

    // 8m 9m 仍缺 7m；如果错误地允许顺子跨到 1p，这手牌就会被判为已和牌。
    assert_eq!(standard_shanten(&hand), 0);
}

#[test]
fn completed_chiitoitsu_is_minus_one() {
    let hand = counts(&[(0, 2), (2, 2), (4, 2), (9, 2), (11, 2), (13, 2), (27, 2)]);

    assert_eq!(chiitoitsu_shanten(&hand), -1);
}

#[test]
fn chiitoitsu_ready_hand_is_zero() {
    let hand = counts(&[(0, 2), (2, 2), (4, 2), (9, 2), (11, 2), (13, 2), (27, 1)]);

    assert_eq!(chiitoitsu_shanten(&hand), 0);
}

#[test]
fn chiitoitsu_one_away_hand_is_one() {
    let hand = counts(&[
        (0, 2),
        (2, 2),
        (4, 2),
        (9, 2),
        (11, 2),
        (13, 1),
        (27, 1),
        (28, 1),
    ]);

    assert_eq!(chiitoitsu_shanten(&hand), 1);
}

#[test]
fn four_identical_tiles_are_only_one_chiitoitsu_pair() {
    let hand = counts(&[(0, 4), (2, 2), (4, 2), (9, 2), (11, 2), (13, 2)]);

    assert_eq!(chiitoitsu_shanten(&hand), 1);
}

#[test]
fn chiitoitsu_penalizes_hands_with_too_few_unique_tiles() {
    let hand = counts(&[(0, 2), (2, 2), (4, 2), (9, 2), (11, 2)]);

    assert_eq!(chiitoitsu_shanten(&hand), 3);
}

#[test]
fn completed_kokushi_is_minus_one() {
    let hand = counts(&[
        (0, 2),
        (8, 1),
        (9, 1),
        (17, 1),
        (18, 1),
        (26, 1),
        (27, 1),
        (28, 1),
        (29, 1),
        (30, 1),
        (31, 1),
        (32, 1),
        (33, 1),
    ]);

    assert_eq!(kokushi_shanten(&hand), -1);
}

#[test]
fn thirteen_sided_kokushi_wait_is_zero() {
    let hand = counts(&[
        (0, 1),
        (8, 1),
        (9, 1),
        (17, 1),
        (18, 1),
        (26, 1),
        (27, 1),
        (28, 1),
        (29, 1),
        (30, 1),
        (31, 1),
        (32, 1),
        (33, 1),
    ]);

    assert_eq!(kokushi_shanten(&hand), 0);
}

#[test]
fn kokushi_with_pair_and_one_missing_kind_is_zero() {
    let hand = counts(&[
        (0, 2),
        (8, 1),
        (9, 1),
        (17, 1),
        (18, 1),
        (26, 1),
        (27, 1),
        (28, 1),
        (29, 1),
        (30, 1),
        (31, 1),
        (32, 1),
    ]);

    assert_eq!(kokushi_shanten(&hand), 0);
}

#[test]
fn kokushi_one_away_hand_is_one() {
    let hand = counts(&[
        (0, 1),
        (8, 1),
        (9, 1),
        (17, 1),
        (18, 1),
        (26, 1),
        (27, 1),
        (28, 1),
        (29, 1),
        (30, 1),
        (31, 1),
        (32, 1),
    ]);

    assert_eq!(kokushi_shanten(&hand), 1);
}

#[test]
fn non_terminal_tiles_do_not_count_toward_kokushi() {
    let hand = counts(&[
        (0, 1),
        (1, 2),
        (8, 1),
        (9, 1),
        (17, 1),
        (18, 1),
        (26, 1),
        (27, 1),
        (28, 1),
        (29, 1),
        (30, 1),
        (31, 1),
        (32, 1),
    ]);

    assert_eq!(kokushi_shanten(&hand), 1);
}

#[test]
fn unified_shanten_prefers_standard_hand() {
    let hand = counts(&[
        (0, 1),
        (1, 1),
        (2, 1),
        (3, 1),
        (4, 1),
        (5, 1),
        (6, 1),
        (7, 1),
        (8, 1),
        (9, 3),
        (27, 2),
    ]);

    assert_eq!(shanten(&hand), standard_shanten(&hand));
    assert!(standard_shanten(&hand) < chiitoitsu_shanten(&hand));
    assert!(standard_shanten(&hand) < kokushi_shanten(&hand));
}

#[test]
fn unified_shanten_prefers_chiitoitsu() {
    let hand = counts(&[(0, 2), (2, 2), (4, 2), (9, 2), (11, 2), (13, 2), (27, 2)]);

    assert_eq!(shanten(&hand), chiitoitsu_shanten(&hand));
    assert!(chiitoitsu_shanten(&hand) < standard_shanten(&hand));
    assert!(chiitoitsu_shanten(&hand) < kokushi_shanten(&hand));
}

#[test]
fn unified_shanten_prefers_kokushi() {
    let hand = counts(&[
        (0, 2),
        (8, 1),
        (9, 1),
        (17, 1),
        (18, 1),
        (26, 1),
        (27, 1),
        (28, 1),
        (29, 1),
        (30, 1),
        (31, 1),
        (32, 1),
        (33, 1),
    ]);

    assert_eq!(shanten(&hand), kokushi_shanten(&hand));
    assert!(kokushi_shanten(&hand) < standard_shanten(&hand));
    assert!(kokushi_shanten(&hand) < chiitoitsu_shanten(&hand));
}
