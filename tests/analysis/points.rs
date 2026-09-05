use kyoku::analysis::agari::{interpretations, patterns};
use kyoku::analysis::{
    AgariContext, BonusHan, HandValue, Payments, PointsError, RiichiStatus, RonSource, TsumoSource,
    WinMethod, calculate_bonus_han, calculate_hand_value, calculate_payments, detect_yaku,
};
use kyoku::mahjong::{hand::Hand, round::Wind, tile::Tile};

const RON: WinMethod = WinMethod::Ron(RonSource::Discard);
const TSUMO: WinMethod = WinMethod::Tsumo(TsumoSource::Wall);

fn ordinary(fu: u32, han: u32) -> HandValue {
    HandValue {
        fu: Some(fu),
        han,
        yakuman: 0,
    }
}

fn yakuman(multiple: u32) -> HandValue {
    HandValue {
        fu: None,
        han: 0,
        yakuman: multiple,
    }
}

fn assert_payments(value: &HandValue, bonus: &BonusHan, expected: [u32; 4]) {
    let [
        non_dealer_ron,
        dealer_ron,
        tsumo_dealer_payment,
        tsumo_non_dealer_payment,
    ] = expected;
    for (is_dealer, method, payment) in [
        (
            false,
            RON,
            Payments::Ron {
                amount: non_dealer_ron,
            },
        ),
        (true, RON, Payments::Ron { amount: dealer_ron }),
        (
            true,
            TSUMO,
            Payments::DealerTsumo {
                each: tsumo_dealer_payment,
            },
        ),
        (
            false,
            TSUMO,
            Payments::NonDealerTsumo {
                dealer: tsumo_dealer_payment,
                each_non_dealer: tsumo_non_dealer_payment,
            },
        ),
    ] {
        assert_eq!(
            calculate_payments(value, bonus, is_dealer, method),
            Ok(payment),
            "{value:?}, {bonus:?}, dealer {is_dealer}, {method:?}"
        );
    }
}

#[test]
fn ordinary_hands_round_each_payment_to_one_hundred() {
    for (fu, han, expected) in [
        (30, 1, [1000, 1500, 500, 300]),
        (40, 1, [1300, 2000, 700, 400]),
        (50, 1, [1600, 2400, 800, 400]),
        (30, 2, [2000, 2900, 1000, 500]),
        (60, 2, [3900, 5800, 2000, 1000]),
        (40, 3, [5200, 7700, 2600, 1300]),
        (110, 1, [3600, 5300, 1800, 900]),
    ] {
        assert_payments(&ordinary(fu, han), &BonusHan::default(), expected);
    }
}

#[test]
fn twenty_and_twenty_five_fu_are_preserved() {
    let no_bonus = BonusHan::default();
    assert_eq!(
        calculate_payments(&ordinary(20, 2), &no_bonus, true, TSUMO),
        Ok(Payments::DealerTsumo { each: 700 })
    );
    assert_eq!(
        calculate_payments(&ordinary(20, 2), &no_bonus, false, TSUMO),
        Ok(Payments::NonDealerTsumo {
            dealer: 700,
            each_non_dealer: 400
        })
    );
    assert_eq!(
        calculate_payments(&ordinary(25, 2), &no_bonus, false, RON),
        Ok(Payments::Ron { amount: 1600 })
    );
    assert_eq!(
        calculate_payments(&ordinary(25, 2), &no_bonus, true, RON),
        Ok(Payments::Ron { amount: 2400 })
    );
    assert_payments(&ordinary(25, 3), &no_bonus, [3200, 4800, 1600, 800]);
    assert_payments(&ordinary(25, 4), &no_bonus, [6400, 9600, 3200, 1600]);
}

#[test]
fn mangan_applies_at_five_han_or_two_thousand_basic_points() {
    for (fu, han) in [(30, 5), (70, 3), (40, 4)] {
        assert_payments(
            &ordinary(fu, han),
            &BonusHan::default(),
            [8000, 12000, 4000, 2000],
        );
    }
}

#[test]
fn four_han_thirty_fu_and_three_han_sixty_fu_do_not_round_up_to_mangan() {
    for (fu, han) in [(30, 4), (60, 3)] {
        assert_payments(
            &ordinary(fu, han),
            &BonusHan::default(),
            [7700, 11600, 3900, 2000],
        );
    }
}

#[test]
fn every_limit_tier_includes_both_han_boundaries() {
    for (han, expected) in [
        (5, [8000, 12000, 4000, 2000]),
        (6, [12000, 18000, 6000, 3000]),
        (7, [12000, 18000, 6000, 3000]),
        (8, [16000, 24000, 8000, 4000]),
        (9, [16000, 24000, 8000, 4000]),
        (10, [16000, 24000, 8000, 4000]),
        (11, [24000, 36000, 12000, 6000]),
        (12, [24000, 36000, 12000, 6000]),
        (13, [32000, 48000, 16000, 8000]),
    ] {
        assert_payments(&ordinary(30, han), &BonusHan::default(), expected);
    }
}

#[test]
fn thirteen_or_more_han_always_scores_one_counted_yakuman() {
    for han in [13, 14, 26, 39, 100, u32::MAX] {
        assert_payments(
            &ordinary(30, han),
            &BonusHan::default(),
            [32000, 48000, 16000, 8000],
        );
    }
}

#[test]
fn actual_yakuman_multiples_stack_for_all_payment_types() {
    for (multiple, expected) in [
        (1, [32000, 48000, 16000, 8000]),
        (2, [64000, 96000, 32000, 16000]),
        (3, [96000, 144000, 48000, 24000]),
        (4, [128000, 192000, 64000, 32000]),
    ] {
        assert_payments(&yakuman(multiple), &BonusHan::default(), expected);
    }
}

#[test]
fn ordinary_yaku_and_all_bonus_kinds_determine_the_limit_tier() {
    let value = ordinary(30, 1);
    let bonus = BonusHan {
        dora: 2,
        aka_dora: 1,
        ura_dora: 1,
    };
    assert_eq!(value.total_han(&bonus), 5);
    assert_payments(&value, &bonus, [8000, 12000, 4000, 2000]);

    let value = ordinary(30, 2);
    let bonus = BonusHan {
        dora: 4,
        aka_dora: 3,
        ura_dora: 4,
    };
    assert_eq!(value.total_han(&bonus), 13);
    assert_payments(&value, &bonus, [32000, 48000, 16000, 8000]);
}

#[test]
fn yakuman_ignores_fu_and_all_ordinary_han_even_if_they_would_overflow() {
    let bonus = BonusHan {
        dora: u32::MAX,
        aka_dora: u32::MAX,
        ura_dora: u32::MAX,
    };
    for fu in [None, Some(0), Some(25)] {
        let value = HandValue {
            fu,
            han: u32::MAX,
            yakuman: 2,
        };
        assert_payments(&value, &bonus, [64000, 96000, 32000, 16000]);
    }
}

#[test]
fn no_yaku_is_an_error_even_with_large_bonus_han() {
    for bonus in [
        BonusHan::default(),
        BonusHan {
            dora: u32::MAX,
            aka_dora: u32::MAX,
            ura_dora: u32::MAX,
        },
    ] {
        for is_dealer in [false, true] {
            for method in [RON, TSUMO] {
                assert_eq!(
                    calculate_payments(&ordinary(30, 0), &bonus, is_dealer, method),
                    Err(PointsError::NoYaku)
                );
            }
        }
    }
}

#[test]
fn missing_and_unrounded_fu_are_rejected_even_for_limit_hands() {
    for han in [1, 5, 13] {
        let value = HandValue {
            fu: None,
            han,
            yakuman: 0,
        };
        assert_eq!(
            calculate_payments(&value, &BonusHan::default(), false, RON),
            Err(PointsError::MissingFu)
        );
        for fu in [0, 1, 19, 21, 24, 26, 29, 31, 32, u32::MAX] {
            assert_eq!(
                calculate_payments(&ordinary(fu, han), &BonusHan::default(), false, RON),
                Err(PointsError::InvalidFu { fu })
            );
        }
    }
}

#[test]
fn large_public_han_and_fu_fields_cannot_overflow_before_capping() {
    let bonus = BonusHan {
        dora: u32::MAX,
        aka_dora: u32::MAX,
        ura_dora: u32::MAX,
    };
    assert_payments(&ordinary(30, u32::MAX), &bonus, [32000, 48000, 16000, 8000]);
    // 点数层只校验符数格式，不反推牌型；大符数先用宽整数计算，再按满贯封顶。
    assert_payments(
        &ordinary(4_294_967_290, 4),
        &BonusHan::default(),
        [8000, 12000, 4000, 2000],
    );
}

#[test]
fn payment_overflow_is_reported_at_the_per_payer_boundary() {
    for (is_dealer, method, multiple, expected, overflow_amount) in [
        (
            true,
            RON,
            89_478,
            Payments::Ron {
                amount: 4_294_944_000,
            },
            4_294_992_000,
        ),
        (
            false,
            RON,
            134_217,
            Payments::Ron {
                amount: 4_294_944_000,
            },
            4_294_976_000,
        ),
        (
            true,
            TSUMO,
            268_435,
            Payments::DealerTsumo {
                each: 4_294_960_000,
            },
            4_294_976_000,
        ),
        (
            false,
            TSUMO,
            268_435,
            Payments::NonDealerTsumo {
                dealer: 4_294_960_000,
                each_non_dealer: 2_147_480_000,
            },
            4_294_976_000,
        ),
    ] {
        assert_eq!(
            calculate_payments(&yakuman(multiple), &BonusHan::default(), is_dealer, method),
            Ok(expected)
        );
        assert_eq!(
            calculate_payments(
                &yakuman(multiple + 1),
                &BonusHan::default(),
                is_dealer,
                method
            ),
            Err(PointsError::PaymentOverflow {
                amount: overflow_amount
            })
        );
    }
    assert_eq!(
        calculate_payments(&yakuman(u32::MAX), &BonusHan::default(), true, RON),
        Err(PointsError::PaymentOverflow {
            amount: 206_158_430_160_000
        })
    );
}

#[test]
fn payment_depends_on_win_method_not_its_event_source() {
    let value = ordinary(30, 5);
    for source in [
        RonSource::Discard,
        RonSource::LastDiscard,
        RonSource::Kakan,
        RonSource::Ankan,
    ] {
        assert_eq!(
            calculate_payments(&value, &BonusHan::default(), false, WinMethod::Ron(source)),
            Ok(Payments::Ron { amount: 8000 })
        );
    }
    for source in [
        TsumoSource::Wall,
        TsumoSource::LastWall,
        TsumoSource::Rinshan,
        TsumoSource::FirstDraw,
    ] {
        assert_eq!(
            calculate_payments(
                &value,
                &BonusHan::default(),
                false,
                WinMethod::Tsumo(source)
            ),
            Ok(Payments::NonDealerTsumo {
                dealer: 4000,
                each_non_dealer: 2000
            })
        );
    }
}

#[test]
fn detected_hand_value_and_bonus_feed_directly_into_payments() {
    let hand = Hand::new(
        [0, 1, 2, 3, 34, 5, 9, 10, 11, 18, 19, 20, 29, 29]
            .map(|tile| Tile::new(tile).unwrap())
            .to_vec(),
        vec![],
    )
    .unwrap();
    let context = AgariContext {
        winning_tile: Tile::new(34).unwrap().kind(),
        win_method: RON,
        round_wind: Wind::East,
        seat_wind: Wind::South,
        riichi: RiichiStatus::None,
    };
    let patterns = patterns(&hand);
    let candidates = interpretations(&patterns[0], context.winning_tile).unwrap();
    let interpretation = &candidates[0];
    let yaku = detect_yaku(interpretation, &context).unwrap();
    let value = calculate_hand_value(interpretation, &context, &yaku).unwrap();
    let bonus = calculate_bonus_han(&hand, context.riichi, &[Tile::new(3).unwrap()], &[]).unwrap();
    assert_eq!(value, ordinary(40, 2));
    assert_eq!(
        bonus,
        BonusHan {
            dora: 1,
            aka_dora: 1,
            ura_dora: 0
        }
    );
    assert_eq!(
        calculate_payments(
            &value,
            &bonus,
            context.seat_wind == Wind::East,
            context.win_method
        ),
        Ok(Payments::Ron { amount: 8000 })
    );
}
