use std::error::Error;
use std::fmt;

use super::hand::{Hand, HandMutationError};
use super::player_index::PlayerIndex;
use super::tile::Tile;

/// 一名玩家在当前局中的状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerState {
    hand: Hand,
    score: i32,
    discards: Vec<Discard>,
    riichi: RiichiState,
}

/// 玩家当前的立直状态。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RiichiState {
    /// 尚未宣告立直。
    #[default]
    NotDeclared,
    /// 已宣告立直，尚未正式成立。
    Declared,
    /// 立直已经正式成立并支付立直棒。
    Accepted,
}

/// 玩家立直状态无法完成转换的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerRiichiError {
    /// 当前状态不能再次宣告立直。
    InvalidDeclaration { state: RiichiState },
    /// 当前状态不能接受立直。
    InvalidAcceptance { state: RiichiState },
    /// 点数不足以宣告立直。
    InsufficientPoints { score: i32 },
    /// 支付立直棒会导致点数溢出。
    ScoreUnderflow { score: i32 },
}

/// 玩家打出的一张牌及其当前状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Discard {
    tile: Tile,
    tsumogiri: bool,
    riichi: bool,
    called: bool,
}

/// 将他家弃牌标记为已鸣牌时无法满足牌河不变量。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscardCallError {
    /// 牌河中没有可鸣的弃牌。
    NoDiscard,
    /// 事件声明的牌与牌河最后一张牌不一致。
    TileMismatch { discarded: Tile, called: Tile },
    /// 牌河最后一张牌已经被鸣走。
    AlreadyCalled { tile: Tile },
}

impl PlayerState {
    /// 从手牌、点数和按时间排列的牌河构造未立直的玩家状态。
    pub fn new(hand: Hand, score: i32, discards: Vec<Discard>) -> Self {
        Self {
            hand,
            score,
            discards,
            riichi: RiichiState::NotDeclared,
        }
    }

    /// 宣告立直，等待随后的宣言牌与立直通过事件。
    pub fn declare_riichi(&mut self) -> Result<(), PlayerRiichiError> {
        if self.riichi != RiichiState::NotDeclared {
            return Err(PlayerRiichiError::InvalidDeclaration { state: self.riichi });
        }
        if self.score < 1_000 {
            return Err(PlayerRiichiError::InsufficientPoints { score: self.score });
        }

        self.riichi = RiichiState::Declared;
        Ok(())
    }

    /// 正式接受此前的立直宣告并支付 1000 点。
    pub(super) fn accept_riichi(&mut self) -> Result<(), PlayerRiichiError> {
        if self.riichi != RiichiState::Declared {
            return Err(PlayerRiichiError::InvalidAcceptance { state: self.riichi });
        }
        let score = self
            .score
            .checked_sub(1_000)
            .ok_or(PlayerRiichiError::ScoreUnderflow { score: self.score })?;

        self.score = score;
        self.riichi = RiichiState::Accepted;
        Ok(())
    }

    /// 将玩家摸到的牌加入手牌。
    pub fn draw(&mut self, tile: Tile) -> Result<(), HandMutationError> {
        self.hand.draw(tile)
    }

    /// 从手牌打出一张牌，并将记录追加到牌河。
    pub fn discard(&mut self, tile: Tile, tsumogiri: bool) -> Result<(), HandMutationError> {
        self.discard_with_riichi(tile, tsumogiri, false)
    }

    /// 从手牌打出一张牌，并按局面上下文记录其是否为立直宣言牌。
    pub(super) fn discard_with_riichi(
        &mut self,
        tile: Tile,
        tsumogiri: bool,
        riichi: bool,
    ) -> Result<(), HandMutationError> {
        self.hand.discard(tile)?;
        self.discards
            .push(Discard::new(tile, tsumogiri, riichi, false));
        Ok(())
    }

    /// 使用两张暗牌完成吃牌。
    pub fn chi(
        &mut self,
        called: Tile,
        from: PlayerIndex,
        consumed: [Tile; 2],
    ) -> Result<(), HandMutationError> {
        self.hand.chi(called, from, consumed)
    }

    /// 使用两张暗牌完成碰牌。
    pub fn pon(
        &mut self,
        called: Tile,
        from: PlayerIndex,
        consumed: [Tile; 2],
    ) -> Result<(), HandMutationError> {
        self.hand.pon(called, from, consumed)
    }

    /// 使用三张暗牌完成大明杠。
    pub fn daiminkan(
        &mut self,
        called: Tile,
        from: PlayerIndex,
        consumed: [Tile; 3],
    ) -> Result<(), HandMutationError> {
        self.hand.daiminkan(called, from, consumed)
    }

    /// 使用四张暗牌完成暗杠。
    pub fn ankan(&mut self, consumed: [Tile; 4]) -> Result<(), HandMutationError> {
        self.hand.ankan(consumed)
    }

    /// 用一张暗牌升级已有碰子。
    pub fn kakan(&mut self, added: Tile, consumed: [Tile; 3]) -> Result<(), HandMutationError> {
        self.hand.kakan(added, consumed)
    }

    /// 将牌河最后一张牌标记为已被鸣走。
    pub fn mark_last_discard_called(&mut self, called: Tile) -> Result<(), DiscardCallError> {
        let discard = self
            .discards
            .last_mut()
            .ok_or(DiscardCallError::NoDiscard)?;
        if discard.tile != called {
            return Err(DiscardCallError::TileMismatch {
                discarded: discard.tile,
                called,
            });
        }
        if discard.called {
            return Err(DiscardCallError::AlreadyCalled { tile: called });
        }

        discard.called = true;
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

    /// 返回玩家当前的立直状态。
    pub const fn riichi(&self) -> RiichiState {
        self.riichi
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

impl fmt::Display for DiscardCallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDiscard => formatter.write_str("the target player has no discard to call"),
            Self::TileMismatch { discarded, called } => write!(
                formatter,
                "called tile {} does not match latest discard {}",
                called.as_u8(),
                discarded.as_u8()
            ),
            Self::AlreadyCalled { tile } => {
                write!(
                    formatter,
                    "discarded tile {} was already called",
                    tile.as_u8()
                )
            }
        }
    }
}

impl Error for DiscardCallError {}

impl fmt::Display for PlayerRiichiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDeclaration { state } => {
                write!(formatter, "cannot declare riichi from state {state:?}")
            }
            Self::InvalidAcceptance { state } => {
                write!(formatter, "cannot accept riichi from state {state:?}")
            }
            Self::InsufficientPoints { score } => {
                write!(formatter, "cannot declare riichi with only {score} points")
            }
            Self::ScoreUnderflow { score } => {
                write!(formatter, "cannot deduct 1000 points from score {score}")
            }
        }
    }
}

impl Error for PlayerRiichiError {}
