use kyoku::analysis::agari::{interpretations, patterns};
use kyoku::analysis::{
    AgariContext, BonusError, BonusHan, HandValue, RiichiStatus, RonSource, TsumoSource, WinMethod,
    calculate_bonus_han, calculate_hand_value, detect_yaku,
};
use kyoku::mahjong::{hand::Hand, meld::Meld, player_index::PlayerIndex, round::Wind, tile::Tile};

fn tile(value: u8) -> Tile {
    Tile::new(value).unwrap()
}

fn hand(concealed: &[u8], melds: Vec<Meld>) -> Hand {
    Hand::new(concealed.iter().map(|&value| tile(value)).collect(), melds).unwrap()
}

fn context(winning: Tile, win_method: WinMethod, riichi: RiichiStatus) -> AgariContext {
    AgariContext {
        winning_tile: winning.kind(),
        win_method,
        round_wind: Wind::East,
        seat_wind: Wind::South,
        riichi,
    }
}

fn hand_value(hand: &Hand, context: &AgariContext) -> HandValue {
    let patterns = patterns(hand);
    let interpretations = interpretations(&patterns[0], context.winning_tile).unwrap();
    let interpretation = &interpretations[0];
    let yaku = detect_yaku(interpretation, context).unwrap();
    calculate_hand_value(interpretation, context, &yaku).unwrap()
}

// 目标牌出现三张，其他牌最多两张，避免错误的指示牌映射碰巧得到相同计数。
fn hand_with_target_triplet(target: u8) -> Hand {
    let other_suit = if target < 27 { (target / 9 + 1) % 3 } else { 0 };
    let mut concealed = (other_suit * 9..other_suit * 9 + 9).collect::<Vec<_>>();
    concealed.extend([target; 3]);
    concealed.extend([if target == 27 { 28 } else { 27 }; 2]);
    hand(&concealed, vec![])
}

fn three_red_fives() -> Hand {
    hand(&[2, 3, 34, 11, 12, 35, 20, 21, 36, 6, 7, 8, 27, 27], vec![])
}

#[test]
fn indicators_follow_every_number_wind_and_dragon_cycle() {
    let successors = [
        1, 2, 3, 4, 5, 6, 7, 8, 0, 10, 11, 12, 13, 14, 15, 16, 17, 9, 19, 20, 21, 22, 23, 24, 25,
        26, 18, 28, 29, 30, 27, 32, 33, 31,
    ];
    for (indicator, target) in successors.into_iter().enumerate() {
        let hand = hand_with_target_triplet(target);
        let indicators = [tile(indicator as u8)];
        assert_eq!(
            calculate_bonus_han(
                &hand,
                RiichiStatus::Riichi { ippatsu: false },
                &indicators,
                &indicators
            )
            .unwrap(),
            BonusHan {
                dora: 3,
                aka_dora: 0,
                ura_dora: 3
            },
            "indicator {indicator}, target {target}"
        );
    }
}

#[test]
fn empty_indicators_still_count_all_three_red_fives() {
    let bonus = calculate_bonus_han(&three_red_fives(), RiichiStatus::None, &[], &[]).unwrap();
    assert_eq!(
        bonus,
        BonusHan {
            dora: 0,
            aka_dora: 3,
            ura_dora: 0
        }
    );
    assert_eq!(bonus.total_han(), 3);
    let no_red = hand_with_target_triplet(4);
    assert_eq!(
        calculate_bonus_han(&no_red, RiichiStatus::None, &[], &[]).unwrap(),
        BonusHan::default()
    );
}

#[test]
fn multiple_and_repeated_indicators_stack_without_counting_as_yaku() {
    let hand = three_red_fives();
    let context = context(
        tile(36),
        WinMethod::Ron(RonSource::Discard),
        RiichiStatus::Riichi { ippatsu: false },
    );
    let bonus = calculate_bonus_han(
        &hand,
        context.riichi,
        &[3, 3, 12, 21].map(tile),
        &[3, 12, 12, 21].map(tile),
    )
    .unwrap();
    assert_eq!(
        bonus,
        BonusHan {
            dora: 4,
            aka_dora: 3,
            ura_dora: 4
        }
    );
    assert_eq!(bonus.total_han(), 11);
    let value = hand_value(&hand, &context);
    // 三色同顺 2 + 立直 1；宝牌合计后超过 13 番也不转成数え役满。
    assert_eq!(value.han, 3);
    assert_eq!(value.total_han(&bonus), 14);
    assert_eq!(value.yakuman, 0);
}

#[test]
fn red_indicator_points_to_six_and_does_not_itself_count_as_aka() {
    for (red, six) in [(34, 5), (35, 14), (36, 23)] {
        let hand = hand_with_target_triplet(six);
        let bonus = calculate_bonus_han(&hand, RiichiStatus::None, &[tile(red)], &[]).unwrap();
        assert_eq!(
            bonus,
            BonusHan {
                dora: 3,
                aka_dora: 0,
                ura_dora: 0
            }
        );
    }
}

#[test]
fn ura_requires_established_riichi_including_double_riichi() {
    let hand = hand(&[0, 1, 2, 3, 34, 5, 9, 10, 11, 18, 19, 20, 29, 29], vec![]);
    for (riichi, ura) in [
        (RiichiStatus::None, 0),
        (RiichiStatus::Riichi { ippatsu: false }, 2),
        (RiichiStatus::Riichi { ippatsu: true }, 2),
        (RiichiStatus::DoubleRiichi { ippatsu: false }, 2),
        (RiichiStatus::DoubleRiichi { ippatsu: true }, 2),
    ] {
        let bonus = calculate_bonus_han(&hand, riichi, &[tile(3)], &[tile(3); 2]).unwrap();
        assert_eq!(
            bonus,
            BonusHan {
                dora: 1,
                aka_dora: 1,
                ura_dora: ura
            }
        );
        assert_eq!(bonus.total_han(), 2 + ura);
    }
}

#[test]
fn winning_red_tile_is_included_once_for_ron_and_tsumo() {
    let mut hand = hand(&[0, 1, 2, 3, 5, 9, 10, 11, 18, 19, 20, 29, 29], vec![]);
    assert_eq!(
        calculate_bonus_han(&hand, RiichiStatus::None, &[tile(3)], &[]),
        Err(BonusError::InvalidHandSize { actual: 13 })
    );
    hand.draw(tile(34)).unwrap();
    let before = hand.clone();
    for (method, yaku_han) in [
        (WinMethod::Ron(RonSource::Discard), 2),
        (WinMethod::Tsumo(TsumoSource::Wall), 3),
    ] {
        let context = context(tile(34), method, RiichiStatus::None);
        let bonus = calculate_bonus_han(&hand, context.riichi, &[tile(3)], &[]).unwrap();
        assert_eq!(
            bonus,
            BonusHan {
                dora: 1,
                aka_dora: 1,
                ura_dora: 0
            }
        );
        let value = hand_value(&hand, &context);
        assert_eq!(value.han, yaku_han);
        assert_eq!(value.total_han(&bonus), yaku_han + 2);
        assert_eq!(value.total_han(&BonusHan::default()), yaku_han);
    }
    assert_eq!(hand, before);
}

#[test]
fn all_meld_types_count_actual_tiles_and_called_red_tile_once() {
    let from = PlayerIndex::new(1).unwrap();
    for (meld, dora) in [
        (
            Meld::Chi {
                tiles: [3, 34, 5].map(tile),
                called: tile(34),
                from,
            },
            1,
        ),
        (
            Meld::Pon {
                tiles: [34, 4, 4].map(tile),
                called: tile(34),
                from,
            },
            3,
        ),
        (
            Meld::Daiminkan {
                tiles: [34, 4, 4, 4].map(tile),
                called: tile(34),
                from,
            },
            4,
        ),
        (
            Meld::Ankan {
                tiles: [34, 4, 4, 4].map(tile),
            },
            4,
        ),
        (
            Meld::Kakan {
                tiles: [34, 4, 4, 4].map(tile),
                called: tile(34),
                from,
            },
            4,
        ),
    ] {
        let hand = hand(&[9, 10, 11, 12, 13, 14, 18, 19, 20, 29, 29], vec![meld]);
        let bonus = calculate_bonus_han(&hand, RiichiStatus::None, &[tile(3)], &[]).unwrap();
        assert_eq!(
            bonus,
            BonusHan {
                dora,
                aka_dora: 1,
                ura_dora: 0
            },
            "{meld:?}"
        );
    }
}

#[test]
fn four_kans_and_five_indicators_count_all_eighteen_tiles() {
    let hand = hand(
        &[31, 31],
        [0, 9, 18, 27]
            .map(|kind| Meld::Ankan {
                tiles: [tile(kind); 4],
            })
            .to_vec(),
    );
    let indicators = [8, 17, 26, 30, 33].map(tile);
    let bonus = calculate_bonus_han(
        &hand,
        RiichiStatus::Riichi { ippatsu: false },
        &indicators,
        &indicators,
    )
    .unwrap();
    assert_eq!(hand.effective_tile_count(), 14);
    assert_eq!(
        bonus,
        BonusHan {
            dora: 18,
            aka_dora: 0,
            ura_dora: 18
        }
    );
    assert_eq!(bonus.total_han(), 36);
}

#[test]
fn bonus_cannot_make_a_yakuless_hand_eligible() {
    let hand = hand(
        &[12, 35, 14, 24, 25, 26, 19, 20, 21, 29, 29],
        vec![Meld::Chi {
            tiles: [0, 1, 2].map(tile),
            called: tile(0),
            from: PlayerIndex::new(1).unwrap(),
        }],
    );
    let context = context(
        tile(19),
        WinMethod::Ron(RonSource::Discard),
        RiichiStatus::None,
    );
    let bonus = calculate_bonus_han(&hand, context.riichi, &[tile(12)], &[tile(12)]).unwrap();
    assert_eq!(
        bonus,
        BonusHan {
            dora: 1,
            aka_dora: 1,
            ura_dora: 0
        }
    );
    assert_eq!(bonus.total_han(), 2);
    let value = hand_value(&hand, &context);
    assert_eq!(value.han, 0);
    assert_eq!(value.yakuman, 0);
    assert_eq!(value.total_han(&bonus), 0);
}

#[test]
fn yakuman_keeps_bonus_breakdown_but_does_not_add_any_han() {
    let hand = hand(&[0, 0, 0, 34, 4, 4, 9, 9, 9, 18, 18, 18, 31, 31], vec![]);
    let context = context(
        tile(31),
        WinMethod::Ron(RonSource::Discard),
        RiichiStatus::Riichi { ippatsu: false },
    );
    let bonus = calculate_bonus_han(&hand, context.riichi, &[tile(3)], &[tile(3)]).unwrap();
    assert_eq!(
        bonus,
        BonusHan {
            dora: 3,
            aka_dora: 1,
            ura_dora: 3
        }
    );
    let value = hand_value(&hand, &context);
    assert_eq!(
        value,
        HandValue {
            fu: None,
            han: 0,
            yakuman: 1
        }
    );
    assert_eq!(value.total_han(&bonus), 0);
}
