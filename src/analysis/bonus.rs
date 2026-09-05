use std::error::Error;
use std::fmt;

use crate::analysis::RiichiStatus;
use crate::mahjong::{hand::Hand, tile::Tile};

/// 完整和牌中的宝牌番，独立于役的番数。
///
/// 这里只统计宝牌；是否能计入和牌价值由 `HandValue::total_han` 处理。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BonusHan {
    /// 普通宝牌番，包括已翻开的杠宝牌。
    pub dora: u32,
    /// 赤五的番数，同一张牌也可以计入普通宝牌或里宝牌。
    pub aka_dora: u32,
    /// 里宝牌番，未成立立直时为 0。
    pub ura_dora: u32,
}

impl BonusHan {
    /// 返回三类宝牌的总番数，不表示手牌已经满足有役条件。
    pub const fn total_han(&self) -> u32 {
        self.dora + self.aka_dora + self.ura_dora
    }
}

/// 宝牌统计无法完成的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BonusError {
    /// 需要已包含和了牌、等效张数为 14 的完整手牌。
    InvalidHandSize { actual: usize },
}

impl fmt::Display for BonusError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHandSize { actual } => write!(
                formatter,
                "bonus calculation requires 14 effective tiles including the winning tile; got {actual}"
            ),
        }
    }
}

impl Error for BonusError {}

/// 统计完整和牌中的普通宝牌、赤宝牌和里宝牌。
///
/// `hand` 必须已包含和了牌，等效张数为 14；荣和牌也应先加入暗牌，不能另算一次。
/// 副露按实际牌张统计，杠计四张，不额外重复计算 `Meld::called()`。
/// 立直状态复用 `RiichiStatus`，普通立直和两立直都能计里宝牌。
///
/// 指示牌传入本次和牌适用的全部指示牌，包括杠后新增的指示牌；相同指示牌分别累加。
/// 指示牌自身是否为赤五不增加赤宝牌番。翻牌时机、指示牌数量和立直历史由调用方保证。
/// 本函数不判役，也不检查手牌能否合法和牌；役满手也返回实际宝牌统计。
pub fn calculate_bonus_han(
    hand: &Hand,
    riichi: RiichiStatus,
    dora_indicators: &[Tile],
    ura_indicators: &[Tile],
) -> Result<BonusHan, BonusError> {
    if hand.effective_tile_count() != Hand::MAX_TILE_COUNT {
        return Err(BonusError::InvalidHandSize {
            actual: hand.effective_tile_count(),
        });
    }
    let mut counts = [0u32; 34];
    let mut aka_dora = 0;
    for tile in hand
        .concealed()
        .iter()
        .chain(hand.melds().iter().flat_map(|meld| meld.tiles()))
    {
        counts[usize::from(tile.kind().as_u8())] += 1;
        aka_dora += u32::from(tile.is_aka());
    }
    Ok(BonusHan {
        dora: count_dora(&counts, dora_indicators),
        aka_dora,
        ura_dora: if riichi == RiichiStatus::None {
            0
        } else {
            count_dora(&counts, ura_indicators)
        },
    })
}

fn count_dora(counts: &[u32; 34], indicators: &[Tile]) -> u32 {
    indicators
        .iter()
        .map(|indicator| {
            let kind = indicator.kind().as_u8();
            let dora = match kind {
                0..=26 => kind / 9 * 9 + (kind % 9 + 1) % 9,
                27..=30 => 27 + (kind - 27 + 1) % 4,
                _ => 31 + (kind - 31 + 1) % 3,
            };
            counts[usize::from(dora)]
        })
        .sum()
}
