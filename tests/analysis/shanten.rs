use kyoku::analysis::{
    Yaku, chiitoitsu_shanten, kokushi_shanten, shanten, standard_shanten, yaku_shanten,
};
use kyoku::mahjong::hand::Hand;
use kyoku::mahjong::meld::Meld;
use kyoku::mahjong::player_index::PlayerIndex;
use kyoku::mahjong::tile::Tile;

fn counts(tiles: &[(usize, u8)]) -> [u8; 34] {
    let mut counts = [0; 34];
    for &(tile, count) in tiles {
        counts[tile] = count;
    }
    counts
}

fn hand_from_counts(counts: &[u8; 34]) -> Hand {
    hand_with_melds(counts, Vec::new())
}

fn hand_with_melds(counts: &[u8; 34], melds: Vec<Meld>) -> Hand {
    let concealed = counts
        .iter()
        .enumerate()
        .flat_map(|(tile, &count)| {
            std::iter::repeat_n(
                Tile::new(tile as u8).expect("test tile must be valid"),
                count as usize,
            )
        })
        .collect();

    Hand::new(concealed, melds).expect("test hand must have a valid size")
}

fn chi(tiles: [u8; 3]) -> Meld {
    Meld::Chi {
        tiles: tiles.map(|value| Tile::new(value).expect("test tile must be valid")),
        called: Tile::new(tiles[0]).expect("test tile must be valid"),
        from: PlayerIndex::new(1).expect("test player must be valid"),
    }
}

fn generated_hand(mut seed: u64) -> [u8; 34] {
    let mut hand = [0; 34];
    let mut remaining = 14;

    while remaining > 0 {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let tile = (seed % 34) as usize;
        if hand[tile] < 4 {
            hand[tile] += 1;
            remaining -= 1;
        }
    }

    hand
}

// 重构前的普通型实现，只用于固定新旧算法的回归关系。
fn legacy_standard_shanten(counts: &[u8; 34]) -> i8 {
    const UNREACHABLE: u8 = u8::MAX;
    let mut dp = [[[[[UNREACHABLE; 5]; 2]; 5]; 5]; 35];
    dp[0][0][0][0][0] = 0;

    for tile in 0..34 {
        for a in 0..=4 {
            for b in 0..=4 {
                for pairs in 0..=1 {
                    for melds in 0..=4 {
                        let missing_so_far = dp[tile][a][b][pairs][melds];
                        if missing_so_far == UNREACHABLE {
                            continue;
                        }

                        for pair in 0..=1 {
                            if pairs + pair > 1 {
                                continue;
                            }

                            for triplet in 0..=1 {
                                let max_sequences = if tile < 27 && tile % 9 <= 6 { 4 } else { 0 };
                                for sequence in 0..=max_sequences {
                                    let next_melds = melds + triplet + sequence;
                                    let required = a + 2 * pair + 3 * triplet + sequence;
                                    if next_melds > 4 || required > 4 || b + sequence > 4 {
                                        continue;
                                    }

                                    let missing_here =
                                        required.saturating_sub(counts[tile] as usize) as u8;
                                    let candidate = missing_so_far + missing_here;
                                    let next = &mut dp[tile + 1][b + sequence][sequence]
                                        [pairs + pair][next_melds];
                                    *next = (*next).min(candidate);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    dp[34][0][0][1][4] as i8 - 1
}

// 重构前的对对和实现，只用于固定新旧算法的回归关系。
fn legacy_toitoi_shanten(counts: &[u8; 34]) -> i8 {
    const UNREACHABLE: u8 = u8::MAX;
    let mut dp = [[UNREACHABLE; 5]; 2];
    dp[0][0] = 0;

    for &count in counts {
        let mut next = [[UNREACHABLE; 5]; 2];
        for pairs in 0..=1 {
            for melds in 0..=4 {
                let missing_so_far = dp[pairs][melds];
                if missing_so_far == UNREACHABLE {
                    continue;
                }

                next[pairs][melds] = next[pairs][melds].min(missing_so_far);
                if pairs < 1 {
                    let candidate = missing_so_far + 2u8.saturating_sub(count);
                    next[pairs + 1][melds] = next[pairs + 1][melds].min(candidate);
                }
                if melds < 4 {
                    let candidate = missing_so_far + 3u8.saturating_sub(count);
                    next[pairs][melds + 1] = next[pairs][melds + 1].min(candidate);
                }
            }
        }
        dp = next;
    }

    dp[1][4] as i8 - 1
}

#[test]
fn constrained_standard_matches_pre_refactor_results() {
    for seed in 0..64 {
        let hand = generated_hand(seed);
        assert_eq!(standard_shanten(&hand), legacy_standard_shanten(&hand));
    }
}

#[test]
fn constrained_toitoi_matches_pre_refactor_results() {
    for seed in 0..64 {
        let counts = generated_hand(seed);
        let hand = hand_from_counts(&counts);
        assert_eq!(
            yaku_shanten(&hand, Yaku::Toitoi),
            Some(legacy_toitoi_shanten(&counts))
        );
    }
}

#[test]
fn yaku_api_routes_special_hand_families() {
    let chiitoitsu = counts(&[(0, 2), (2, 2), (4, 2), (9, 2), (11, 2), (13, 2), (27, 2)]);
    assert_eq!(
        yaku_shanten(&hand_from_counts(&chiitoitsu), Yaku::Chiitoitsu),
        Some(chiitoitsu_shanten(&chiitoitsu))
    );

    let kokushi = counts(&[
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
    assert_eq!(
        yaku_shanten(&hand_from_counts(&kokushi), Yaku::Kokushi),
        Some(kokushi_shanten(&kokushi))
    );
}

#[test]
fn completed_ittsu_in_each_suit_is_minus_one() {
    for suit_start in [0, 9, 18] {
        let mut hand = [0; 34];
        hand[suit_start..suit_start + 9].fill(1);
        hand[27] = 3;
        hand[28] = 2;

        assert_eq!(
            yaku_shanten(&hand_from_counts(&hand), Yaku::Ittsu),
            Some(-1)
        );
    }
}

#[test]
fn ittsu_can_pay_the_original_dp_cost_to_complete_a_segment() {
    // 12m 456m 789m 111p 东东，只缺 3m。
    let hand = counts(&[
        (0, 1),
        (1, 1),
        (3, 1),
        (4, 1),
        (5, 1),
        (6, 1),
        (7, 1),
        (8, 1),
        (9, 3),
        (27, 2),
    ]);

    assert_eq!(yaku_shanten(&hand_from_counts(&hand), Yaku::Ittsu), Some(0));
}

#[test]
fn ittsu_segments_cannot_be_split_across_suits() {
    // 123m 456p 789s 东东东 南南是普通完成形，但不是一气通贯。
    let hand = counts(&[
        (0, 1),
        (1, 1),
        (2, 1),
        (12, 1),
        (13, 1),
        (14, 1),
        (24, 1),
        (25, 1),
        (26, 1),
        (27, 3),
        (28, 2),
    ]);

    assert_eq!(standard_shanten(&hand), -1);
    assert!(
        yaku_shanten(&hand_from_counts(&hand), Yaku::Ittsu).is_some_and(|shanten| shanten > -1)
    );
}

#[test]
fn related_chi_advances_ittsu_constraint_state() {
    // 副露 123m，加上暗牌中的 456m、789m、111p、东东。
    let concealed = counts(&[
        (3, 1),
        (4, 1),
        (5, 1),
        (6, 1),
        (7, 1),
        (8, 1),
        (9, 3),
        (27, 2),
    ]);
    let hand = hand_with_melds(&concealed, vec![chi([0, 1, 2])]);

    assert_eq!(yaku_shanten(&hand, Yaku::Ittsu), Some(-1));
}

#[test]
fn unrelated_chi_does_not_advance_another_suits_ittsu_state() {
    // 副露 123p 不能代替缺失的 123m；补成万子一气仍需要三张牌。
    let concealed = counts(&[
        (3, 1),
        (4, 1),
        (5, 1),
        (6, 1),
        (7, 1),
        (8, 1),
        (18, 3),
        (27, 2),
    ]);
    let hand = hand_with_melds(&concealed, vec![chi([9, 10, 11])]);

    assert_eq!(yaku_shanten(&hand, Yaku::Ittsu), Some(2));
}

#[test]
fn completed_iipeikou_in_each_suit_is_minus_one() {
    for suit_start in [0, 9, 18] {
        let hand = counts(&[
            (suit_start, 2),
            (suit_start + 1, 2),
            (suit_start + 2, 2),
            (27, 3),
            (28, 3),
            (29, 2),
        ]);

        assert_eq!(
            yaku_shanten(&hand_from_counts(&hand), Yaku::Iipeikou),
            Some(-1)
        );
    }
}

#[test]
fn iipeikou_can_pay_the_original_dp_cost_for_the_second_sequence() {
    // 123m、12m、东东东、南南南、西西，只缺一张 3m 组成第二个 123m。
    let hand = counts(&[(0, 2), (1, 2), (2, 1), (27, 3), (28, 3), (29, 2)]);

    assert_eq!(
        yaku_shanten(&hand_from_counts(&hand), Yaku::Iipeikou),
        Some(0)
    );
}

#[test]
fn a_single_sequence_is_not_enough_for_iipeikou() {
    // 123m 456m 789p 东东东 南南是普通完成形，但没有重复顺子。
    let hand = counts(&[
        (0, 1),
        (1, 1),
        (2, 1),
        (3, 1),
        (4, 1),
        (5, 1),
        (15, 1),
        (16, 1),
        (17, 1),
        (27, 3),
        (28, 2),
    ]);

    assert_eq!(standard_shanten(&hand), -1);
    assert!(
        yaku_shanten(&hand_from_counts(&hand), Yaku::Iipeikou).is_some_and(|shanten| shanten > -1)
    );
}

#[test]
fn different_sequences_do_not_form_iipeikou() {
    // 123m 234m 东东东 南南南 西西中的两个顺子并不相同。
    let hand = counts(&[(0, 1), (1, 2), (2, 2), (3, 1), (27, 3), (28, 3), (29, 2)]);

    assert_eq!(standard_shanten(&hand), -1);
    assert!(
        yaku_shanten(&hand_from_counts(&hand), Yaku::Iipeikou).is_some_and(|shanten| shanten > -1)
    );
}

#[test]
fn same_ranks_in_different_suits_do_not_form_iipeikou() {
    // 123m 123p 东东东 南南南 西西中的两个顺子花色不同。
    let hand = counts(&[
        (0, 1),
        (1, 1),
        (2, 1),
        (9, 1),
        (10, 1),
        (11, 1),
        (27, 3),
        (28, 3),
        (29, 2),
    ]);

    assert_eq!(standard_shanten(&hand), -1);
    assert!(
        yaku_shanten(&hand_from_counts(&hand), Yaku::Iipeikou).is_some_and(|shanten| shanten > -1)
    );
}

#[test]
fn open_hand_is_ineligible_for_iipeikou() {
    let concealed = counts(&[
        (0, 1),
        (1, 1),
        (2, 1),
        (9, 1),
        (10, 1),
        (11, 1),
        (27, 3),
        (28, 2),
    ]);
    let hand = hand_with_melds(&concealed, vec![chi([18, 19, 20])]);

    assert_eq!(yaku_shanten(&hand, Yaku::Iipeikou), None);
}

#[test]
fn fixed_melds_can_make_a_yaku_unreachable() {
    let concealed = counts(&[
        (3, 1),
        (4, 1),
        (5, 1),
        (6, 1),
        (7, 1),
        (8, 1),
        (18, 3),
        (27, 2),
    ]);
    let hand = hand_with_melds(&concealed, vec![chi([9, 10, 11])]);

    assert_eq!(yaku_shanten(&hand, Yaku::Toitoi), None);
    assert_eq!(yaku_shanten(&hand, Yaku::Chiitoitsu), None);
    assert_eq!(yaku_shanten(&hand, Yaku::Kokushi), None);
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
fn completed_chinitsu_is_minus_one() {
    // 123m 123m 456m 789m 55m。
    let hand = counts(&[
        (0, 2),
        (1, 2),
        (2, 2),
        (3, 1),
        (4, 3),
        (5, 1),
        (6, 1),
        (7, 1),
        (8, 1),
    ]);

    assert_eq!(
        yaku_shanten(&hand_from_counts(&hand), Yaku::Chinitsu),
        Some(-1)
    );
}

#[test]
fn completed_chinitsu_chiitoitsu_in_each_suit_is_minus_one() {
    for suit_start in [0, 9, 18] {
        let mut hand = [0; 34];
        for rank in [0, 1, 2, 3, 5, 7, 8] {
            hand[suit_start + rank] = 2;
        }

        assert_eq!(
            yaku_shanten(&hand_from_counts(&hand), Yaku::Chinitsu),
            Some(-1)
        );
    }
}

#[test]
fn chinitsu_chiitoitsu_ready_hand_is_zero() {
    let hand = counts(&[(0, 2), (1, 2), (2, 2), (3, 2), (5, 2), (7, 2), (8, 1)]);

    assert_eq!(
        yaku_shanten(&hand_from_counts(&hand), Yaku::Chinitsu),
        Some(0)
    );
}

#[test]
fn off_suit_pair_increases_chinitsu_chiitoitsu_distance() {
    let hand = counts(&[(0, 2), (1, 2), (2, 2), (3, 2), (5, 2), (7, 2), (27, 2)]);

    assert_eq!(
        yaku_shanten(&hand_from_counts(&hand), Yaku::Chinitsu),
        Some(1)
    );
}

#[test]
fn open_chinitsu_can_use_ordinary_family() {
    // 副露 123m，暗牌为 111m 456m 789m 22m。
    let concealed = counts(&[
        (0, 3),
        (1, 2),
        (3, 1),
        (4, 1),
        (5, 1),
        (6, 1),
        (7, 1),
        (8, 1),
    ]);
    let hand = hand_with_melds(&concealed, vec![chi([0, 1, 2])]);

    assert_eq!(yaku_shanten(&hand, Yaku::Chinitsu), Some(-1));
}

#[test]
fn chinitsu_ready_hand_is_zero() {
    // 123p 123p 456p 789p 5p，等待 5p。
    let hand = counts(&[
        (9, 2),
        (10, 2),
        (11, 2),
        (12, 1),
        (13, 2),
        (14, 1),
        (15, 1),
        (16, 1),
        (17, 1),
    ]);

    assert_eq!(
        yaku_shanten(&hand_from_counts(&hand), Yaku::Chinitsu),
        Some(0)
    );
}

#[test]
fn off_suit_tiles_do_not_help_chinitsu() {
    // 123m 456m 789m 123p 11s 是普通和牌，但万子清一色仍需补五张牌。
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
        (9, 1),
        (10, 1),
        (11, 1),
        (18, 2),
    ]);

    assert_eq!(standard_shanten(&hand), -1);
    assert_eq!(
        yaku_shanten(&hand_from_counts(&hand), Yaku::Chinitsu),
        Some(4)
    );
}

#[test]
fn chinitsu_chooses_the_closest_suit() {
    // 筒子已有四个面子；万子和索子各只有一张，筒子目标最近。
    let hand = counts(&[
        (4, 1),
        (9, 2),
        (10, 2),
        (11, 2),
        (12, 1),
        (13, 1),
        (14, 1),
        (15, 1),
        (16, 1),
        (17, 1),
        (22, 1),
    ]);

    assert_eq!(
        yaku_shanten(&hand_from_counts(&hand), Yaku::Chinitsu),
        Some(1)
    );
}

#[test]
fn honors_do_not_help_chinitsu() {
    // 123m 456m 789m 东东东 南南 是普通和牌，万子清一色仍需补五张牌。
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
        (27, 3),
        (28, 2),
    ]);

    assert_eq!(standard_shanten(&hand), -1);
    assert_eq!(
        yaku_shanten(&hand_from_counts(&hand), Yaku::Chinitsu),
        Some(4)
    );
}

#[test]
fn completed_toitoi_is_minus_one() {
    let hand = counts(&[(0, 3), (8, 3), (9, 3), (26, 3), (27, 2)]);

    assert_eq!(
        yaku_shanten(&hand_from_counts(&hand), Yaku::Toitoi),
        Some(-1)
    );
}

#[test]
fn toitoi_ready_hand_is_zero() {
    let hand = counts(&[(0, 3), (8, 3), (9, 3), (26, 3), (27, 1)]);

    assert_eq!(
        yaku_shanten(&hand_from_counts(&hand), Yaku::Toitoi),
        Some(0)
    );
}

#[test]
fn toitoi_one_away_hand_is_one() {
    let hand = counts(&[(0, 3), (8, 3), (9, 3), (26, 2), (27, 1), (28, 1)]);

    assert_eq!(
        yaku_shanten(&hand_from_counts(&hand), Yaku::Toitoi),
        Some(1)
    );
}

#[test]
fn four_identical_tiles_cannot_be_both_toitoi_triplet_and_pair() {
    let hand = counts(&[(0, 4), (8, 3), (9, 3), (26, 3)]);

    assert_eq!(
        yaku_shanten(&hand_from_counts(&hand), Yaku::Toitoi),
        Some(1)
    );
}

#[test]
fn sequences_cannot_be_toitoi_target_melds() {
    // 123m 456m 789m 123p 11s 只在允许顺子时是完成形。
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
        (9, 1),
        (10, 1),
        (11, 1),
        (18, 2),
    ]);

    assert_eq!(standard_shanten(&hand), -1);
    assert!(yaku_shanten(&hand_from_counts(&hand), Yaku::Toitoi) > Some(-1));
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
