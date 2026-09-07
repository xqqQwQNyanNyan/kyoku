//! 枚举合法拆分并按实际基础支付选择最高计分，保留同点数的不同解释。

use super::{
    AgariContext, BonusHan, HandValue, Payments, PointsError, Yaku,
    agari::{self, AgariGroup, AgariPattern, WinningPosition},
    calculate_bonus_han, calculate_hand_value, calculate_payments, detect_yaku,
};
use crate::mahjong::{hand::Hand, round::Wind, tile::Tile};

#[derive(Debug)]
pub(crate) enum WinValueError {
    InvalidHandSize,
    InvalidInterpretation(agari::AgariError),
    Yaku(super::YakuDetectionError),
    Scoring(super::ScoringError),
    Bonus(super::BonusError),
    Points(PointsError),
}

impl std::fmt::Display for WinValueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidHandSize => write!(f, "计分需要等效 14 张完整手牌"),
            Self::InvalidInterpretation(e) => e.fmt(f),
            Self::Yaku(e) => e.fmt(f),
            Self::Scoring(e) => e.fmt(f),
            Self::Bonus(e) => e.fmt(f),
            Self::Points(e) => e.fmt(f),
        }
    }
}

pub(crate) struct WinValue {
    pub yaku: Vec<Yaku>,
    pub value: HandValue,
    pub bonus: BonusHan,
    pub payments: Payments,
    pub wait: &'static str,
}

pub(crate) fn total_payment(payments: &Payments) -> u64 {
    match *payments {
        Payments::Ron { amount } => u64::from(amount),
        Payments::DealerTsumo { each } => 3 * u64::from(each),
        Payments::NonDealerTsumo {
            dealer,
            each_non_dealer,
        } => u64::from(dealer) + 2 * u64::from(each_non_dealer),
    }
}

/// 无符合役条件的解释时返回空列表；不将牌形完成误当作有役或未振听。
pub(crate) fn best_win_values(
    hand: &Hand,
    context: &AgariContext,
    dora: &[Tile],
) -> Result<Vec<WinValue>, WinValueError> {
    if hand.effective_tile_count() != Hand::MAX_TILE_COUNT {
        return Err(WinValueError::InvalidHandSize);
    }
    let mut best = Vec::new();
    let mut best_payment = 0;
    for pattern in agari::patterns(hand) {
        for interpretation in agari::interpretations(&pattern, context.winning_tile)
            .map_err(WinValueError::InvalidInterpretation)?
        {
            let yaku = detect_yaku(&interpretation, context).map_err(WinValueError::Yaku)?;
            let value = calculate_hand_value(&interpretation, context, &yaku)
                .map_err(WinValueError::Scoring)?;
            // 未知里宝牌不作假设；自家已有赤牌由实际手牌统计。
            let bonus = calculate_bonus_han(hand, context.riichi, dora, &[])
                .map_err(WinValueError::Bonus)?;
            let payments = match calculate_payments(
                &value,
                &bonus,
                context.seat_wind == Wind::East,
                context.win_method,
            ) {
                Ok(payments) => payments,
                Err(PointsError::NoYaku) => continue,
                Err(e) => return Err(WinValueError::Points(e)),
            };
            let amount = total_payment(&payments);
            if amount < best_payment {
                continue;
            }
            if amount > best_payment {
                best.clear();
                best_payment = amount;
            }
            let wait = match interpretation.winning_position() {
                WinningPosition::Pair => "tanki",
                WinningPosition::Chiitoitsu => "chiitoitsu_tanki",
                WinningPosition::Kokushi => match pattern {
                    AgariPattern::Kokushi { pair } if pair == context.winning_tile => {
                        "kokushi_thirteen_sided"
                    }
                    _ => "kokushi_single",
                },
                WinningPosition::Group(index) => match &pattern {
                    AgariPattern::Standard { groups, .. } => match groups[index] {
                        AgariGroup::Sequence { start, .. } => {
                            let offset = context.winning_tile.as_u8() - start.as_u8();
                            if offset == 1 {
                                "kanchan"
                            } else if (start.as_u8() % 9 == 0 && offset == 2)
                                || (start.as_u8() % 9 == 6 && offset == 0)
                            {
                                "penchan"
                            } else {
                                "ryanmen"
                            }
                        }
                        _ => "shanpon",
                    },
                    _ => unreachable!("面子归属只来自普通型"),
                },
            };
            if !best
                .iter()
                .any(|b: &WinValue| b.yaku == yaku && b.value == value && b.wait == wait)
            {
                best.push(WinValue {
                    yaku,
                    value,
                    bonus,
                    payments,
                    wait,
                });
            }
        }
    }
    Ok(best)
}

#[cfg(test)]
mod tests;
