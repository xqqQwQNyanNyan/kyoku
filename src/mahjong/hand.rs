use std::error::Error;
use std::fmt;

use super::meld::Meld;
use super::tile::Tile;

/// 一名玩家当前持有的手牌。
///
/// 暗牌始终按 [`Tile`] 的顺序排列。手牌的等效张数必须为 13 或 14；每个
/// 副露按三张计算，因此杠的第四张牌不增加等效张数。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hand {
    concealed: Vec<Tile>,
    melds: Vec<Meld>,
}

/// 手牌的等效张数不是 13 或 14。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidHandSize {
    concealed_count: usize,
    meld_count: usize,
}

impl Hand {
    pub const MIN_TILE_COUNT: usize = 13;
    pub const MAX_TILE_COUNT: usize = 14;
    pub const TILES_PER_MELD: usize = 3;

    /// 构造手牌，并按领域顺序整理暗牌。
    ///
    /// 除等效张数外，不校验牌的副本数、面子内容或其他规则合法性。
    pub fn new(mut concealed: Vec<Tile>, melds: Vec<Meld>) -> Result<Self, InvalidHandSize> {
        let effective_tile_count = concealed.len() + melds.len() * Self::TILES_PER_MELD;
        if !(Self::MIN_TILE_COUNT..=Self::MAX_TILE_COUNT).contains(&effective_tile_count) {
            return Err(InvalidHandSize {
                concealed_count: concealed.len(),
                meld_count: melds.len(),
            });
        }

        concealed.sort_unstable();
        Ok(Self { concealed, melds })
    }

    /// 返回排好序的暗牌。
    pub fn concealed(&self) -> &[Tile] {
        &self.concealed
    }

    /// 返回已有面子，并保留构造时的顺序。
    pub fn melds(&self) -> &[Meld] {
        &self.melds
    }

    /// 返回手牌的等效张数；每个面子按三张计算。
    pub fn effective_tile_count(&self) -> usize {
        self.concealed.len() + self.melds.len() * Self::TILES_PER_MELD
    }
}

impl InvalidHandSize {
    /// 返回暗牌张数。
    pub const fn concealed_count(self) -> usize {
        self.concealed_count
    }

    /// 返回面子数量。
    pub const fn meld_count(self) -> usize {
        self.meld_count
    }

    /// 返回手牌的等效张数；每个面子按三张计算。
    pub const fn effective_tile_count(self) -> usize {
        self.concealed_count + self.meld_count * Hand::TILES_PER_MELD
    }
}

impl fmt::Display for InvalidHandSize {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid hand size {}; expected 13 or 14 effective tiles ({} concealed, {} melds)",
            self.effective_tile_count(),
            self.concealed_count,
            self.meld_count
        )
    }
}

impl Error for InvalidHandSize {}
