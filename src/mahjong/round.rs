use std::error::Error;
use std::fmt;

use super::hand::HandMutationError;
use super::player::{
    DiscardCallError, PlayerRiichiError, PlayerState, RiichiState, ScoreMutationError,
};
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

/// 玩家当前手中最后一张摸牌的来源。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DrawSource {
    /// 从通常牌山摸牌。
    Wall,
    /// 杠后从岭上摸牌。
    Rinshan,
}

/// 一局结束的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RoundResult {
    /// 本局因和牌结束。
    Hora,
    /// 本局因流局结束。
    Ryukyoku,
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
        /// 这张摸牌的来源。
        source: DrawSource,
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
    /// 某玩家完成杠后，等待岭上摸牌或抢杠等后续事件。
    AfterKanDeclaration {
        /// 完成杠的玩家。
        player: PlayerIndex,
        /// 完成的杠种类。
        kind: KanKind,
    },
    /// 和牌或流局结算已经完成，等待 [`RoundState::end_kyoku`] 确认本局结束。
    AwaitingEnd(RoundResult),
    /// 本局已经结束，并记录终局原因。
    Ended(RoundResult),
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
    /// 当前阶段不接受摸牌。
    InvalidPhase { phase: RoundPhase },
    /// 牌山中已经没有可摸牌。
    NoRemainingDraws,
    /// 玩家手牌无法接受这张牌。
    Hand(HandMutationError),
}

/// 打牌无法应用到当前局面的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscardError {
    /// 当前阶段不接受打牌。
    InvalidPhase { phase: RoundPhase },
    /// 玩家的手牌无法完成打牌。
    Hand(HandMutationError),
}

/// 宝牌指示牌事件无法应用到当前局面的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoraError {
    /// 当前阶段不接受新的宝牌指示牌。
    InvalidPhase { phase: RoundPhase },
}

/// 结束一局无法完成的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndKyokuError {
    /// 当前阶段尚未完成和牌或流局结算。
    InvalidPhase { phase: RoundPhase },
}

/// 和牌终局结算无法应用到当前局面的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoraError {
    /// 当前阶段不接受和牌终局结算。
    InvalidPhase { phase: RoundPhase },
    /// 一名玩家的点数无法应用结算点差。
    Score {
        player: PlayerIndex,
        error: ScoreMutationError,
    },
}

/// 流局结算无法应用到当前局面的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RyuukyokuError {
    /// 当前阶段不接受流局结算。
    InvalidPhase { phase: RoundPhase },
    /// 一名玩家的点数无法应用结算点差。
    Score {
        player: PlayerIndex,
        error: ScoreMutationError,
    },
}

/// 立直事件无法应用到当前局面的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiichiError {
    /// 当前阶段不接受该立直事件。
    InvalidPhase { phase: RoundPhase },
    /// 立直事件的玩家与当前阶段中的玩家不一致。
    WrongActor {
        expected: PlayerIndex,
        actual: PlayerIndex,
    },
    /// 玩家尚未打出立直宣言牌。
    DeclarationDiscardMissing { player: PlayerIndex },
    /// 玩家状态不能完成请求的立直转换。
    Player {
        player: PlayerIndex,
        error: PlayerRiichiError,
    },
    /// 场上的立直棒数量已经无法再增加。
    TooManySticks,
}

/// 吃、碰或杠事件无法应用到当前局面的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallError {
    /// 当前阶段不接受该鸣牌事件。
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
    /// 暗杠或加杠者不是刚刚摸牌的玩家。
    WrongActor {
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
            Self::AfterDraw { player, .. } => Some(player),
            Self::AfterKanDeclaration { player, .. } => Some(player),
            Self::AfterDiscard { player } | Self::AfterCall { player } => Some(player),
            Self::Initial | Self::AwaitingEnd(_) | Self::Ended(_) => None,
        }
    }

    const fn is_terminal(self) -> bool {
        matches!(self, Self::AwaitingEnd(_) | Self::Ended(_))
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
    pub fn draw(&mut self, actor: PlayerIndex, tile: Tile) -> Result<(), DrawError> {
        if self.phase.is_terminal() {
            return Err(DrawError::InvalidPhase { phase: self.phase });
        }
        if self.remaining_draws == 0 {
            return Err(DrawError::NoRemainingDraws);
        }

        let source = match self.phase {
            RoundPhase::AfterKanDeclaration {
                player: kan_actor, ..
            } if kan_actor == actor => DrawSource::Rinshan,
            _ => DrawSource::Wall,
        };

        self.player_mut(actor).draw(tile).map_err(DrawError::Hand)?;
        self.remaining_draws -= 1;
        self.phase = RoundPhase::AfterDraw {
            player: actor,
            source,
        };
        Ok(())
    }

    /// 追加一张新翻开的宝牌指示牌，不改变当前阶段。
    pub fn reveal_dora(&mut self, marker: Tile) -> Result<(), DoraError> {
        if self.phase.is_terminal() {
            return Err(DoraError::InvalidPhase { phase: self.phase });
        }
        self.dora_indicators.push(marker);
        Ok(())
    }

    /// 记录一名玩家的立直宣告，不改变点数、立直棒或当前阶段。
    pub fn declare_riichi(&mut self, player: PlayerIndex) -> Result<(), RiichiError> {
        let RoundPhase::AfterDraw {
            player: expected, ..
        } = self.phase
        else {
            return Err(RiichiError::InvalidPhase { phase: self.phase });
        };
        if player != expected {
            return Err(RiichiError::WrongActor {
                expected,
                actual: player,
            });
        }
        self.player_mut(player)
            .declare_riichi()
            .map_err(|error| RiichiError::Player { player, error })
    }

    /// 接受一名玩家此前的立直宣告，原子地更新点数和立直棒。
    pub fn accept_riichi(&mut self, player: PlayerIndex) -> Result<(), RiichiError> {
        let RoundPhase::AfterDiscard { player: expected } = self.phase else {
            return Err(RiichiError::InvalidPhase { phase: self.phase });
        };
        if player != expected {
            return Err(RiichiError::WrongActor {
                expected,
                actual: player,
            });
        }
        if !self
            .player(player)
            .discards()
            .last()
            .is_some_and(|discard| discard.is_riichi())
        {
            return Err(RiichiError::DeclarationDiscardMissing { player });
        }
        let riichi_sticks = self
            .riichi_sticks
            .checked_add(1)
            .ok_or(RiichiError::TooManySticks)?;
        self.player_mut(player)
            .accept_riichi()
            .map_err(|error| RiichiError::Player { player, error })?;
        self.riichi_sticks = riichi_sticks;
        Ok(())
    }

    /// 将一名玩家的打牌应用到当前局面。
    pub fn discard(
        &mut self,
        player: PlayerIndex,
        tile: Tile,
        tsumogiri: bool,
    ) -> Result<(), DiscardError> {
        if self.phase.is_terminal() {
            return Err(DiscardError::InvalidPhase { phase: self.phase });
        }
        let player_state = self.player(player);
        let is_riichi = matches!(
            self.phase,
            RoundPhase::AfterDraw {
                player: phase_player,
                ..
            } if phase_player == player
        ) && player_state.riichi() == RiichiState::Declared
            && !player_state
                .discards()
                .iter()
                .any(|discard| discard.is_riichi());
        self.player_mut(player)
            .discard_with_riichi(tile, tsumogiri, is_riichi)
            .map_err(DiscardError::Hand)?;
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

    /// 应用一次大明杠，并原子地更新手牌、牌河和局面阶段。
    pub fn daiminkan(
        &mut self,
        actor: PlayerIndex,
        target: PlayerIndex,
        called: Tile,
        consumed: [Tile; 3],
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
            .daiminkan(called, target, consumed)
            .map_err(|error| CallError::Hand {
                player: actor,
                error,
            })?;

        self.players = players;
        self.phase = RoundPhase::AfterKanDeclaration {
            player: actor,
            kind: KanKind::Daiminkan,
        };
        Ok(())
    }

    /// 应用一次暗杠，并进入杠声明后的阶段。
    pub fn ankan(&mut self, actor: PlayerIndex, consumed: [Tile; 4]) -> Result<(), CallError> {
        self.validate_self_kan_context(actor)?;
        self.player_mut(actor)
            .ankan(consumed)
            .map_err(|error| CallError::Hand {
                player: actor,
                error,
            })?;
        self.phase = RoundPhase::AfterKanDeclaration {
            player: actor,
            kind: KanKind::Ankan,
        };
        Ok(())
    }

    /// 应用一次加杠，将已有碰子原地升级并进入杠声明后的阶段。
    pub fn kakan(
        &mut self,
        actor: PlayerIndex,
        added: Tile,
        consumed: [Tile; 3],
    ) -> Result<(), CallError> {
        self.validate_self_kan_context(actor)?;
        self.player_mut(actor)
            .kakan(added, consumed)
            .map_err(|error| CallError::Hand {
                player: actor,
                error,
            })?;
        self.phase = RoundPhase::AfterKanDeclaration {
            player: actor,
            kind: KanKind::Kakan,
        };
        Ok(())
    }

    /// 应用一次完整的和牌结算，并等待 `end_kyoku` 确认本局结束。
    pub fn hora(&mut self, score_deltas: [i32; 4]) -> Result<(), HoraError> {
        if !matches!(
            self.phase,
            RoundPhase::AfterDraw { .. }
                | RoundPhase::AfterDiscard { .. }
                | RoundPhase::AfterKanDeclaration {
                    kind: KanKind::Kakan | KanKind::Ankan,
                    ..
                }
        ) {
            return Err(HoraError::InvalidPhase { phase: self.phase });
        }

        let mut players = self.players.clone();
        for (index, (player_state, delta)) in players.iter_mut().zip(score_deltas).enumerate() {
            let player = PlayerIndex::new(index as u8).expect("player array index is always valid");
            player_state
                .apply_score_delta(delta)
                .map_err(|error| HoraError::Score { player, error })?;
        }

        self.players = players;
        self.riichi_sticks = 0;
        self.phase = RoundPhase::AwaitingEnd(RoundResult::Hora);
        Ok(())
    }

    /// 应用一次流局结算，并等待 `end_kyoku` 确认本局结束。
    pub fn ryuukyoku(&mut self, score_deltas: [i32; 4]) -> Result<(), RyuukyokuError> {
        if self.phase.is_terminal() {
            return Err(RyuukyokuError::InvalidPhase { phase: self.phase });
        }

        let mut players = self.players.clone();
        for (index, (player_state, delta)) in players.iter_mut().zip(score_deltas).enumerate() {
            let player = PlayerIndex::new(index as u8).expect("player array index is always valid");
            player_state
                .apply_score_delta(delta)
                .map_err(|error| RyuukyokuError::Score { player, error })?;
        }

        self.players = players;
        self.phase = RoundPhase::AwaitingEnd(RoundResult::Ryukyoku);
        Ok(())
    }

    /// 确认已经完成结算的一局结束。
    ///
    /// 只有 [`RoundPhase::AwaitingEnd`] 可以完成这一转换。成功后阶段变为
    /// [`RoundPhase::Ended`]，其中保留此前记录的 [`RoundResult`]。该操作不负责
    /// 结算，也不会修改点数、手牌、牌河、宝牌指示牌或剩余摸牌数。
    pub fn end_kyoku(&mut self) -> Result<(), EndKyokuError> {
        let RoundPhase::AwaitingEnd(result) = self.phase else {
            return Err(EndKyokuError::InvalidPhase { phase: self.phase });
        };
        self.phase = RoundPhase::Ended(result);
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

    fn validate_self_kan_context(&self, actor: PlayerIndex) -> Result<(), CallError> {
        let RoundPhase::AfterDraw {
            player: expected, ..
        } = self.phase
        else {
            return Err(CallError::InvalidPhase { phase: self.phase });
        };
        if actor != expected {
            return Err(CallError::WrongActor {
                expected,
                actual: actor,
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
            Self::InvalidPhase { phase } => {
                write!(formatter, "cannot draw in phase {phase:?}")
            }
            Self::NoRemainingDraws => formatter.write_str("no draws remain"),
            Self::Hand(error) => error.fmt(formatter),
        }
    }
}

impl Error for DrawError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidPhase { .. } | Self::NoRemainingDraws => None,
            Self::Hand(error) => Some(error),
        }
    }
}

impl fmt::Display for DiscardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPhase { phase } => {
                write!(formatter, "cannot discard in phase {phase:?}")
            }
            Self::Hand(error) => error.fmt(formatter),
        }
    }
}

impl Error for DiscardError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidPhase { .. } => None,
            Self::Hand(error) => Some(error),
        }
    }
}

impl fmt::Display for DoraError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPhase { phase } => {
                write!(formatter, "cannot reveal dora in phase {phase:?}")
            }
        }
    }
}

impl Error for DoraError {}

impl fmt::Display for EndKyokuError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPhase { phase } => {
                write!(formatter, "cannot end round in phase {phase:?}")
            }
        }
    }
}

impl Error for EndKyokuError {}

impl fmt::Display for HoraError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPhase { phase } => {
                write!(formatter, "cannot apply hora in phase {phase:?}")
            }
            Self::Score { error, .. } => error.fmt(formatter),
        }
    }
}

impl Error for HoraError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Score { error, .. } => Some(error),
            Self::InvalidPhase { .. } => None,
        }
    }
}

impl fmt::Display for RyuukyokuError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPhase { phase } => {
                write!(formatter, "cannot apply ryuukyoku in phase {phase:?}")
            }
            Self::Score { error, .. } => error.fmt(formatter),
        }
    }
}

impl Error for RyuukyokuError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Score { error, .. } => Some(error),
            Self::InvalidPhase { .. } => None,
        }
    }
}

impl fmt::Display for RiichiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPhase { phase } => {
                write!(formatter, "cannot apply riichi event in phase {phase:?}")
            }
            Self::WrongActor { expected, actual } => write!(
                formatter,
                "riichi event belongs to player {}, but expected player {}",
                actual.get_id(),
                expected.get_id()
            ),
            Self::DeclarationDiscardMissing { player } => write!(
                formatter,
                "player {} has not made a riichi declaration discard",
                player.get_id()
            ),
            Self::Player { error, .. } => error.fmt(formatter),
            Self::TooManySticks => formatter.write_str("riichi stick count cannot be incremented"),
        }
    }
}

impl Error for RiichiError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Player { error, .. } => Some(error),
            Self::InvalidPhase { .. }
            | Self::WrongActor { .. }
            | Self::DeclarationDiscardMissing { .. }
            | Self::TooManySticks => None,
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
            Self::WrongActor { expected, actual } => write!(
                formatter,
                "player {} cannot declare this kan; expected player {}",
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
