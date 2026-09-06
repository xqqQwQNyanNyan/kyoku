use std::error::Error;
use std::fmt;

use crate::analysis::{BonusHan, HandValue, WinMethod};

/// 一次四人麻将和牌的基础支付金额，不包含本场或立直棒。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Payments {
    /// 荣和时放铳者支付的金额。
    Ron { amount: u32 },
    /// 庄家自摸时，三位闲家各自支付的金额。
    DealerTsumo { each: u32 },
    /// 闲家自摸时，庄家及另外两位闲家各自支付的金额。
    NonDealerTsumo { dealer: u32, each_non_dealer: u32 },
}

/// 和牌点数无法计算的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointsError {
    /// 没有普通役或役满，宝牌不能满足有役条件。
    NoYaku,
    /// 非役满手必须提供符数。
    MissingFu,
    /// 符数必须为 20、25，或不小于 30 的 10 的倍数。
    InvalidFu { fu: u32 },
    /// 进位后的单笔支付金额超出 `u32` 范围。
    PaymentOverflow { amount: u64 },
}

impl fmt::Display for PointsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoYaku => formatter.write_str("point calculation requires at least one yaku"),
            Self::MissingFu => formatter.write_str("non-yakuman hand value requires fu"),
            Self::InvalidFu { fu } => write!(
                formatter,
                "invalid fu {fu}; expected 20, 25, or a multiple of 10 starting at 30"
            ),
            Self::PaymentOverflow { amount } => {
                write!(formatter, "payment amount {amount} exceeds u32 range")
            }
        }
    }
}

impl Error for PointsError {}

/// 将单个和牌价值与宝牌番换算成各家基础支付金额。
///
/// `is_dealer` 表示和牌者是否为庄家；`win_method` 只使用荣和／自摸类别，不检查来源。
/// 输入应对应同一次和牌，不重新判役、算符或校验完整和牌合法性。
/// 非役满手要求有普通役及格式合法的符数，番数按普通役与三类宝牌相加。
///
/// 普通基本点为 `fu * 2^(han + 2)`，最高为满贯的 2000；6～7 番跳满、8～10 番倍满、
/// 11～12 番三倍满，13 番以上均按一倍累计役满计点。不采用切上满贯。
/// `yakuman > 0` 时只按役满倍数计算，不读取符数或累计普通番数、宝牌番。
/// 每笔支付独立向上进位到 100 点；超出结果类型范围时返回错误。
/// 不处理本场、立直棒、牌局结算或最佳和牌解释选择。
pub fn calculate_payments(
    value: &HandValue,
    bonus: &BonusHan,
    is_dealer: bool,
    win_method: WinMethod,
) -> Result<Payments, PointsError> {
    let base = basic_points(value, bonus)?;
    match win_method {
        WinMethod::Ron(_) => Ok(Payments::Ron {
            amount: round_payment(base * if is_dealer { 6 } else { 4 })?,
        }),
        WinMethod::Tsumo(_) if is_dealer => Ok(Payments::DealerTsumo {
            each: round_payment(base * 2)?,
        }),
        WinMethod::Tsumo(_) => Ok(Payments::NonDealerTsumo {
            dealer: round_payment(base * 2)?,
            each_non_dealer: round_payment(base)?,
        }),
    }
}

fn basic_points(value: &HandValue, bonus: &BonusHan) -> Result<u64, PointsError> {
    if value.yakuman > 0 {
        return Ok(8000 * u64::from(value.yakuman));
    }
    if value.han == 0 {
        return Err(PointsError::NoYaku);
    }
    let fu = value.fu.ok_or(PointsError::MissingFu)?;
    if fu != 20 && fu != 25 && !(fu >= 30 && fu.is_multiple_of(10)) {
        return Err(PointsError::InvalidFu { fu });
    }
    // 沿用 total_han 的计分语义，但先用宽整数相加，避免公开字段中的大番数溢出。
    let han = u64::from(value.han)
        + u64::from(bonus.dora)
        + u64::from(bonus.aka_dora)
        + u64::from(bonus.ura_dora);
    Ok(match han {
        1..=4 => (u64::from(fu) * (1u64 << (han + 2))).min(2000),
        5 => 2000,
        6..=7 => 3000,
        8..=10 => 4000,
        11..=12 => 6000,
        _ => 8000,
    })
}

fn round_payment(amount: u64) -> Result<u32, PointsError> {
    let amount = amount.div_ceil(100) * 100;
    u32::try_from(amount).map_err(|_| PointsError::PaymentOverflow { amount })
}
