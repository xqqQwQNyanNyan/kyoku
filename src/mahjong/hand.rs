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

/// 修改手牌时无法保持手牌不变量。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandMutationError {
    /// 修改后的等效张数无效。
    InvalidSize(InvalidHandSize),
    /// 暗牌中不存在要移除的牌。
    TileNotFound { tile: Tile },
}

impl Hand {
    pub const MIN_TILE_COUNT: usize = 13;
    pub const MAX_TILE_COUNT: usize = 14;
    pub const TILES_PER_MELD: usize = 3;

    /// 构造手牌，并按领域顺序整理暗牌。
    ///
    /// 除等效张数外，不校验牌的副本数、面子内容或其他规则合法性。
    pub fn new(mut concealed: Vec<Tile>, melds: Vec<Meld>) -> Result<Self, InvalidHandSize> {
        validate_size(concealed.len(), melds.len())?;

        concealed.sort_unstable();
        Ok(Self { concealed, melds })
    }

    /// 将摸到的牌加入暗牌，并保持领域顺序。
    pub fn draw(&mut self, tile: Tile) -> Result<(), HandMutationError> {
        validate_size(self.concealed.len() + 1, self.melds.len())
            .map_err(HandMutationError::InvalidSize)?;
        self.concealed.push(tile);
        self.concealed.sort_unstable();
        Ok(())
    }

    /// 从暗牌中打出一张牌。
    pub fn discard(&mut self, tile: Tile) -> Result<(), HandMutationError> {
        validate_size(self.concealed.len() - 1, self.melds.len())
            .map_err(HandMutationError::InvalidSize)?;
        let position = self
            .concealed
            .iter()
            .position(|held| *held == tile)
            .ok_or(HandMutationError::TileNotFound { tile })?;
        self.concealed.remove(position);
        Ok(())
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

impl fmt::Display for HandMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSize(error) => error.fmt(formatter),
            Self::TileNotFound { tile } => {
                write!(
                    formatter,
                    "tile {} is not in the concealed hand",
                    tile.as_u8()
                )
            }
        }
    }
}

impl Error for HandMutationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidSize(error) => Some(error),
            Self::TileNotFound { .. } => None,
        }
    }
}

fn validate_size(concealed_count: usize, meld_count: usize) -> Result<(), InvalidHandSize> {
    let effective_tile_count = concealed_count + meld_count * Hand::TILES_PER_MELD;
    if (Hand::MIN_TILE_COUNT..=Hand::MAX_TILE_COUNT).contains(&effective_tile_count) {
        Ok(())
    } else {
        Err(InvalidHandSize {
            concealed_count,
            meld_count,
        })
    }
}
