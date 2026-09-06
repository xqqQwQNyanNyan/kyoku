use std::error::Error;
use std::fmt;

use crate::analysis::agari::{AgariGroup, AgariInterpretation, AgariPattern, WinningPosition};
use crate::analysis::{AgariContext, BonusHan, WinMethod, Yaku};
use crate::mahjong::{round::Wind, tile::TileKind};

/// 单个和牌解释的役与符数价值，不包含点数上限或支付金额。
///
/// `han` 只保存普通役的番数，宝牌另行统计并通过 [`Self::total_han`] 合计。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HandValue {
    /// 普通型为进位后的符数，七对子为 25；役满不使用符数，返回 `None`。
    pub fu: Option<u32>,
    /// 普通役的总番数；存在役满时固定为 0。
    pub han: u32,
    /// 役满倍数；当前每个役满均计一倍，复合役满累加。
    pub yakuman: u32,
}

impl HandValue {
    /// 返回普通役与宝牌合计的番数；役满或无役时返回 0。
    ///
    /// `bonus` 应来自同一手完整和牌的统计。宝牌不能满足有役条件，也不增加役满价值。
    /// 本方法不检查其他和牌合法性，不将高番转换成数え役满。
    pub const fn total_han(&self, bonus: &BonusHan) -> u32 {
        if self.yakuman > 0 || self.han == 0 {
            0
        } else {
            self.han + bonus.total_han()
        }
    }
}

/// 和牌价值无法计算的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScoringError {
    /// 上下文与解释使用了不同的和牌张。
    ContextWinningTileMismatch {
        interpretation_tile: TileKind,
        context_tile: TileKind,
    },
    /// 同一役不能重复计入价值；场风和自风是两个不同的役。
    DuplicateYaku(Yaku),
    /// 流局满贯等非和牌计分条件不由本入口处理。
    UnsupportedYaku(Yaku),
}

impl fmt::Display for ScoringError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContextWinningTileMismatch {
                interpretation_tile,
                context_tile,
            } => write!(
                formatter,
                "context winning tile {} does not match interpretation winning tile {}",
                context_tile.as_u8(),
                interpretation_tile.as_u8()
            ),
            Self::DuplicateYaku(yaku) => write!(formatter, "duplicate scoring yaku: {yaku:?}"),
            Self::UnsupportedYaku(yaku) => {
                write!(
                    formatter,
                    "hand value calculation is not supported for {yaku:?}"
                )
            }
        }
    }
}

impl Error for ScoringError {}

/// 计算单个和牌解释的符数、番数或役满倍数。
///
/// `yaku` 应为同一解释和上下文的完整 `detect_yaku` 结果；本函数不重新判役、
/// 校验完整和牌合法性或枚举其他解释。空役列表返回零番，不代表可以合法和牌。
/// 校验和牌张一致性、重复役及不支持的计分条件。
///
/// 当前所有役满形均计一倍，多个不同役满相加；役满返回 `fu: None, han: 0`。
/// 不计算宝牌、累计役满、满贯线、最终点数，也不选择最佳解释。
pub fn calculate_hand_value(
    interpretation: &AgariInterpretation<'_>,
    context: &AgariContext,
    yaku: &[Yaku],
) -> Result<HandValue, ScoringError> {
    if context.winning_tile != interpretation.winning_tile() {
        return Err(ScoringError::ContextWinningTileMismatch {
            interpretation_tile: interpretation.winning_tile(),
            context_tile: context.winning_tile,
        });
    }
    let closed = match interpretation.pattern() {
        AgariPattern::Standard { groups, .. } => groups.iter().all(|group| !group_open(group)),
        _ => true,
    };
    let mut han = 0;
    let mut yakuman = 0;
    for (index, &item) in yaku.iter().enumerate() {
        if yaku[..index].contains(&item) {
            return Err(ScoringError::DuplicateYaku(item));
        }
        match yaku_value(item, closed)? {
            YakuValue::Han(value) => han += value,
            YakuValue::Yakuman(value) => yakuman += value,
        }
    }
    if yakuman > 0 {
        return Ok(HandValue {
            fu: None,
            han: 0,
            yakuman,
        });
    }
    Ok(HandValue {
        fu: calculate_fu(interpretation, context, closed, yaku.contains(&Yaku::Pinfu)),
        han,
        yakuman: 0,
    })
}

enum YakuValue {
    Han(u32),
    Yakuman(u32),
}

// 计分规则集中在这里；将来支持其他役满倍数时，不改变役种检测行为。
fn yaku_value(yaku: Yaku, closed: bool) -> Result<YakuValue, ScoringError> {
    use Yaku::*;
    let han = match yaku {
        Riichi | Ippatsu | MenzenTsumo | Pinfu | Iipeikou | Tanyao | Haku | Hatsu | Chun
        | Bakaze | Jikaze | Haitei | Houtei | RinshanKaihou | Chankan => 1,
        DoubleRiichi | Chiitoitsu | Toitoi | Sanankou | Sankantsu | SanshokuDoukou | Shousangen
        | Honroutou => 2,
        Ryanpeikou => 3,
        SanshokuDoujun | Ittsu | Chanta => 1 + u32::from(closed),
        Junchan | Honitsu => 2 + u32::from(closed),
        Chinitsu => 5 + u32::from(closed),
        Kokushi | KokushiJuusanmen | Suuankou | SuuankouTanki | Daisangen | Shousuushi
        | Daisuushi | Tsuuiisou | Chinroutou | Ryuuiisou | Suukantsu | ChuurenPoutou
        | JunseiChuurenPoutou | Tenhou | Chiihou => return Ok(YakuValue::Yakuman(1)),
        NagashiMangan => return Err(ScoringError::UnsupportedYaku(yaku)),
    };
    Ok(YakuValue::Han(han))
}

fn calculate_fu(
    interpretation: &AgariInterpretation<'_>,
    context: &AgariContext,
    closed: bool,
    pinfu: bool,
) -> Option<u32> {
    let (groups, pair) = match interpretation.pattern() {
        AgariPattern::Chiitoitsu { .. } => return Some(25),
        AgariPattern::Kokushi { .. } => return None,
        AgariPattern::Standard { groups, pair } => (groups, *pair),
    };
    let tsumo = matches!(context.win_method, WinMethod::Tsumo(_));
    if pinfu && tsumo {
        return Some(20);
    }
    let mut fu = 20;
    if tsumo {
        fu += 2;
    } else if closed {
        fu += 10;
    }
    fu += 2 * u32::from(pair.as_u8() >= 31);
    fu += 2 * u32::from(pair.as_u8() == wind_tile(context.round_wind));
    fu += 2 * u32::from(pair.as_u8() == wind_tile(context.seat_wind));

    let position = interpretation.winning_position();
    for (index, group) in groups.iter().enumerate() {
        let (tile, mut group_fu) = match group {
            AgariGroup::Sequence { .. } => continue,
            AgariGroup::Triplet { tile, open } => {
                // 荣和补成的刻子按明刻计符，但整手仍可获得门前荣和加符。
                let ron_triplet = !tsumo && position == WinningPosition::Group(index);
                (*tile, if *open || ron_triplet { 2 } else { 4 })
            }
            AgariGroup::Kan { tile, open } => (*tile, if *open { 8 } else { 16 }),
        };
        if tile.as_u8() >= 27 || matches!(tile.as_u8() % 9, 0 | 8) {
            group_fu *= 2;
        }
        fu += group_fu;
    }
    fu += match position {
        WinningPosition::Pair => 2,
        WinningPosition::Group(index) => match groups[index] {
            AgariGroup::Sequence { start, .. } => {
                let start = start.as_u8();
                let winning = interpretation.winning_tile().as_u8();
                let closed_wait = winning == start + 1;
                let edge_wait = (start % 9 == 0 && winning == start + 2)
                    || (start % 9 == 6 && winning == start);
                if closed_wait || edge_wait { 2 } else { 0 }
            }
            _ => 0,
        },
        _ => 0,
    };
    // 副露的无加符顺子形荣和也至少为 30 符；平和自摸已在上面单独返回。
    Some(fu.div_ceil(10).max(3) * 10)
}

fn group_open(group: &AgariGroup) -> bool {
    match group {
        AgariGroup::Sequence { open, .. }
        | AgariGroup::Triplet { open, .. }
        | AgariGroup::Kan { open, .. } => *open,
    }
}

fn wind_tile(wind: Wind) -> u8 {
    match wind {
        Wind::East => 27,
        Wind::South => 28,
        Wind::West => 29,
        Wind::North => 30,
    }
}
