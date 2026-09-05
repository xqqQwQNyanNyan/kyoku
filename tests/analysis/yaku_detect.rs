use kyoku::analysis::agari::{AgariGroup, AgariPattern, interpretations, patterns};
use kyoku::analysis::{
    AgariContext, RiichiStatus, RonSource, TsumoSource, WinMethod, Yaku, YakuDetectionError,
    detect_yaku,
};
use kyoku::mahjong::hand::Hand;
use kyoku::mahjong::round::Wind;
use kyoku::mahjong::tile::{Tile, TileKind};

const RON: WinMethod = WinMethod::Ron(RonSource::Discard);
const TSUMO: WinMethod = WinMethod::Tsumo(TsumoSource::Wall);

fn kind(tile: u8) -> TileKind {
    TileKind::new(tile).unwrap()
}

fn sequence(start: u8) -> AgariGroup {
    AgariGroup::Sequence {
        start: kind(start),
        open: false,
    }
}

fn triplet(tile: u8) -> AgariGroup {
    AgariGroup::Triplet {
        tile: kind(tile),
        open: false,
    }
}

fn kan(tile: u8, open: bool) -> AgariGroup {
    AgariGroup::Kan {
        tile: kind(tile),
        open,
    }
}

fn standard(groups: [AgariGroup; 4], pair: u8) -> AgariPattern {
    AgariPattern::Standard {
        groups: groups.to_vec(),
        pair: kind(pair),
    }
}

fn chiitoitsu(pairs: [u8; 7]) -> AgariPattern {
    AgariPattern::Chiitoitsu {
        pairs: pairs.map(kind).to_vec(),
    }
}

fn assert_yaku(pattern: &AgariPattern, expected: &[Yaku]) {
    // 这些牌型用例统一以雀头牌荣和，等待相关的歧义另有专门测试。
    let winning_tile = match pattern {
        AgariPattern::Standard { pair, .. } | AgariPattern::Kokushi { pair } => *pair,
        AgariPattern::Chiitoitsu { pairs } => pairs[0],
    };
    assert_context_yaku(pattern, context(winning_tile.as_u8(), RON), &[expected]);
}

#[test]
fn chiitoitsu_combines_with_whole_hand_yaku() {
    use Yaku::*;
    for (pairs, expected) in [
        ([1, 3, 5, 10, 12, 19, 21], vec![Chiitoitsu, Tanyao]),
        ([0, 3, 5, 10, 12, 19, 21], vec![Chiitoitsu]),
        ([8, 3, 5, 10, 12, 19, 21], vec![Chiitoitsu]),
        ([27, 3, 5, 10, 12, 19, 21], vec![Chiitoitsu]),
        ([1, 2, 3, 4, 5, 6, 7], vec![Chiitoitsu, Tanyao, Chinitsu]),
        ([0, 1, 2, 3, 4, 5, 8], vec![Chiitoitsu, Chinitsu]),
        ([0, 1, 2, 3, 4, 27, 31], vec![Chiitoitsu, Honitsu]),
        ([0, 8, 9, 17, 18, 26, 27], vec![Chiitoitsu, Honroutou]),
        (
            [0, 8, 27, 28, 29, 30, 31],
            vec![Chiitoitsu, Honroutou, Honitsu],
        ),
        ([27, 28, 29, 30, 31, 32, 33], vec![Chiitoitsu, Tsuuiisou]),
    ] {
        assert_yaku(&chiitoitsu(pairs), &expected);
    }
}

#[test]
fn tanyao_checks_every_group_and_pair() {
    let groups = [sequence(1), sequence(10), sequence(21), triplet(5)];
    assert_yaku(&standard(groups, 13), &[Yaku::Tanyao]);
    for pair in [0, 8, 9, 17, 18, 26, 27, 33] {
        assert_yaku(&standard(groups, pair), &[]);
    }
    for group in [sequence(0), sequence(6), triplet(31), kan(8, true)] {
        let mut changed = groups;
        changed[3] = group;
        assert_yaku(
            &standard(changed, 13),
            if group == triplet(31) {
                &[Yaku::Haku]
            } else {
                &[]
            },
        );
    }
}

#[test]
fn toitoi_accepts_open_triplets_and_kans_but_not_sequences() {
    let groups = [triplet(1), triplet(10), kan(19, false), kan(23, true)];
    assert_yaku(
        &standard(groups, 13),
        &[
            Yaku::Tanyao,
            Yaku::Toitoi,
            Yaku::SanshokuDoukou,
            Yaku::Sanankou,
        ],
    );
    let mut changed = groups;
    changed[0] = sequence(1);
    assert_yaku(&standard(changed, 13), &[Yaku::Tanyao]);
}

#[test]
fn terminal_and_honor_yaku_check_the_whole_hand_and_supersede_honroutou() {
    use Yaku::*;
    let terminals = [triplet(0), triplet(8), kan(9, true), triplet(17)];
    assert_yaku(&standard(terminals, 26), &[Chinroutou, Toitoi, Sanankou]);
    assert_yaku(&standard(terminals, 27), &[Honroutou, Toitoi, Sanankou]);
    assert_yaku(&standard(terminals, 22), &[Toitoi, Sanankou]);
    let honors = [triplet(27), triplet(28), kan(29, false), triplet(31)];
    assert_yaku(
        &standard(honors, 32),
        &[Tsuuiisou, Toitoi, Haku, Bakaze, Jikaze, SuuankouTanki],
    );
    assert_yaku(
        &standard(honors, 0),
        &[
            Honroutou,
            Honitsu,
            Toitoi,
            Haku,
            Bakaze,
            Jikaze,
            SuuankouTanki,
        ],
    );
    assert_yaku(
        &standard(honors, 1),
        &[Honitsu, Toitoi, Haku, Bakaze, Jikaze, SuuankouTanki],
    );
    let mixed = [triplet(0), triplet(9), kan(27, true), triplet(31)];
    assert_yaku(
        &standard(mixed, 8),
        &[Honroutou, Toitoi, Bakaze, Haku, Sanankou],
    );
    assert_yaku(&standard(mixed, 1), &[Toitoi, Bakaze, Haku, Sanankou]);
}

#[test]
fn flush_yaku_include_pair_and_kan_and_require_exactly_one_suit() {
    use Yaku::*;
    for suit in 0..3 {
        let base = suit * 9;
        let groups = [
            sequence(base),
            sequence(base + 1),
            sequence(base + 5),
            kan(base + 8, true),
        ];
        assert_yaku(&standard(groups, base + 4), &[Chinitsu]);
        assert_yaku(&standard(groups, 27), &[Honitsu]);
        assert_yaku(&standard(groups, (base + 9) % 27 + 4), &[]);
        let mut changed = groups;
        changed[3] = kan(31, true);
        assert_yaku(&standard(changed, base + 4), &[Honitsu, Haku]);
        changed[3] = kan((base + 9) % 27, false);
        assert_yaku(&standard(changed, base + 4), &[]);
    }
}

#[test]
fn dragon_yaku_count_triplets_and_kans_and_check_pair() {
    use Yaku::*;
    for pair in 31..=33 {
        let others: Vec<_> = (31..=33).filter(|&tile| tile != pair).collect();
        let groups = [
            triplet(others[0]),
            kan(others[1], true),
            sequence(1),
            sequence(12),
        ];
        let dragons = match pair {
            31 => [Hatsu, Chun],
            32 => [Haku, Chun],
            _ => [Haku, Hatsu],
        };
        assert_yaku(
            &standard(groups, pair),
            &[Shousangen, dragons[0], dragons[1]],
        );
        assert_yaku(&standard(groups, 27), &dragons);
    }
    assert_yaku(
        &standard(
            [triplet(31), kan(32, true), kan(33, false), sequence(1)],
            13,
        ),
        &[Daisangen, Haku, Hatsu, Chun],
    );
    assert_yaku(
        &standard([triplet(31), triplet(27), sequence(1), sequence(12)], 32),
        &[Haku, Bakaze],
    );
    assert_yaku(
        &standard([triplet(31), triplet(32), kan(33, true), triplet(27)], 28),
        &[
            Tsuuiisou, Toitoi, Daisangen, Haku, Hatsu, Chun, Bakaze, Sanankou,
        ],
    );
}

#[test]
fn repeated_sequences_count_disjoint_pairs() {
    use Yaku::*;
    assert_yaku(
        &standard([sequence(1), sequence(1), sequence(10), triplet(19)], 23),
        &[Tanyao, Iipeikou],
    );
    assert_yaku(
        &standard([sequence(1), sequence(1), sequence(1), sequence(10)], 23),
        &[Tanyao, Iipeikou],
    );
    assert_yaku(
        &standard([sequence(1), sequence(1), sequence(10), sequence(10)], 23),
        &[Tanyao, Ryanpeikou],
    );
    assert_yaku(&standard([sequence(1); 4], 13), &[Tanyao, Ryanpeikou]);
    assert_yaku(
        &standard([sequence(1), sequence(2), sequence(10), sequence(11)], 23),
        &[Tanyao],
    );
    assert_yaku(
        &standard([triplet(1), triplet(2), triplet(3), triplet(10)], 23),
        &[Tanyao, Toitoi, SuuankouTanki],
    );
}

#[test]
fn any_open_group_disables_iipeikou_and_ryanpeikou() {
    for groups in [
        [sequence(1), sequence(1), sequence(10), sequence(10)],
        [sequence(1), sequence(1), sequence(10), triplet(19)],
        [sequence(1), sequence(1), sequence(10), kan(19, false)],
    ] {
        for index in 0..4 {
            let mut opened = groups;
            match &mut opened[index] {
                AgariGroup::Sequence { open, .. }
                | AgariGroup::Triplet { open, .. }
                | AgariGroup::Kan { open, .. } => *open = true,
            }
            assert_yaku(&standard(opened, 23), &[Yaku::Tanyao]);
        }
    }
    assert_yaku(
        &standard([sequence(1), sequence(1), sequence(10), kan(19, false)], 23),
        &[Yaku::Tanyao, Yaku::Iipeikou],
    );
}

#[test]
fn sanshoku_requires_matching_sequence_starts_in_all_three_suits() {
    for start in 0..=6 {
        let pattern = standard(
            [
                sequence(start),
                sequence(start + 9),
                sequence(start + 18),
                triplet(27),
            ],
            31,
        );
        let expected = if matches!(start, 0 | 6) {
            vec![Yaku::SanshokuDoujun, Yaku::Chanta, Yaku::Bakaze]
        } else {
            vec![Yaku::SanshokuDoujun, Yaku::Bakaze]
        };
        assert_yaku(&pattern, &expected);
    }
    assert_yaku(
        &standard([sequence(1), sequence(10), sequence(20), triplet(27)], 31),
        &[Yaku::Bakaze],
    );
    assert_yaku(
        &standard([sequence(1), sequence(1), sequence(10), triplet(27)], 31),
        &[Yaku::Iipeikou, Yaku::Bakaze],
    );
    assert_yaku(
        &standard([triplet(1), triplet(10), triplet(19), sequence(4)], 31),
        &[Yaku::SanshokuDoukou, Yaku::Sanankou],
    );
}

#[test]
fn ittsu_requires_123_456_789_in_the_same_suit() {
    for suit in 0..3 {
        let base = suit * 9;
        assert_yaku(
            &standard(
                [
                    sequence(base),
                    sequence(base + 3),
                    sequence(base + 6),
                    triplet(27),
                ],
                31,
            ),
            &[Yaku::Honitsu, Yaku::Ittsu, Yaku::Bakaze],
        );
    }
    assert_yaku(
        &standard([sequence(0), sequence(3), sequence(24), triplet(27)], 31),
        &[Yaku::Bakaze],
    );
    assert_yaku(
        &standard([sequence(0), sequence(3), sequence(5), triplet(27)], 31),
        &[Yaku::Honitsu, Yaku::Bakaze],
    );
    assert_yaku(
        &standard([sequence(0), sequence(3), triplet(6), triplet(27)], 31),
        &[Yaku::Honitsu, Yaku::Bakaze],
    );
}

#[test]
fn outside_hands_require_a_sequence_and_every_group_and_pair_to_qualify() {
    use Yaku::*;
    let groups = [sequence(0), sequence(15), triplet(18), kan(26, true)];
    assert_yaku(&standard(groups, 8), &[Junchan]);
    assert_yaku(&standard(groups, 27), &[Chanta]);
    assert_context_yaku(&standard(groups, 1), context(1, RON), &[&[], &[]]);
    for group in [sequence(10), triplet(10), kan(10, false)] {
        let mut changed = groups;
        changed[1] = group;
        assert_yaku(&standard(changed, 8), &[]);
        assert_yaku(&standard(changed, 27), &[]);
    }
    let mut honors = groups;
    honors[3] = kan(31, true);
    assert_yaku(&standard(honors, 8), &[Chanta, Haku]);
    assert_context_yaku(&standard(honors, 1), context(1, RON), &[&[Haku], &[Haku]]);
    assert_yaku(
        &standard([triplet(0), triplet(9), triplet(18), kan(27, false)], 31),
        &[Honroutou, Toitoi, SanshokuDoukou, Bakaze, SuuankouTanki],
    );
    assert_yaku(
        &standard([triplet(0), triplet(9), triplet(18), kan(26, false)], 8),
        &[Chinroutou, Toitoi, SanshokuDoukou, SuuankouTanki],
    );
}

#[test]
fn open_sequences_preserve_sanshoku_ittsu_and_outside_yaku() {
    for (groups, pair, expected) in [
        (
            [sequence(0), sequence(9), sequence(18), triplet(27)],
            31,
            vec![Yaku::SanshokuDoujun, Yaku::Chanta, Yaku::Bakaze],
        ),
        (
            [sequence(0), sequence(9), sequence(18), triplet(8)],
            26,
            vec![Yaku::SanshokuDoujun, Yaku::Junchan],
        ),
        (
            [sequence(0), sequence(3), sequence(6), triplet(27)],
            31,
            vec![Yaku::Honitsu, Yaku::Ittsu, Yaku::Bakaze],
        ),
    ] {
        assert_yaku(&standard(groups, pair), &expected);
        for index in 0..3 {
            let mut opened = groups;
            if let AgariGroup::Sequence { open, .. } = &mut opened[index] {
                *open = true;
            }
            assert_yaku(&standard(opened, pair), &expected);
        }
    }
}

#[test]
fn ambiguous_hand_keeps_chiitoitsu_and_ryanpeikou_in_separate_patterns() {
    let hand = Hand::new(
        [0, 0, 6, 6, 7, 7, 8, 8, 15, 15, 16, 16, 17, 17]
            .map(|tile| Tile::new(tile).unwrap())
            .to_vec(),
        vec![],
    )
    .unwrap();
    let candidates = patterns(&hand);
    assert_eq!(candidates.len(), 2);
    for pattern in candidates {
        match &pattern {
            AgariPattern::Chiitoitsu { .. } => assert_yaku(&pattern, &[Yaku::Chiitoitsu]),
            AgariPattern::Standard { .. } => {
                assert_yaku(&pattern, &[Yaku::Ryanpeikou, Yaku::Junchan])
            }
            _ => panic!("unexpected pattern: {pattern:?}"),
        }
    }
}

#[test]
fn kokushi_keeps_its_existing_detection_without_other_tile_property_yaku() {
    assert_yaku(
        &AgariPattern::Kokushi { pair: kind(0) },
        &[Yaku::KokushiJuusanmen],
    );
}

fn context(winning_tile: u8, win_method: WinMethod) -> AgariContext {
    AgariContext {
        winning_tile: kind(winning_tile),
        win_method,
        round_wind: Wind::East,
        seat_wind: Wind::South,
        riichi: RiichiStatus::None,
    }
}

fn assert_context_yaku(pattern: &AgariPattern, context: AgariContext, expected: &[&[Yaku]]) {
    let candidates = interpretations(pattern, context.winning_tile).unwrap();
    let actual: Vec<_> = candidates
        .iter()
        .map(|candidate| detect_yaku(candidate, &context).unwrap())
        .collect();
    assert_eq!(
        actual.len(),
        expected.len(),
        "{pattern:?} {context:?}: {actual:?}"
    );
    let mut remaining = expected.to_vec();
    for candidate in &actual {
        let index = remaining.iter().position(|expected| {
            candidate.len() == expected.len()
                && expected.iter().all(|yaku| candidate.contains(yaku))
        });
        let Some(index) = index else {
            panic!("{pattern:?} {context:?}: unexpected {candidate:?}, expected {expected:?}");
        };
        remaining.swap_remove(index);
    }
}

#[test]
fn pinfu_distinguishes_ryanmen_from_middle_and_edge_waits_in_every_suit() {
    for suit in 0..3 {
        for start in 0..7 {
            let pattern = standard(
                [
                    sequence(suit * 9 + start),
                    sequence((suit + 1) % 3 * 9 + 1),
                    sequence((suit + 1) % 3 * 9 + 4),
                    sequence((suit + 2) % 3 * 9 + 3),
                ],
                30,
            );
            for offset in 0..3 {
                for method in [RON, TSUMO] {
                    let expected: &[Yaku] = match (start, offset) {
                        (_, 1) | (0, 2) | (6, 0) => &[],
                        _ => &[Yaku::Pinfu],
                    };
                    let mut expected = expected.to_vec();
                    if method == TSUMO {
                        expected.push(Yaku::MenzenTsumo);
                    }
                    assert_context_yaku(
                        &pattern,
                        context(suit * 9 + start + offset, method),
                        &[&expected],
                    );
                }
            }
        }
    }
}

#[test]
fn pinfu_rejects_value_pairs_for_every_round_and_seat_wind() {
    let groups = [sequence(1), sequence(4), sequence(10), sequence(21)];
    let winds = [Wind::East, Wind::South, Wind::West, Wind::North];
    for round_wind in winds {
        for seat_wind in winds {
            for (index, pair_wind) in winds.into_iter().enumerate() {
                let expected: &[Yaku] = if pair_wind == round_wind || pair_wind == seat_wind {
                    &[]
                } else {
                    &[Yaku::Pinfu]
                };
                assert_context_yaku(
                    &standard(groups, 27 + index as u8),
                    AgariContext {
                        round_wind,
                        seat_wind,
                        ..context(1, RON)
                    },
                    &[expected],
                );
            }
        }
    }
    for pair in 31..=33 {
        assert_context_yaku(&standard(groups, pair), context(1, RON), &[&[]]);
    }
}

#[test]
fn pinfu_requires_closed_sequences_and_a_sequence_win() {
    let groups = [sequence(1), sequence(4), sequence(10), sequence(21)];
    assert_context_yaku(
        &standard(groups, 13),
        context(1, RON),
        &[&[Yaku::Tanyao, Yaku::Pinfu]],
    );
    assert_context_yaku(&standard(groups, 13), context(13, RON), &[&[Yaku::Tanyao]]);
    for replacement in [
        AgariGroup::Sequence {
            start: kind(4),
            open: true,
        },
        triplet(4),
        kan(4, false),
    ] {
        let mut changed = groups;
        changed[1] = replacement;
        assert_context_yaku(&standard(changed, 13), context(1, RON), &[&[Yaku::Tanyao]]);
    }
}

#[test]
fn ambiguous_sequence_and_pair_positions_keep_separate_yaku_lists() {
    for (pattern, winning_tile) in [
        (
            standard([sequence(1), sequence(2), sequence(10), sequence(21)], 13),
            2,
        ),
        (
            standard([sequence(1), sequence(4), sequence(10), sequence(21)], 1),
            1,
        ),
    ] {
        assert_context_yaku(
            &pattern,
            context(winning_tile, RON),
            &[&[Yaku::Tanyao], &[Yaku::Tanyao, Yaku::Pinfu]],
        );
    }
    assert_context_yaku(
        &standard([sequence(1), sequence(1), sequence(10), sequence(21)], 13),
        context(1, RON),
        &[
            &[Yaku::Tanyao, Yaku::Iipeikou, Yaku::Pinfu],
            &[Yaku::Tanyao, Yaku::Iipeikou, Yaku::Pinfu],
        ],
    );
}

#[test]
fn sanankou_counts_only_triplets_completed_without_ron() {
    let pattern = standard([triplet(1), triplet(10), triplet(19), sequence(22)], 13);
    assert_context_yaku(
        &pattern,
        context(1, RON),
        &[&[Yaku::Tanyao, Yaku::SanshokuDoukou]],
    );
    for (tile, method) in [(1, TSUMO), (22, RON), (13, RON)] {
        let mut expected = vec![Yaku::Tanyao, Yaku::SanshokuDoukou, Yaku::Sanankou];
        if method == TSUMO {
            expected.push(Yaku::MenzenTsumo);
        }
        assert_context_yaku(&pattern, context(tile, method), &[&expected]);
    }
}

#[test]
fn ron_can_complete_either_a_triplet_or_a_sequence_of_the_same_tile() {
    let pattern = standard([triplet(1), triplet(10), triplet(19), sequence(1)], 13);
    let before = pattern.clone();
    assert_context_yaku(
        &pattern,
        context(1, RON),
        &[
            &[Yaku::Tanyao, Yaku::SanshokuDoukou],
            &[Yaku::Tanyao, Yaku::SanshokuDoukou, Yaku::Sanankou],
        ],
    );
    assert_context_yaku(
        &pattern,
        context(1, TSUMO),
        &[
            &[
                Yaku::Tanyao,
                Yaku::SanshokuDoukou,
                Yaku::Sanankou,
                Yaku::MenzenTsumo,
            ],
            &[
                Yaku::Tanyao,
                Yaku::SanshokuDoukou,
                Yaku::Sanankou,
                Yaku::MenzenTsumo,
            ],
        ],
    );
    assert_eq!(pattern, before);
}

#[test]
fn sanankou_allows_an_open_fourth_group_and_counts_only_concealed_kans() {
    let open_sequence = AgariGroup::Sequence {
        start: kind(22),
        open: true,
    };
    for fourth in [
        open_sequence,
        AgariGroup::Triplet {
            tile: kind(22),
            open: true,
        },
        kan(22, true),
    ] {
        let expected = if matches!(fourth, AgariGroup::Sequence { .. }) {
            vec![Yaku::Tanyao, Yaku::SanshokuDoukou, Yaku::Sanankou]
        } else {
            vec![
                Yaku::Tanyao,
                Yaku::SanshokuDoukou,
                Yaku::Toitoi,
                Yaku::Sanankou,
            ]
        };
        assert_context_yaku(
            &standard([kan(1, false), triplet(10), triplet(19), fourth], 13),
            context(13, RON),
            &[&expected],
        );
    }
    assert_context_yaku(
        &standard([kan(1, true), triplet(10), triplet(19), open_sequence], 13),
        context(13, RON),
        &[&[Yaku::Tanyao, Yaku::SanshokuDoukou]],
    );
}

#[test]
fn suuankou_distinguishes_tsumo_triplet_from_ron_and_pair_wins() {
    for first in [triplet(1), kan(1, false)] {
        let pattern = standard([first, triplet(10), triplet(19), triplet(22)], 13);
        assert_context_yaku(
            &pattern,
            context(10, TSUMO),
            &[&[
                Yaku::Tanyao,
                Yaku::SanshokuDoukou,
                Yaku::Toitoi,
                Yaku::Suuankou,
                Yaku::MenzenTsumo,
            ]],
        );
        assert_context_yaku(
            &pattern,
            context(10, RON),
            &[&[
                Yaku::Tanyao,
                Yaku::SanshokuDoukou,
                Yaku::Toitoi,
                Yaku::Sanankou,
            ]],
        );
        for method in [RON, TSUMO] {
            let mut expected = vec![
                Yaku::Tanyao,
                Yaku::SanshokuDoukou,
                Yaku::Toitoi,
                Yaku::SuuankouTanki,
            ];
            if method == TSUMO {
                expected.push(Yaku::MenzenTsumo);
            }
            assert_context_yaku(&pattern, context(13, method), &[&expected]);
        }
    }
}

#[test]
fn special_interpretations_keep_existing_yaku() {
    let seven_pairs = chiitoitsu([0, 3, 5, 10, 12, 19, 21]);
    for tile in [0, 3, 5, 10, 12, 19, 21] {
        assert_context_yaku(&seven_pairs, context(tile, RON), &[&[Yaku::Chiitoitsu]]);
    }
    let kokushi = AgariPattern::Kokushi { pair: kind(0) };
    for tile in [0, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 33] {
        assert_context_yaku(
            &kokushi,
            context(tile, RON),
            &[&[if tile == 0 {
                Yaku::KokushiJuusanmen
            } else {
                Yaku::Kokushi
            }]],
        );
    }
}

#[test]
fn unified_entry_combines_shape_and_context_yaku() {
    assert_yaku(
        &standard([sequence(1), sequence(4), sequence(11), sequence(23)], 10),
        &[Yaku::Tanyao],
    );
    assert_yaku(
        &standard([triplet(1), triplet(10), triplet(19), triplet(31)], 27),
        &[
            Yaku::Toitoi,
            Yaku::SanshokuDoukou,
            Yaku::Haku,
            Yaku::SuuankouTanki,
        ],
    );
}

#[test]
fn all_tsumo_sources_use_the_same_concealed_triplet_rule() {
    for source in [
        TsumoSource::Wall,
        TsumoSource::LastWall,
        TsumoSource::Rinshan,
        TsumoSource::FirstDraw,
    ] {
        let first = if source == TsumoSource::Rinshan {
            kan(1, false)
        } else {
            triplet(1)
        };
        let mut expected = vec![
            Yaku::Tanyao,
            Yaku::SanshokuDoukou,
            Yaku::Toitoi,
            Yaku::Suuankou,
            Yaku::MenzenTsumo,
        ];
        match source {
            TsumoSource::Wall => {}
            TsumoSource::LastWall => expected.push(Yaku::Haitei),
            TsumoSource::Rinshan => expected.push(Yaku::RinshanKaihou),
            TsumoSource::FirstDraw => expected.push(Yaku::Chiihou),
        }
        assert_context_yaku(
            &standard([first, triplet(10), triplet(19), triplet(22)], 13),
            context(10, WinMethod::Tsumo(source)),
            &[&expected],
        );
    }
}

#[test]
fn ron_sources_accept_matching_patterns_and_add_event_yaku() {
    let sequence_hand = standard([sequence(1), sequence(4), sequence(10), sequence(21)], 13);
    for source in [RonSource::Discard, RonSource::LastDiscard, RonSource::Kakan] {
        let mut expected = vec![Yaku::Tanyao, Yaku::Pinfu];
        match source {
            RonSource::LastDiscard => expected.push(Yaku::Houtei),
            RonSource::Kakan => expected.push(Yaku::Chankan),
            _ => {}
        }
        assert_context_yaku(
            &sequence_hand,
            context(1, WinMethod::Ron(source)),
            &[&expected],
        );
    }

    assert_context_yaku(
        &standard([triplet(1), triplet(10), triplet(19), triplet(22)], 13),
        context(10, WinMethod::Ron(RonSource::LastDiscard)),
        &[&[
            Yaku::Tanyao,
            Yaku::Toitoi,
            Yaku::SanshokuDoukou,
            Yaku::Sanankou,
            Yaku::Houtei,
        ]],
    );
}

#[test]
fn riichi_status_adds_riichi_double_riichi_and_ippatsu() {
    let pattern = standard([sequence(1), sequence(4), sequence(10), sequence(21)], 13);
    for riichi in [
        RiichiStatus::None,
        RiichiStatus::Riichi { ippatsu: false },
        RiichiStatus::Riichi { ippatsu: true },
        RiichiStatus::DoubleRiichi { ippatsu: false },
        RiichiStatus::DoubleRiichi { ippatsu: true },
    ] {
        let mut expected = vec![Yaku::Tanyao, Yaku::Pinfu];
        match riichi {
            RiichiStatus::None => {}
            RiichiStatus::Riichi { ippatsu } => {
                expected.push(Yaku::Riichi);
                if ippatsu {
                    expected.push(Yaku::Ippatsu);
                }
            }
            RiichiStatus::DoubleRiichi { ippatsu } => {
                expected.push(Yaku::DoubleRiichi);
                if ippatsu {
                    expected.push(Yaku::Ippatsu);
                }
            }
        }
        assert_context_yaku(
            &pattern,
            AgariContext {
                riichi,
                ..context(1, RON)
            },
            &[&expected],
        );
    }
}

#[test]
fn context_cannot_replace_the_winning_tile_of_an_interpretation() {
    for (pattern, original, replacement) in [
        (
            standard([sequence(1), sequence(4), sequence(10), sequence(21)], 13),
            1,
            4,
        ),
        (chiitoitsu([1, 3, 5, 10, 12, 19, 21]), 1, 3),
        (AgariPattern::Kokushi { pair: kind(0) }, 0, 8),
    ] {
        // 两张牌都能完成这个拆分，但不能更换已经生成的解释中的和牌张。
        assert!(interpretations(&pattern, kind(replacement)).is_ok());
        for interpretation in interpretations(&pattern, kind(original)).unwrap() {
            assert_eq!(
                detect_yaku(&interpretation, &context(replacement, RON)),
                Err(YakuDetectionError::ContextWinningTileMismatch {
                    interpretation_tile: kind(original),
                    context_tile: kind(replacement),
                })
            );
        }
    }
}
