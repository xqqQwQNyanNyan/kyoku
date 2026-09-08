//! 和牌、荒牌流局的条件点数变化，不修改真实牌局，也不预测比赛是否结束。

use super::Payments;
use crate::mahjong::player_index::PlayerIndex;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ScoreScenarioError {
    InvalidPayer,
    WrongDealerPayment,
}

pub(crate) struct ScoreOutcome {
    pub scores: [i64; 4],
    pub deltas: [i64; 4],
    pub ranks: [u8; 4],
}

/// 四人荒牌流局按总计3000点罚符结算。听牌组合是给定条件，不由暗牌猜测。
pub(crate) fn apply_exhaustive_draw(scores: [i32; 4], tenpai: [bool; 4]) -> ScoreOutcome {
    let ready = tenpai.iter().filter(|&&ready| ready).count() as i64;
    let deltas = std::array::from_fn(|player| match ready {
        0 | 4 => 0,
        _ if tenpai[player] => 3000 / ready,
        _ => -3000 / (4 - ready),
    });
    score_outcome(scores, deltas)
}

/// 项目固定玩家0为东一起家；同点按起家顺序比较。本场每家100，荣和合计300。
pub(crate) fn apply_win(
    scores: [i32; 4],
    winner: PlayerIndex,
    dealer: PlayerIndex,
    payer: Option<PlayerIndex>,
    payments: &Payments,
    honba: u8,
    sticks: u8,
) -> Result<ScoreOutcome, ScoreScenarioError> {
    let winner = winner.get_id() as usize;
    let dealer = dealer.get_id() as usize;
    let mut deltas = [0i64; 4];
    let mut pay = |from: usize, amount: i64| {
        deltas[from] -= amount;
        deltas[winner] += amount;
    };
    match *payments {
        Payments::Ron { amount } => {
            let from = payer
                .filter(|p| p.get_id() as usize != winner)
                .ok_or(ScoreScenarioError::InvalidPayer)?
                .get_id() as usize;
            pay(from, i64::from(amount) + i64::from(honba) * 300);
        }
        Payments::DealerTsumo { each } => {
            if payer.is_some() {
                return Err(ScoreScenarioError::InvalidPayer);
            }
            if winner != dealer {
                return Err(ScoreScenarioError::WrongDealerPayment);
            }
            for from in (0..4).filter(|&i| i != winner) {
                pay(from, i64::from(each) + i64::from(honba) * 100);
            }
        }
        Payments::NonDealerTsumo {
            dealer: dealer_amount,
            each_non_dealer,
        } => {
            if payer.is_some() {
                return Err(ScoreScenarioError::InvalidPayer);
            }
            if winner == dealer {
                return Err(ScoreScenarioError::WrongDealerPayment);
            }
            for from in (0..4).filter(|&i| i != winner) {
                pay(
                    from,
                    i64::from(if from == dealer {
                        dealer_amount
                    } else {
                        each_non_dealer
                    }) + i64::from(honba) * 100,
                );
            }
        }
    }
    deltas[winner] += i64::from(sticks) * 1000;
    Ok(score_outcome(scores, deltas))
}

fn score_outcome(scores: [i32; 4], deltas: [i64; 4]) -> ScoreOutcome {
    let scores = std::array::from_fn(|i| i64::from(scores[i]) + deltas[i]);
    let ranks = std::array::from_fn(|i| {
        1 + (0..4)
            .filter(|&j| scores[j] > scores[i] || (scores[j] == scores[i] && j < i))
            .count() as u8
    });
    ScoreOutcome {
        scores,
        deltas,
        ranks,
    }
}

#[cfg(test)]
mod tests;
