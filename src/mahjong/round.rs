use std::error::Error;
use std::fmt;

use super::hand::HandMutationError;
use super::player::{DiscardCallError, PlayerState};
use super::player_index::PlayerIndex;
use super::tile::Tile;

/// 一局麻将的场风。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Wind {
    /// 东风。
    East,
    /// 南风。
    South,
    /// 西风。
    West,
    /// 北风。
    North,
}

/// 一局在整场牌局中的标识。
///
/// `number` 的有效范围为 `1..=4`。当前四人麻将模型约定玩家 `0` 是东一局
/// 庄家，因此每局庄家可以由局数直接确定。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RoundId {
    wind: Wind,
    number: u8,
}

/// 杠的种类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KanKind {
    /// 使用他家弃牌完成的大明杠。
    Daiminkan,
    /// 使用四张自有牌完成的暗杠。
    Ankan,
    /// 在已有碰子上追加第四张牌的加杠。
    Kakan,
}

/// 当前一局所处的阶段。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RoundPhase {
    /// 本局刚刚开始，庄家尚未完成第一次摸牌。
    Initial,
    /// 某玩家摸牌后，等待其进行自摸、打牌或杠等操作。
    AfterDraw {
        /// 摸牌的玩家。
        player: PlayerIndex,
    },
    /// 某玩家打牌后，等待其他玩家进行荣和、吃、碰或杠等响应。
    AfterDiscard {
        /// 打出牌的玩家。
        player: PlayerIndex,
    },
    /// 某玩家吃或碰后，等待其打出一张牌。
    AfterCall {
        /// 完成吃或碰的玩家。
        player: PlayerIndex,
    },
    /// 某玩家宣告杠后，等待抢杠响应或后续杠处理。
    AfterKanDeclaration {
        /// 宣告杠的玩家。
        player: PlayerIndex,
        /// 宣告的杠种类。
        kind: KanKind,
    },
    /// 本局已经结束。
    Ended,
}

/// 一局麻将的当前状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoundState {
    players: [PlayerState; 4],
    round: RoundId,
    honba: u8,
    riichi_sticks: u8,
    dora_indicators: Vec<Tile>,
    remaining_draws: u8,
    phase: RoundPhase,
}

/// 摸牌无法应用到当前局面的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawError {
    /// 牌山中已经没有可摸牌。
    NoRemainingDraws,
    /// 玩家手牌无法接受这张牌。
    Hand(HandMutationError),
}

/// 吃碰事件无法应用到当前局面的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallError {
    /// 当前阶段不接受吃碰事件。
    InvalidPhase { phase: RoundPhase },
    /// 鸣牌者与弃牌者相同。
    ActorIsTarget { player: PlayerIndex },
    /// 事件的来源玩家不是当前最后一次弃牌的玩家。
    WrongTarget {
        expected: PlayerIndex,
        actual: PlayerIndex,
    },
    /// 吃牌者不是弃牌者的下家。
    InvalidChiActor {
        expected: PlayerIndex,
        actual: PlayerIndex,
    },
    /// 鸣牌者的手牌无法完成该操作。
    Hand {
        player: PlayerIndex,
        error: HandMutationError,
    },
    /// 来源玩家的牌河与事件不一致。
    Discard {
        player: PlayerIndex,
        error: DiscardCallError,
    },
}

impl RoundId {
    /// 当前四人麻将模型允许的最小局数。
    pub const MIN_NUMBER: u8 = 1;
    /// 当前四人麻将模型允许的最大局数。
    pub const MAX_NUMBER: u8 = 4;

    /// 从场风和局数构造一局的标识；局数无效时返回 `None`。
    pub const fn new(wind: Wind, number: u8) -> Option<Self> {
        if number >= Self::MIN_NUMBER && number <= Self::MAX_NUMBER {
            Some(Self { wind, number })
        } else {
            None
        }
    }

    /// 返回场风。
    pub const fn wind(self) -> Wind {
        self.wind
    }

    /// 返回当前场风内从一开始的局数。
    pub const fn number(self) -> u8 {
        self.number
    }

    /// 返回当前局的庄家。
    pub const fn dealer(self) -> PlayerIndex {
        match PlayerIndex::new(self.number - 1) {
            Some(player) => player,
            None => unreachable!(),
        }
    }
}

impl RoundPhase {
    /// 返回阶段中记录的玩家；开局及本局结束时返回 `None`。
    pub const fn player(self) -> Option<PlayerIndex> {
        match self {
            Self::AfterDraw { player }
            | Self::AfterDiscard { player }
            | Self::AfterCall { player }
            | Self::AfterKanDeclaration { player, .. } => Some(player),
            Self::Initial | Self::Ended => None,
        }
    }
}

impl RoundState {
    /// 四人日麻开局时的可摸牌数量。
    pub const INITIAL_REMAINING_DRAWS: u8 = 70;

    /// 创建一局的初始状态。
    pub fn start(
        players: [PlayerState; 4],
        round: RoundId,
        honba: u8,
        riichi_sticks: u8,
        dora_indicator: Tile,
    ) -> Self {
        Self::new(
            players,
            round,
            honba,
            riichi_sticks,
            vec![dora_indicator],
            Self::INITIAL_REMAINING_DRAWS,
            RoundPhase::Initial,
        )
    }

    /// 从四名玩家及完整局面信息构造状态。
    ///
    /// 该构造器不校验这些字段能否由同一段合法事件序列产生。
    pub fn new(
        players: [PlayerState; 4],
        round: RoundId,
        honba: u8,
        riichi_sticks: u8,
        dora_indicators: Vec<Tile>,
        remaining_draws: u8,
        phase: RoundPhase,
    ) -> Self {
        Self {
            players,
            round,
            honba,
            riichi_sticks,
            dora_indicators,
            remaining_draws,
            phase,
        }
    }

    /// 将一名玩家的摸牌应用到当前局面。
    pub fn draw(&mut self, player: PlayerIndex, tile: Tile) -> Result<(), DrawError> {
        if self.remaining_draws == 0 {
            return Err(DrawError::NoRemainingDraws);
        }

        self.player_mut(player)
            .draw(tile)
            .map_err(DrawError::Hand)?;
        self.remaining_draws -= 1;
        self.phase = RoundPhase::AfterDraw { player };
        Ok(())
    }

    /// 将一名玩家的打牌应用到当前局面。
    pub fn discard(
        &mut self,
        player: PlayerIndex,
        tile: Tile,
        tsumogiri: bool,
    ) -> Result<(), HandMutationError> {
        self.player_mut(player).discard(tile, tsumogiri)?;
        self.phase = RoundPhase::AfterDiscard { player };
        Ok(())
    }

    /// 应用一次吃牌，并原子地更新手牌、牌河和局面阶段。
    pub fn chi(
        &mut self,
        actor: PlayerIndex,
        target: PlayerIndex,
        called: Tile,
        consumed: [Tile; 2],
    ) -> Result<(), CallError> {
        self.validate_call_context(actor, target)?;
        let expected = next_player(target);
        if actor != expected {
            return Err(CallError::InvalidChiActor {
                expected,
                actual: actor,
            });
        }

        let mut players = self.players.clone();
        players[usize::from(target.get_id())]
            .mark_last_discard_called(called)
            .map_err(|error| CallError::Discard {
                player: target,
                error,
            })?;
        players[usize::from(actor.get_id())]
            .chi(called, target, consumed)
            .map_err(|error| CallError::Hand {
                player: actor,
                error,
            })?;

        self.players = players;
        self.phase = RoundPhase::AfterCall { player: actor };
        Ok(())
    }

    /// 应用一次碰牌，并原子地更新手牌、牌河和局面阶段。
    pub fn pon(
        &mut self,
        actor: PlayerIndex,
        target: PlayerIndex,
        called: Tile,
        consumed: [Tile; 2],
    ) -> Result<(), CallError> {
        self.validate_call_context(actor, target)?;

        let mut players = self.players.clone();
        players[usize::from(target.get_id())]
            .mark_last_discard_called(called)
            .map_err(|error| CallError::Discard {
                player: target,
                error,
            })?;
        players[usize::from(actor.get_id())]
            .pon(called, target, consumed)
            .map_err(|error| CallError::Hand {
                player: actor,
                error,
            })?;

        self.players = players;
        self.phase = RoundPhase::AfterCall { player: actor };
        Ok(())
    }

    /// 返回按玩家索引排列的四名玩家状态。
    pub const fn players(&self) -> &[PlayerState; 4] {
        &self.players
    }

    /// 返回指定玩家的状态。
    pub fn player(&self, player: PlayerIndex) -> &PlayerState {
        &self.players[usize::from(player.get_id())]
    }

    fn player_mut(&mut self, player: PlayerIndex) -> &mut PlayerState {
        &mut self.players[usize::from(player.get_id())]
    }

    fn validate_call_context(
        &self,
        actor: PlayerIndex,
        target: PlayerIndex,
    ) -> Result<(), CallError> {
        let RoundPhase::AfterDiscard { player: expected } = self.phase else {
            return Err(CallError::InvalidPhase { phase: self.phase });
        };
        if actor == target {
            return Err(CallError::ActorIsTarget { player: actor });
        }
        if target != expected {
            return Err(CallError::WrongTarget {
                expected,
                actual: target,
            });
        }
        Ok(())
    }

    /// 返回当前局的标识。
    pub const fn round(&self) -> RoundId {
        self.round
    }

    /// 返回本场数。
    pub const fn honba(&self) -> u8 {
        self.honba
    }

    /// 返回场上的立直棒数量。
    pub const fn riichi_sticks(&self) -> u8 {
        self.riichi_sticks
    }

    /// 返回按翻开顺序排列的宝牌指示牌。
    pub fn dora_indicators(&self) -> &[Tile] {
        &self.dora_indicators
    }

    /// 返回从当前时点起剩余的可摸牌数量。
    pub const fn remaining_draws(&self) -> u8 {
        self.remaining_draws
    }

    /// 返回当前阶段。
    pub const fn phase(&self) -> RoundPhase {
        self.phase
    }
}

impl fmt::Display for DrawError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoRemainingDraws => formatter.write_str("no draws remain"),
            Self::Hand(error) => error.fmt(formatter),
        }
    }
}

impl Error for DrawError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::NoRemainingDraws => None,
            Self::Hand(error) => Some(error),
        }
    }
}

impl fmt::Display for CallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPhase { phase } => write!(formatter, "cannot call in phase {phase:?}"),
            Self::ActorIsTarget { player } => {
                write!(
                    formatter,
                    "player {} cannot call their own discard",
                    player.get_id()
                )
            }
            Self::WrongTarget { expected, actual } => write!(
                formatter,
                "call targets player {}, but the latest discard belongs to player {}",
                actual.get_id(),
                expected.get_id()
            ),
            Self::InvalidChiActor { expected, actual } => write!(
                formatter,
                "player {} cannot chi; expected player {}",
                actual.get_id(),
                expected.get_id()
            ),
            Self::Hand { error, .. } => error.fmt(formatter),
            Self::Discard { error, .. } => error.fmt(formatter),
        }
    }
}

impl Error for CallError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Hand { error, .. } => Some(error),
            Self::Discard { error, .. } => Some(error),
            _ => None,
        }
    }
}

fn next_player(player: PlayerIndex) -> PlayerIndex {
    PlayerIndex::new((player.get_id() + 1) % 4).expect("next player is always valid")
}
