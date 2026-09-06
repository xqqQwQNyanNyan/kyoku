use kyoku::analysis::agari::{
    AgariGroup, AgariInterpretation, AgariPattern, WinningPosition, interpretations,
};
use kyoku::analysis::{
    AgariContext, HandValue, RiichiStatus, RonSource, ScoringError, TsumoSource, WinMethod, Yaku,
    calculate_hand_value, detect_yaku,
};
use kyoku::mahjong::{round::Wind, tile::TileKind};

fn kind(tile: u8) -> TileKind {
    TileKind::new(tile).unwrap()
}

fn sequence(start: u8, open: bool) -> AgariGroup {
    AgariGroup::Sequence {
        start: kind(start),
        open,
    }
}

fn triplet(tile: u8, open: bool) -> AgariGroup {
    AgariGroup::Triplet {
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

fn context(tile: u8, tsumo: bool) -> AgariContext {
    AgariContext {
        winning_tile: kind(tile),
        win_method: if tsumo {
            WinMethod::Tsumo(TsumoSource::Wall)
        } else {
            WinMethod::Ron(RonSource::Discard)
        },
        round_wind: Wind::East,
        seat_wind: Wind::South,
        riichi: RiichiStatus::None,
    }
}

fn interpretation(pattern: &AgariPattern, tile: u8) -> AgariInterpretation<'_> {
    interpretations(pattern, kind(tile)).unwrap()[0]
}

fn value(pattern: &AgariPattern, context: &AgariContext) -> HandValue {
    let interpretation = interpretation(pattern, context.winning_tile.as_u8());
    let yaku = detect_yaku(&interpretation, context).unwrap();
    calculate_hand_value(&interpretation, context, &yaku).unwrap()
}

#[test]
fn pinfu_tsumo_is_twenty_and_closed_ron_is_thirty() {
    let pattern = standard(
        [
            sequence(0, false),
            sequence(3, false),
            sequence(10, false),
            sequence(21, false),
        ],
        13,
    );
    assert_eq!(
        value(&pattern, &context(0, true)),
        HandValue {
            fu: Some(20),
            han: 2,
            yakuman: 0
        }
    );
    assert_eq!(
        value(&pattern, &context(0, false)),
        HandValue {
            fu: Some(30),
            han: 1,
            yakuman: 0
        }
    );
}

#[test]
fn open_sequence_shape_is_thirty_for_ron_and_tsumo() {
    let pattern = standard(
        [
            sequence(0, true),
            sequence(3, false),
            sequence(10, false),
            sequence(21, false),
        ],
        13,
    );
    for tsumo in [false, true] {
        assert_eq!(
            value(&pattern, &context(3, tsumo)),
            HandValue {
                fu: Some(30),
                han: 0,
                yakuman: 0
            }
        );
    }
}

#[test]
fn seven_pairs_stays_twenty_five_with_value_pair_and_tsumo() {
    let pattern = AgariPattern::Chiitoitsu {
        pairs: [0, 3, 10, 13, 19, 27, 31].map(kind).to_vec(),
    };
    for tsumo in [false, true] {
        let mut context = context(27, tsumo);
        context.seat_wind = Wind::East;
        assert_eq!(
            value(&pattern, &context),
            HandValue {
                fu: Some(25),
                han: if tsumo { 3 } else { 2 },
                yakuman: 0
            }
        );
    }
}

#[test]
fn sequence_waits_cover_all_suits_and_both_edges() {
    for suit in 0..3 {
        for (start, winning, expected_fu) in [
            (0, 0, 30),
            (0, 1, 40),
            (0, 2, 40),
            (3, 3, 30),
            (3, 4, 40),
            (3, 5, 30),
            (6, 6, 40),
            (6, 7, 40),
            (6, 8, 30),
        ] {
            let pattern = standard(
                [
                    sequence(suit * 9 + start, false),
                    sequence((suit + 1) % 3 * 9, false),
                    sequence((suit + 2) % 3 * 9 + 3, false),
                    sequence((suit + 1) % 3 * 9 + 6, false),
                ],
                29,
            );
            assert_eq!(
                value(&pattern, &context(suit * 9 + winning, false)).fu,
                Some(expected_fu),
                "suit {suit}, start {start}, winning {winning}"
            );
        }
    }
}

#[test]
fn tanki_adds_two_and_shanpon_adds_none() {
    // 明中张杠 8 符使单骑的 2 符跨过进位边界。
    let tanki = standard(
        [
            AgariGroup::Kan {
                tile: kind(4),
                open: true,
            },
            sequence(9, false),
            sequence(12, false),
            sequence(21, false),
        ],
        29,
    );
    assert_eq!(value(&tanki, &context(29, true)).fu, Some(40));
    // 荣和刻子 2 + 明幺九杠 16 + 三元雀头 2；错误添加双碰符会从 40 变成 50。
    let shanpon = standard(
        [
            triplet(4, false),
            AgariGroup::Kan {
                tile: kind(0),
                open: true,
            },
            sequence(12, false),
            sequence(21, false),
        ],
        31,
    );
    assert_eq!(value(&shanpon, &context(4, true)).fu, Some(50));
    assert_eq!(value(&shanpon, &context(4, false)).fu, Some(40));
}

#[test]
fn ron_completed_triplet_is_open_for_fu_but_hand_remains_closed() {
    let pattern = standard(
        [
            triplet(0, false),
            triplet(13, false),
            sequence(18, false),
            sequence(21, false),
        ],
        29,
    );
    // 门前荣和 10 + 荣和幺九刻 4 + 暗中张刻 4 = 38，不能按暗刻算成 42。
    assert_eq!(value(&pattern, &context(0, false)).fu, Some(40));
    assert_eq!(value(&pattern, &context(0, true)).fu, Some(40));
}

#[test]
fn triplet_and_kan_fu_cover_open_closed_simple_terminal_and_honor() {
    for (tile, terminal_multiplier) in [(4, 1), (0, 2), (8, 2), (31, 2)] {
        for (kan, open, base_fu) in [
            (false, true, 2),
            (false, false, 4),
            (true, true, 8),
            (true, false, 16),
        ] {
            let group = if kan {
                AgariGroup::Kan {
                    tile: kind(tile),
                    open,
                }
            } else {
                triplet(tile, open)
            };
            for (pair, pair_fu, seat_wind) in [
                (29, 0, Wind::South),
                (27, 2, Wind::South),
                (27, 4, Wind::East),
            ] {
                for tsumo in [false, true] {
                    let pattern = standard(
                        [
                            sequence(9, true),
                            group,
                            triplet(13, true),
                            sequence(21, false),
                        ],
                        pair,
                    );
                    let mut context = context(21, tsumo);
                    context.seat_wind = seat_wind;
                    let raw: u32 = 20
                        + 2
                        + base_fu * terminal_multiplier
                        + pair_fu
                        + if tsumo { 2 } else { 0 };
                    assert_eq!(
                        value(&pattern, &context).fu,
                        Some(raw.div_ceil(10) * 10),
                        "{group:?}, pair {pair}, tsumo {tsumo}"
                    );
                }
            }
        }
    }
}

#[test]
fn value_pairs_include_dragons_each_wind_and_double_wind() {
    for (pair, seat, round, expected_fu) in [
        (29, Wind::South, Wind::East, 30),
        (27, Wind::South, Wind::East, 30),
        (28, Wind::South, Wind::East, 30),
        (27, Wind::East, Wind::East, 40),
        (31, Wind::South, Wind::East, 30),
        (32, Wind::South, Wind::East, 30),
        (33, Wind::South, Wind::East, 30),
    ] {
        let pattern = standard(
            [
                triplet(0, false),
                sequence(9, true),
                sequence(12, false),
                sequence(21, false),
            ],
            pair,
        );
        let mut context = context(21, false);
        context.seat_wind = seat;
        context.round_wind = round;
        assert_eq!(value(&pattern, &context).fu, Some(expected_fu));
        context.win_method = WinMethod::Tsumo(TsumoSource::Wall);
        assert_eq!(
            value(&pattern, &context).fu,
            Some(if pair == 29 { 30 } else { 40 })
        );
    }
}

#[test]
fn identical_yaku_lists_keep_different_fu_for_different_interpretations() {
    let pattern = standard(
        [
            sequence(0, false),
            sequence(2, false),
            triplet(13, false),
            triplet(21, false),
        ],
        29,
    );
    let context = context(2, true);
    let candidates = interpretations(&pattern, kind(2)).unwrap();
    assert_eq!(candidates.len(), 2);
    let first_yaku = detect_yaku(&candidates[0], &context).unwrap();
    let second_yaku = detect_yaku(&candidates[1], &context).unwrap();
    assert_eq!(first_yaku, second_yaku);
    // 自摸 22 + 两个暗刻 8，边张再加 2，跨过进位边界。
    for (candidate, expected) in candidates.iter().zip([40, 30]) {
        let yaku = detect_yaku(candidate, &context).unwrap();
        assert_eq!(
            calculate_hand_value(candidate, &context, &yaku).unwrap().fu,
            Some(expected)
        );
    }
}

#[test]
fn reduced_han_use_the_hand_open_state() {
    for (yaku, starts, pair, closed_han, open_han) in [
        (Yaku::SanshokuDoujun, [0, 9, 18, 12], 29, 2, 1),
        (Yaku::Ittsu, [0, 3, 6, 12], 29, 2, 1),
        (Yaku::Chanta, [0, 9, 24, 6], 27, 2, 1),
        (Yaku::Junchan, [0, 9, 24, 6], 8, 3, 2),
        (Yaku::Honitsu, [0, 3, 3, 6], 27, 3, 2),
        (Yaku::Chinitsu, [0, 3, 3, 6], 1, 6, 5),
    ] {
        for open in [false, true] {
            let mut groups = starts.map(|start| sequence(start, false));
            groups[0] = sequence(starts[0], open);
            let pattern = standard(groups, pair);
            let context = context(starts[3] + 1, false);
            let interpretation = interpretation(&pattern, context.winning_tile.as_u8());
            let detected = detect_yaku(&interpretation, &context).unwrap();
            assert!(detected.contains(&yaku));
            // 独立检查食下役映射，避免其他复合役掩盖番数差异。
            assert_eq!(
                calculate_hand_value(&interpretation, &context, &[yaku])
                    .unwrap()
                    .han,
                if open { open_han } else { closed_han }
            );
        }
    }
}

#[test]
fn event_yaku_han_accumulate_and_double_riichi_replaces_riichi() {
    let pattern = standard(
        [
            sequence(0, false),
            sequence(3, false),
            sequence(10, false),
            sequence(21, false),
        ],
        13,
    );
    for (riichi, expected) in [
        (RiichiStatus::None, 3),
        (RiichiStatus::Riichi { ippatsu: false }, 4),
        (RiichiStatus::Riichi { ippatsu: true }, 5),
        (RiichiStatus::DoubleRiichi { ippatsu: true }, 6),
    ] {
        let context = AgariContext {
            riichi,
            win_method: WinMethod::Tsumo(TsumoSource::LastWall),
            ..context(0, true)
        };
        assert_eq!(
            value(&pattern, &context),
            HandValue {
                fu: Some(20),
                han: expected,
                yakuman: 0
            }
        );
    }
}

#[test]
fn concealed_kans_preserve_closed_han_and_ron_fu() {
    let pattern = standard(
        [
            AgariGroup::Kan {
                tile: kind(0),
                open: false,
            },
            AgariGroup::Kan {
                tile: kind(4),
                open: false,
            },
            AgariGroup::Kan {
                tile: kind(8),
                open: false,
            },
            sequence(12, false),
        ],
        29,
    );
    let mut context = context(12, false);
    context.riichi = RiichiStatus::Riichi { ippatsu: false };
    assert_eq!(
        value(&pattern, &context),
        HandValue {
            fu: Some(110),
            han: 5,
            yakuman: 0
        }
    );
    context.win_method = WinMethod::Tsumo(TsumoSource::Rinshan);
    assert_eq!(
        value(&pattern, &context),
        HandValue {
            fu: Some(110),
            han: 7,
            yakuman: 0
        }
    );
}

#[test]
fn ordinary_triplet_yaku_and_double_wind_han_accumulate() {
    let pattern = standard(
        [
            triplet(27, true),
            triplet(31, true),
            triplet(32, true),
            triplet(0, false),
        ],
        33,
    );
    let mut context = context(0, false);
    context.seat_wind = Wind::East;
    // 对对和 2、混老头 2、小三元 2、混一色 2、白发各 1、连风 2。
    assert_eq!(
        value(&pattern, &context),
        HandValue {
            fu: Some(40),
            han: 12,
            yakuman: 0
        }
    );
}

#[test]
fn every_yakuman_shape_is_one_and_suppresses_ordinary_han() {
    use Yaku::*;
    let pattern = standard(
        [
            triplet(0, false),
            triplet(9, false),
            triplet(18, false),
            triplet(27, false),
        ],
        31,
    );
    let context = context(31, false);
    let interpretation = interpretation(&pattern, 31);
    for yaku in [
        Kokushi,
        KokushiJuusanmen,
        Suuankou,
        SuuankouTanki,
        Daisangen,
        Shousuushi,
        Daisuushi,
        Tsuuiisou,
        Chinroutou,
        Ryuuiisou,
        Suukantsu,
        ChuurenPoutou,
        JunseiChuurenPoutou,
        Tenhou,
        Chiihou,
    ] {
        // 逐项验证计分表；计分入口不重新判断传入役的成立条件。
        for yaku_list in [vec![yaku, Toitoi, Haku], vec![Toitoi, Haku, yaku]] {
            assert_eq!(
                calculate_hand_value(&interpretation, &context, &yaku_list).unwrap(),
                HandValue {
                    fu: None,
                    han: 0,
                    yakuman: 1
                }
            );
        }
    }
}

#[test]
fn detected_yakuman_combinations_count_each_shape_once() {
    let pattern = standard(
        [
            triplet(27, false),
            triplet(28, false),
            triplet(29, false),
            triplet(30, false),
        ],
        31,
    );
    let context = context(31, false);
    // 大四喜、字一色、四暗刻单骑，各一倍。
    assert_eq!(
        value(&pattern, &context),
        HandValue {
            fu: None,
            han: 0,
            yakuman: 3
        }
    );
    let context = AgariContext {
        win_method: WinMethod::Tsumo(TsumoSource::FirstDraw),
        seat_wind: Wind::East,
        ..context
    };
    assert_eq!(
        value(&pattern, &context),
        HandValue {
            fu: None,
            han: 0,
            yakuman: 4
        }
    );
}

#[test]
fn kokushi_and_nine_gates_have_no_fu_including_enhanced_shapes() {
    let kokushi = AgariPattern::Kokushi { pair: kind(0) };
    for tile in [0, 8] {
        assert_eq!(
            value(&kokushi, &context(tile, false)),
            HandValue {
                fu: None,
                han: 0,
                yakuman: 1
            }
        );
    }
    let nine_gates = standard(
        [
            triplet(0, false),
            sequence(1, false),
            sequence(5, false),
            triplet(8, false),
        ],
        4,
    );
    for tile in [0, 4] {
        assert_eq!(
            value(&nine_gates, &context(tile, false)),
            HandValue {
                fu: None,
                han: 0,
                yakuman: 1
            }
        );
    }
    let honor_pairs = AgariPattern::Chiitoitsu {
        pairs: (27..34).map(kind).collect(),
    };
    assert_eq!(
        value(&honor_pairs, &context(31, true)),
        HandValue {
            fu: None,
            han: 0,
            yakuman: 1
        }
    );
}

#[test]
fn high_han_does_not_turn_into_counted_yakuman() {
    let pattern = standard(
        [
            AgariGroup::Kan {
                tile: kind(0),
                open: false,
            },
            AgariGroup::Kan {
                tile: kind(4),
                open: false,
            },
            AgariGroup::Kan {
                tile: kind(8),
                open: false,
            },
            sequence(1, false),
        ],
        5,
    );
    let context = AgariContext {
        win_method: WinMethod::Tsumo(TsumoSource::Rinshan),
        riichi: RiichiStatus::DoubleRiichi { ippatsu: false },
        ..context(1, true)
    };
    assert_eq!(
        value(&pattern, &context),
        HandValue {
            fu: Some(110),
            han: 14,
            yakuman: 0
        }
    );
}

#[test]
fn scoring_rejects_mismatched_tile_duplicates_and_nagashi_mangan() {
    let pattern = standard(
        [
            sequence(0, false),
            sequence(3, false),
            sequence(10, false),
            sequence(21, false),
        ],
        13,
    );
    let interpretation = interpretation(&pattern, 0);
    assert_eq!(interpretation.winning_position(), WinningPosition::Group(0));
    assert_eq!(
        calculate_hand_value(&interpretation, &context(2, false), &[Yaku::Pinfu]),
        Err(ScoringError::ContextWinningTileMismatch {
            interpretation_tile: kind(0),
            context_tile: kind(2)
        })
    );
    for yaku in [Yaku::Pinfu, Yaku::SuuankouTanki] {
        assert_eq!(
            calculate_hand_value(&interpretation, &context(0, false), &[yaku, yaku]),
            Err(ScoringError::DuplicateYaku(yaku))
        );
    }
    for yaku in [
        vec![Yaku::NagashiMangan],
        vec![Yaku::Tenhou, Yaku::NagashiMangan],
    ] {
        assert_eq!(
            calculate_hand_value(&interpretation, &context(0, false), &yaku),
            Err(ScoringError::UnsupportedYaku(Yaku::NagashiMangan))
        );
    }
}
