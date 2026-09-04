use super::hand::{Hand, HandMutationError};
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

impl PlayerState {
    /// 从手牌、点数和按时间排列的牌河构造玩家状态。
    pub fn new(hand: Hand, score: i32, discards: Vec<Discard>) -> Self {
        Self {
            hand,
            score,
            discards,
        }
    }

    /// 将玩家摸到的牌加入手牌。
    pub fn draw(&mut self, tile: Tile) -> Result<(), HandMutationError> {
        self.hand.draw(tile)
    }

    /// 从手牌打出一张牌，并将记录追加到牌河。
    pub fn discard(&mut self, tile: Tile, tsumogiri: bool) -> Result<(), HandMutationError> {
        self.hand.discard(tile)?;
        self.discards
            .push(Discard::new(tile, tsumogiri, false, false));
        Ok(())
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
