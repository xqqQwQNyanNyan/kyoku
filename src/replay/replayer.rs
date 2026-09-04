use std::error::Error;
use std::fmt;

use convlog::Event;

use crate::mahjong::hand::{Hand, HandMutationError};
use crate::mahjong::player::PlayerState;
use crate::mahjong::player_index::PlayerIndex;
use crate::mahjong::round::{CallError, DrawError, RiichiError, RoundId, RoundState, Wind};
use crate::mahjong::tile::Tile;

/// 按顺序消费 mjai 事件并重建当前局面状态。
#[derive(Debug, Default)]
pub struct Replayer {
    state: Option<RoundState>,
}

/// 重放事件失败的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayError {
    /// 当前版本尚未支持这种事件。
    UnsupportedEvent,
    /// 尚未收到 `start_kyoku`。
    NoRound,
    /// 事件中的玩家编号无效。
    InvalidPlayer { value: u8 },
    /// 事件中的牌编码无效或为未知牌。
    InvalidTile { value: u8 },
    /// `start_kyoku` 中的场风不是风牌。
    InvalidRoundWind { value: u8 },
    /// `start_kyoku` 中的局数无效。
    InvalidRoundNumber { value: u8 },
    /// 状态转移产生了无效的手牌张数。
    InvalidHandSize {
        player: PlayerIndex,
        effective_tile_count: usize,
    },
    /// 玩家没有要打出的牌。
    TileNotInHand { player: PlayerIndex, tile: Tile },
    /// 牌山中已经没有可摸牌。
    NoRemainingDraws,
    /// 吃、碰或杠事件违反当前局面的领域规则。
    Call(CallError),
    /// 立直事件违反当前局面的领域规则。
    Riichi(RiichiError),
}

impl Replayer {
    /// 创建一个尚未开始任何一局的状态机。
    pub const fn new() -> Self {
        Self { state: None }
    }

    /// 将一个 mjai 事件应用到当前状态。
    pub fn apply(&mut self, event: &Event) -> Result<(), ReplayError> {
        match event {
            Event::StartKyoku {
                bakaze,
                dora_marker,
                kyoku,
                honba,
                kyotaku,
                scores,
                tehais,
                ..
            } => self.start_kyoku(
                *bakaze,
                *dora_marker,
                *kyoku,
                *honba,
                *kyotaku,
                *scores,
                tehais,
            ),
            Event::Tsumo { actor, pai } => self.draw(*actor, *pai),
            Event::Dahai {
                actor,
                pai,
                tsumogiri,
            } => self.discard(*actor, *pai, *tsumogiri),
            Event::Chi {
                actor,
                target,
                pai,
                consumed,
            } => self.chi(*actor, *target, *pai, *consumed),
            Event::Pon {
                actor,
                target,
                pai,
                consumed,
            } => self.pon(*actor, *target, *pai, *consumed),
            Event::Daiminkan {
                actor,
                target,
                pai,
                consumed,
            } => self.daiminkan(*actor, *target, *pai, *consumed),
            Event::Ankan { actor, consumed } => self.ankan(*actor, *consumed),
            Event::Kakan {
                actor,
                pai,
                consumed,
            } => self.kakan(*actor, *pai, *consumed),
            Event::Dora { dora_marker } => self.reveal_dora(*dora_marker),
            Event::Reach { actor } => self.declare_riichi(*actor),
            Event::ReachAccepted { actor } => self.accept_riichi(*actor),
            _ => Err(ReplayError::UnsupportedEvent),
        }
    }

    /// 返回最近一次成功应用事件后的局面；尚未开局时返回 `None`。
    pub const fn state(&self) -> Option<&RoundState> {
        self.state.as_ref()
    }

    #[allow(clippy::too_many_arguments)]
    fn start_kyoku(
        &mut self,
        bakaze: convlog::Tile,
        dora_marker: convlog::Tile,
        kyoku: u8,
        honba: u8,
        kyotaku: u8,
        scores: [i32; 4],
        tehais: &[[convlog::Tile; 13]; 4],
    ) -> Result<(), ReplayError> {
        let wind = convert_wind(bakaze)?;
        let round =
            RoundId::new(wind, kyoku).ok_or(ReplayError::InvalidRoundNumber { value: kyoku })?;
        let players = [
            player_from_initial_hand(0, scores[0], &tehais[0])?,
            player_from_initial_hand(1, scores[1], &tehais[1])?,
            player_from_initial_hand(2, scores[2], &tehais[2])?,
            player_from_initial_hand(3, scores[3], &tehais[3])?,
        ];

        self.state = Some(RoundState::start(
            players,
            round,
            honba,
            kyotaku,
            convert_tile(dora_marker)?,
        ));
        Ok(())
    }

    fn draw(&mut self, actor: u8, pai: convlog::Tile) -> Result<(), ReplayError> {
        let actor = convert_player(actor)?;
        let tile = convert_tile(pai)?;
        self.state
            .as_mut()
            .ok_or(ReplayError::NoRound)?
            .draw(actor, tile)
            .map_err(|error| translate_draw_error(actor, error))
    }

    fn discard(
        &mut self,
        actor: u8,
        pai: convlog::Tile,
        tsumogiri: bool,
    ) -> Result<(), ReplayError> {
        let actor = convert_player(actor)?;
        let tile = convert_tile(pai)?;
        self.state
            .as_mut()
            .ok_or(ReplayError::NoRound)?
            .discard(actor, tile, tsumogiri)
            .map_err(|error| translate_hand_error(actor, error))
    }

    fn chi(
        &mut self,
        actor: u8,
        target: u8,
        pai: convlog::Tile,
        consumed: [convlog::Tile; 2],
    ) -> Result<(), ReplayError> {
        let actor = convert_player(actor)?;
        let target = convert_player(target)?;
        let called = convert_tile(pai)?;
        let consumed = convert_consumed(consumed)?;
        self.state
            .as_mut()
            .ok_or(ReplayError::NoRound)?
            .chi(actor, target, called, consumed)
            .map_err(ReplayError::Call)
    }

    fn pon(
        &mut self,
        actor: u8,
        target: u8,
        pai: convlog::Tile,
        consumed: [convlog::Tile; 2],
    ) -> Result<(), ReplayError> {
        let actor = convert_player(actor)?;
        let target = convert_player(target)?;
        let called = convert_tile(pai)?;
        let consumed = convert_consumed(consumed)?;
        self.state
            .as_mut()
            .ok_or(ReplayError::NoRound)?
            .pon(actor, target, called, consumed)
            .map_err(ReplayError::Call)
    }

    fn daiminkan(
        &mut self,
        actor: u8,
        target: u8,
        pai: convlog::Tile,
        consumed: [convlog::Tile; 3],
    ) -> Result<(), ReplayError> {
        let actor = convert_player(actor)?;
        let target = convert_player(target)?;
        let called = convert_tile(pai)?;
        let consumed = convert_three_tiles(consumed)?;
        self.state
            .as_mut()
            .ok_or(ReplayError::NoRound)?
            .daiminkan(actor, target, called, consumed)
            .map_err(ReplayError::Call)
    }

    fn ankan(&mut self, actor: u8, consumed: [convlog::Tile; 4]) -> Result<(), ReplayError> {
        let actor = convert_player(actor)?;
        let consumed = convert_four_tiles(consumed)?;
        self.state
            .as_mut()
            .ok_or(ReplayError::NoRound)?
            .ankan(actor, consumed)
            .map_err(ReplayError::Call)
    }

    fn kakan(
        &mut self,
        actor: u8,
        pai: convlog::Tile,
        consumed: [convlog::Tile; 3],
    ) -> Result<(), ReplayError> {
        let actor = convert_player(actor)?;
        let added = convert_tile(pai)?;
        let consumed = convert_three_tiles(consumed)?;
        self.state
            .as_mut()
            .ok_or(ReplayError::NoRound)?
            .kakan(actor, added, consumed)
            .map_err(ReplayError::Call)
    }

    fn reveal_dora(&mut self, marker: convlog::Tile) -> Result<(), ReplayError> {
        let marker = convert_tile(marker)?;
        self.state
            .as_mut()
            .ok_or(ReplayError::NoRound)?
            .reveal_dora(marker);
        Ok(())
    }

    fn declare_riichi(&mut self, actor: u8) -> Result<(), ReplayError> {
        let actor = convert_player(actor)?;
        self.state
            .as_mut()
            .ok_or(ReplayError::NoRound)?
            .declare_riichi(actor)
            .map_err(ReplayError::Riichi)
    }

    fn accept_riichi(&mut self, actor: u8) -> Result<(), ReplayError> {
        let actor = convert_player(actor)?;
        self.state
            .as_mut()
            .ok_or(ReplayError::NoRound)?
            .accept_riichi(actor)
            .map_err(ReplayError::Riichi)
    }
}

impl fmt::Display for ReplayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedEvent => formatter.write_str("event is not supported yet"),
            Self::NoRound => formatter.write_str("no round has started"),
            Self::InvalidPlayer { value } => {
                write!(formatter, "invalid player index {value}; expected 0..=3")
            }
            Self::InvalidTile { value } => {
                write!(formatter, "invalid or unknown tile value {value}")
            }
            Self::InvalidRoundWind { value } => {
                write!(formatter, "tile value {value} is not a round wind")
            }
            Self::InvalidRoundNumber { value } => {
                write!(formatter, "invalid round number {value}; expected 1..=4")
            }
            Self::InvalidHandSize {
                player,
                effective_tile_count,
            } => write!(
                formatter,
                "player {} has invalid effective hand size {} after transition",
                player.get_id(),
                effective_tile_count
            ),
            Self::TileNotInHand { player, tile } => write!(
                formatter,
                "player {} does not hold tile {}",
                player.get_id(),
                tile.as_u8()
            ),
            Self::NoRemainingDraws => formatter.write_str("no draws remain"),
            Self::Call(error) => error.fmt(formatter),
            Self::Riichi(error) => error.fmt(formatter),
        }
    }
}

impl Error for ReplayError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Call(error) => Some(error),
            Self::Riichi(error) => Some(error),
            _ => None,
        }
    }
}

fn player_from_initial_hand(
    index: u8,
    score: i32,
    tiles: &[convlog::Tile; 13],
) -> Result<PlayerState, ReplayError> {
    let player = convert_player(index)?;
    let concealed = tiles
        .iter()
        .copied()
        .map(convert_tile)
        .collect::<Result<Vec<_>, _>>()?;
    let hand = make_hand(player, concealed, vec![])?;
    Ok(PlayerState::new(hand, score, vec![]))
}

fn make_hand(
    player: PlayerIndex,
    concealed: Vec<Tile>,
    melds: Vec<crate::mahjong::meld::Meld>,
) -> Result<Hand, ReplayError> {
    Hand::new(concealed, melds).map_err(|error| ReplayError::InvalidHandSize {
        player,
        effective_tile_count: error.effective_tile_count(),
    })
}

fn convert_player(value: u8) -> Result<PlayerIndex, ReplayError> {
    PlayerIndex::new(value).ok_or(ReplayError::InvalidPlayer { value })
}

fn convert_tile(tile: convlog::Tile) -> Result<Tile, ReplayError> {
    Tile::new(tile.as_u8()).ok_or(ReplayError::InvalidTile {
        value: tile.as_u8(),
    })
}

fn convert_consumed(tiles: [convlog::Tile; 2]) -> Result<[Tile; 2], ReplayError> {
    Ok([convert_tile(tiles[0])?, convert_tile(tiles[1])?])
}

fn convert_three_tiles(tiles: [convlog::Tile; 3]) -> Result<[Tile; 3], ReplayError> {
    Ok([
        convert_tile(tiles[0])?,
        convert_tile(tiles[1])?,
        convert_tile(tiles[2])?,
    ])
}

fn convert_four_tiles(tiles: [convlog::Tile; 4]) -> Result<[Tile; 4], ReplayError> {
    Ok([
        convert_tile(tiles[0])?,
        convert_tile(tiles[1])?,
        convert_tile(tiles[2])?,
        convert_tile(tiles[3])?,
    ])
}

fn convert_wind(tile: convlog::Tile) -> Result<Wind, ReplayError> {
    match tile.as_u8() {
        27 => Ok(Wind::East),
        28 => Ok(Wind::South),
        29 => Ok(Wind::West),
        30 => Ok(Wind::North),
        value => Err(ReplayError::InvalidRoundWind { value }),
    }
}

fn translate_hand_error(player: PlayerIndex, error: HandMutationError) -> ReplayError {
    match error {
        HandMutationError::InvalidSize(error) => ReplayError::InvalidHandSize {
            player,
            effective_tile_count: error.effective_tile_count(),
        },
        HandMutationError::TileNotFound { tile } => ReplayError::TileNotInHand { player, tile },
        error @ (HandMutationError::InvalidChi { .. }
        | HandMutationError::InvalidPon { .. }
        | HandMutationError::InvalidDaiminkan { .. }
        | HandMutationError::InvalidAnkan { .. }
        | HandMutationError::InvalidKakan { .. }
        | HandMutationError::PonNotFound { .. }) => {
            ReplayError::Call(CallError::Hand { player, error })
        }
    }
}

fn translate_draw_error(player: PlayerIndex, error: DrawError) -> ReplayError {
    match error {
        DrawError::NoRemainingDraws => ReplayError::NoRemainingDraws,
        DrawError::Hand(error) => translate_hand_error(player, error),
    }
}
