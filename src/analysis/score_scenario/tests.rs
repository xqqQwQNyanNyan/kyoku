use super::*;
fn p(i: u8) -> PlayerIndex {
    PlayerIndex::new(i).unwrap()
}

#[test]
fn single_winner_settlement_conserves_points_plus_existing_sticks() {
    let result = apply_win(
        [25000; 4],
        p(1),
        p(0),
        Some(p(2)),
        &Payments::Ron { amount: 3900 },
        2,
        1,
    )
    .unwrap();
    assert_eq!(result.deltas, [0, 5500, -4500, 0]);
    assert_eq!(result.scores, [25000, 30500, 20500, 25000]);
    assert_eq!(result.ranks, [2, 1, 4, 3]);
    assert_eq!(result.deltas.iter().sum::<i64>(), 1000);
    let result = apply_win(
        [25000; 4],
        p(1),
        p(0),
        None,
        &Payments::NonDealerTsumo {
            dealer: 2000,
            each_non_dealer: 1000,
        },
        1,
        2,
    )
    .unwrap();
    assert_eq!(result.deltas, [-2100, 6300, -1100, -1100]);
    assert_eq!(result.deltas.iter().sum::<i64>(), 2000);
}

#[test]
fn rejects_impossible_payers_and_uses_wide_score_arithmetic() {
    assert!(matches!(
        apply_win(
            [0; 4],
            p(0),
            p(0),
            Some(p(0)),
            &Payments::Ron { amount: 1000 },
            0,
            0
        ),
        Err(ScoreScenarioError::InvalidPayer)
    ));
    assert!(matches!(
        apply_win(
            [0; 4],
            p(1),
            p(0),
            None,
            &Payments::DealerTsumo { each: 1000 },
            0,
            0
        ),
        Err(ScoreScenarioError::WrongDealerPayment)
    ));
    let result = apply_win(
        [i32::MAX; 4],
        p(0),
        p(0),
        None,
        &Payments::DealerTsumo { each: u32::MAX },
        255,
        255,
    )
    .unwrap();
    assert!(result.scores[0] > i64::from(i32::MAX));
    assert_eq!(result.deltas.iter().sum::<i64>(), 255000);
}

#[test]
fn exhaustive_draw_payments_cover_all_tenpai_combinations_and_conserve_points() {
    for mask in 0..16u8 {
        let tenpai = std::array::from_fn(|player| mask & (1 << player) != 0);
        let outcome = apply_exhaustive_draw([25000; 4], tenpai);
        assert_eq!(outcome.deltas.iter().sum::<i64>(), 0);
        let ready = mask.count_ones();
        for (player, &is_tenpai) in tenpai.iter().enumerate() {
            let expected = match ready {
                0 | 4 => 0,
                1 if is_tenpai => 3000,
                1 => -1000,
                2 if is_tenpai => 1500,
                2 => -1500,
                3 if is_tenpai => 1000,
                3 => -3000,
                _ => unreachable!(),
            };
            assert_eq!(outcome.deltas[player], expected);
            assert_eq!(outcome.scores[player], 25000 + expected);
        }
    }
}
