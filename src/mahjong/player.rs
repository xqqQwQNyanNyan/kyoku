use std::error::Error;
use std::fmt;

use super::hand::Hand;
use super::tile::Tile;

/// 一名玩家在当前局中的状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerState {
    hand: Hand,
    score: i32,
    discards: Vec<Discard>,
}

/// 玩家打出的一张牌及其当前状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Discard {
    tile: Tile,
    tsumogiri: bool,
    riichi: bool,
    called: bool,
}

/// 玩家在一局中的索引。
///
/// 有效值为 `0..=3`。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlayerIndex(u8);

/// `PlayerIndex` 编码超出 `0..=3`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidPlayerIndex(u8);

impl PlayerState {
    /// 从手牌、点数和按时间排列的牌河构造玩家状态。
    pub fn new(hand: Hand, score: i32, discards: Vec<Discard>) -> Self {
        Self {
            hand,
            score,
            discards,
        }
    }

    /// 返回玩家当前的手牌。
    pub const fn hand(&self) -> &Hand {
        &self.hand
    }

    /// 返回玩家当前的点数。
    pub const fn score(&self) -> i32 {
        self.score
    }

    /// 返回按打出时间排列的牌河。
    pub fn discards(&self) -> &[Discard] {
        &self.discards
    }
}

impl Discard {
    /// 构造一张牌河记录。
    pub const fn new(tile: Tile, tsumogiri: bool, riichi: bool, called: bool) -> Self {
        Self {
            tile,
            tsumogiri,
            riichi,
            called,
        }
    }

    /// 返回打出的牌。
    pub const fn tile(self) -> Tile {
        self.tile
    }

    /// 这张牌是否为摸切。
    pub const fn is_tsumogiri(self) -> bool {
        self.tsumogiri
    }

    /// 这张牌是否为立直宣言牌。
    pub const fn is_riichi(self) -> bool {
        self.riichi
    }

    /// 这张牌是否已被其他玩家鸣走。
    pub const fn is_called(self) -> bool {
        self.called
    }
}

impl PlayerIndex {
    const MAX_VALUE: u8 = 3;

    /// 从整数构造玩家索引；值无效时返回 `None`。
    pub const fn new(value: u8) -> Option<Self> {
        if value <= Self::MAX_VALUE {
            Some(Self(value))
        } else {
            None
        }
    }

    /// 返回玩家索引的整数值。
    pub const fn get_id(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for PlayerIndex {
    type Error = InvalidPlayerIndex;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(InvalidPlayerIndex(value))
    }
}

impl InvalidPlayerIndex {
    pub const fn value(self) -> u8 {
        self.0
    }
}

impl fmt::Display for InvalidPlayerIndex {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid player index {}; expected 0..=3", self.0)
    }
}

impl Error for InvalidPlayerIndex {}
