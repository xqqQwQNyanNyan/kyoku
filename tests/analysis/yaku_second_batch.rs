use kyoku::analysis::agari::{AgariGroup, AgariPattern, interpretations, patterns};
use kyoku::analysis::{
    AgariContext, RiichiStatus, RonSource, TsumoSource, WinMethod, Yaku, YakuDetectionError,
    detect_yaku,
};
use kyoku::mahjong::{
    hand::Hand,
    round::Wind,
    tile::{Tile, TileKind},
};

fn kind(n: u8) -> TileKind {
    TileKind::new(n).unwrap()
}
fn seq(n: u8) -> AgariGroup {
    AgariGroup::Sequence {
        start: kind(n),
        open: false,
    }
}
fn trip(n: u8) -> AgariGroup {
    AgariGroup::Triplet {
        tile: kind(n),
        open: false,
    }
}
fn kan(n: u8, open: bool) -> AgariGroup {
    AgariGroup::Kan {
        tile: kind(n),
        open,
    }
}
fn standard(groups: [AgariGroup; 4], pair: u8) -> AgariPattern {
    AgariPattern::Standard {
        groups: groups.to_vec(),
        pair: kind(pair),
    }
}
fn context(tile: u8) -> AgariContext {
    AgariContext {
        winning_tile: kind(tile),
        win_method: WinMethod::Ron(RonSource::Discard),
        round_wind: Wind::East,
        seat_wind: Wind::South,
        riichi: RiichiStatus::None,
    }
}
fn check(pattern: &AgariPattern, context: AgariContext, present: &[Yaku], absent: &[Yaku]) {
    for interpretation in interpretations(pattern, context.winning_tile).unwrap() {
        let actual = detect_yaku(&interpretation, &context).unwrap();
        for yaku in present {
            assert!(
                actual.contains(yaku),
                "{interpretation:?} {context:?}: missing {yaku:?}, got {actual:?}"
            );
        }
        for yaku in absent {
            assert!(
                !actual.contains(yaku),
                "{interpretation:?} {context:?}: unexpected {yaku:?}"
            );
        }
        assert_eq!(
            actual.len(),
            actual
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
        );
    }
}

#[test]
fn same_rank_triplets_include_kans_in_all_ranks() {
    for rank in 0..9 {
        let pattern = standard(
            [trip(rank), kan(rank + 9, true), trip(rank + 18), seq(3)],
            31,
        );
        check(&pattern, context(31), &[Yaku::SanshokuDoukou], &[]);
        let different = standard(
            [
                trip(rank),
                kan((rank + 1) % 9 + 9, true),
                trip(rank + 18),
                seq(3),
            ],
            31,
        );
        check(&different, context(31), &[], &[Yaku::SanshokuDoukou]);
    }
}

#[test]
fn wind_yaku_count_kans_and_double_wind_separately() {
    let winds = [Wind::East, Wind::South, Wind::West, Wind::North];
    let pattern = standard([trip(27), kan(28, true), trip(29), trip(30)], 31);
    for round_wind in winds {
        for seat_wind in winds {
            check(
                &pattern,
                AgariContext {
                    round_wind,
                    seat_wind,
                    ..context(31)
                },
                &[Yaku::Daisuushi, Yaku::Bakaze, Yaku::Jikaze],
                &[Yaku::Shousuushi, Yaku::Haku],
            );
        }
    }
    for pair in 27..31 {
        let others: Vec<_> = (27..31).filter(|&tile| tile != pair).collect();
        let groups = [
            trip(others[0]),
            kan(others[1], false),
            trip(others[2]),
            seq(1),
        ];
        check(
            &standard(groups, pair),
            context(pair),
            &[Yaku::Shousuushi],
            &[Yaku::Daisuushi],
        );
        check(
            &standard(groups, 31),
            context(31),
            &[],
            &[Yaku::Shousuushi, Yaku::Daisuushi],
        );
    }
}

#[test]
fn green_hand_accepts_sequences_and_does_not_require_green_dragon() {
    for groups in [
        [seq(19), trip(23), kan(25, true), trip(32)],
        [seq(19), seq(19), trip(23), kan(25, false)],
    ] {
        check(&standard(groups, 20), context(20), &[Yaku::Ryuuiisou], &[]);
        for pair in [18, 22, 24, 26, 27, 31, 33, 1] {
            check(
                &standard(groups, pair),
                context(pair),
                &[],
                &[Yaku::Ryuuiisou],
            );
        }
    }
    check(
        &standard([seq(18), trip(23), kan(25, true), trip(32)], 20),
        context(20),
        &[],
        &[Yaku::Ryuuiisou],
    );
}

#[test]
fn three_and_four_kans_include_open_and_concealed_kans() {
    for open in [false, true] {
        check(
            &standard([kan(1, open), kan(10, false), kan(19, true), seq(22)], 31),
            context(31),
            &[Yaku::Sankantsu],
            &[Yaku::Suukantsu],
        );
        check(
            &standard(
                [kan(1, open), kan(10, false), kan(19, true), kan(22, false)],
                31,
            ),
            context(31),
            &[Yaku::Suukantsu],
            &[Yaku::Sankantsu],
        );
        check(
            &standard([kan(1, open), kan(10, false), trip(19), seq(22)], 31),
            context(31),
            &[],
            &[Yaku::Sankantsu, Yaku::Suukantsu],
        );
    }
}

#[test]
fn nine_gates_and_pure_nine_gates_cover_all_suits_and_extra_tiles() {
    for suit in 0..3 {
        for extra in 0..9 {
            let base = suit * 9;
            let tiles: Vec<_> = [0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 8, 8, extra]
                .map(|tile| Tile::new(base + tile).unwrap())
                .to_vec();
            let hand = Hand::new(tiles, vec![]).unwrap();
            let patterns = patterns(&hand);
            assert!(!patterns.is_empty());
            for pattern in patterns {
                check(
                    &pattern,
                    context(base + extra),
                    &[Yaku::JunseiChuurenPoutou, Yaku::Chinitsu],
                    &[Yaku::ChuurenPoutou],
                );
                let other = if extra == 0 { 8 } else { 0 };
                check(
                    &pattern,
                    context(base + other),
                    &[Yaku::ChuurenPoutou, Yaku::Chinitsu],
                    &[Yaku::JunseiChuurenPoutou],
                );
            }
        }
    }
}

#[test]
fn nine_gates_rejects_missing_ranks_open_groups_and_concealed_kans() {
    let absent = [Yaku::ChuurenPoutou, Yaku::JunseiChuurenPoutou];
    check(
        &standard([trip(0), seq(3), seq(3), trip(8)], 6),
        context(6),
        &[],
        &absent,
    );
    check(
        &standard([kan(0, false), seq(1), seq(4), trip(8)], 7),
        context(7),
        &[],
        &absent,
    );
    let mut groups = [trip(0), seq(1), seq(4), trip(8)];
    groups[0] = AgariGroup::Triplet {
        tile: kind(0),
        open: true,
    };
    check(&standard(groups, 7), context(7), &[], &absent);
}

#[test]
fn kokushi_wait_depends_on_the_actual_winning_tile() {
    let terminals = [0, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 33];
    for pair in terminals {
        let pattern = AgariPattern::Kokushi { pair: kind(pair) };
        for winning_tile in terminals {
            let (present, absent) = if winning_tile == pair {
                (Yaku::KokushiJuusanmen, Yaku::Kokushi)
            } else {
                (Yaku::Kokushi, Yaku::KokushiJuusanmen)
            };
            check(&pattern, context(winning_tile), &[present], &[absent]);
        }
    }
}

#[test]
fn tsumo_requires_menzen_but_rinshan_and_last_wall_allow_open_hands() {
    for open in [false, true] {
        let pattern = standard([kan(1, open), seq(10), seq(21), seq(4)], 31);
        for (source, event) in [
            (TsumoSource::Rinshan, Yaku::RinshanKaihou),
            (TsumoSource::LastWall, Yaku::Haitei),
        ] {
            let ctx = AgariContext {
                win_method: WinMethod::Tsumo(source),
                ..context(31)
            };
            check(
                &pattern,
                ctx,
                &[event],
                &[if event == Yaku::Haitei {
                    Yaku::RinshanKaihou
                } else {
                    Yaku::Haitei
                }],
            );
            if open {
                check(&pattern, ctx, &[], &[Yaku::MenzenTsumo]);
            } else {
                check(&pattern, ctx, &[Yaku::MenzenTsumo], &[]);
            }
        }
    }
}

#[test]
fn first_draw_and_riichi_also_apply_to_special_patterns() {
    let seven_pairs = AgariPattern::Chiitoitsu {
        pairs: [1, 3, 5, 10, 12, 19, 21].map(kind).to_vec(),
    };
    for pattern in [&seven_pairs, &AgariPattern::Kokushi { pair: kind(0) }] {
        let tile = if matches!(pattern, AgariPattern::Chiitoitsu { .. }) {
            1
        } else {
            0
        };
        for seat_wind in [Wind::East, Wind::South, Wind::West, Wind::North] {
            let (present, absent) = if seat_wind == Wind::East {
                (Yaku::Tenhou, Yaku::Chiihou)
            } else {
                (Yaku::Chiihou, Yaku::Tenhou)
            };
            check(
                pattern,
                AgariContext {
                    seat_wind,
                    win_method: WinMethod::Tsumo(TsumoSource::FirstDraw),
                    ..context(tile)
                },
                &[present, Yaku::MenzenTsumo],
                &[absent],
            );
        }
        check(
            pattern,
            AgariContext {
                riichi: RiichiStatus::DoubleRiichi { ippatsu: true },
                ..context(tile)
            },
            &[Yaku::DoubleRiichi, Yaku::Ippatsu],
            &[Yaku::Riichi],
        );
    }
}

#[test]
fn ippatsu_can_combine_with_robbing_an_added_kan() {
    let pattern = standard([seq(1), seq(4), seq(10), seq(21)], 13);
    check(
        &pattern,
        AgariContext {
            win_method: WinMethod::Ron(RonSource::Kakan),
            riichi: RiichiStatus::Riichi { ippatsu: true },
            ..context(1)
        },
        &[Yaku::Riichi, Yaku::Ippatsu, Yaku::Chankan, Yaku::Pinfu],
        &[Yaku::Houtei, Yaku::MenzenTsumo],
    );
}

#[test]
fn contradictory_contexts_return_specific_errors() {
    let closed = standard([seq(1), seq(4), seq(10), seq(21)], 13);
    let opened = standard(
        [
            AgariGroup::Sequence {
                start: kind(1),
                open: true,
            },
            seq(4),
            seq(10),
            seq(21),
        ],
        13,
    );
    let with_kan = standard([kan(1, false), seq(4), seq(10), seq(21)], 13);
    let riichi = RiichiStatus::Riichi { ippatsu: true };
    let first_draw = WinMethod::Tsumo(TsumoSource::FirstDraw);
    let rinshan = WinMethod::Tsumo(TsumoSource::Rinshan);
    for (pattern, ctx, error) in [
        (
            &opened,
            AgariContext {
                riichi,
                ..context(13)
            },
            YakuDetectionError::RiichiRequiresClosedHand,
        ),
        (
            &closed,
            AgariContext {
                riichi,
                win_method: first_draw,
                ..context(13)
            },
            YakuDetectionError::FirstDrawWithRiichi,
        ),
        (
            &opened,
            AgariContext {
                win_method: first_draw,
                ..context(13)
            },
            YakuDetectionError::FirstDrawRequiresInitialHand,
        ),
        (
            &with_kan,
            AgariContext {
                win_method: first_draw,
                ..context(13)
            },
            YakuDetectionError::FirstDrawRequiresInitialHand,
        ),
        (
            &closed,
            AgariContext {
                win_method: rinshan,
                ..context(13)
            },
            YakuDetectionError::RinshanRequiresKan,
        ),
        (
            &with_kan,
            AgariContext {
                riichi,
                win_method: rinshan,
                ..context(13)
            },
            YakuDetectionError::IppatsuWithRinshan,
        ),
        (
            &closed,
            AgariContext {
                win_method: WinMethod::Ron(RonSource::Kakan),
                ..context(13)
            },
            YakuDetectionError::RobbedKanTileAlreadyHeld {
                winning_tile: kind(13),
            },
        ),
    ] {
        for interpretation in interpretations(pattern, ctx.winning_tile).unwrap() {
            assert_eq!(detect_yaku(&interpretation, &ctx), Err(error));
        }
    }
    let kokushi = AgariPattern::Kokushi { pair: kind(0) };
    let interpretation = interpretations(&kokushi, kind(8)).unwrap()[0];
    assert_eq!(
        detect_yaku(
            &interpretation,
            &AgariContext {
                win_method: WinMethod::Ron(RonSource::Ankan),
                ..context(8)
            }
        ),
        Err(YakuDetectionError::RobbingAnkanUnsupported)
    );
}
