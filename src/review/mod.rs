//! 将指定事件后的可见局面、基础牌效率和 Mortal 判断汇总为复盘数据。

use std::{error::Error, fmt};

use convlog::Event;

use crate::analysis::{AnalysisError, DiscardEfficiency, discard_efficiency};
use crate::mahjong::{
    meld::Meld,
    player::{Discard, RiichiState},
    player_index::PlayerIndex,
    round::{RoundId, RoundPhase, RoundState},
    tile::Tile,
};
use crate::mortal::{Action, Decision, ModelInfo, Mortal, MortalConfig, MortalError};
use crate::replay::replayer::{ReplayError, Replayer};

mod game;
pub use game::{DecisionPoint, GameReview, RecordedAction, review_game};

/// 指定玩家在一个事件应用后的复盘结果，不包含对手暗牌或后续事件。
#[derive(Debug)]
pub struct Review {
    /// 与 replay 命令一致的零基全局事件编号。
    pub event_index: usize,
    pub player: PlayerIndex,
    pub position: VisiblePosition,
    pub model: ModelInfo,
    /// 无行动机会为 None；主动跳过仍为 Some，保留引擎最终推荐。
    pub decision: Option<Decision>,
    /// 仅分析 `Decision.candidates` 中的切牌项，保留其相对顺序；无切牌候选时为空。
    /// 返回时按引擎动作编码排列，不随 CLI 候选表按 Q 值排序。
    /// 应通过 `discard` 对应 `Action::Discard(tile)`，不要按数组下标关联两者。
    pub discards: Vec<DiscardEfficiency>,
}

/// 玩家视角的局面快照。四家副露位于 players，自家暗牌另存。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisiblePosition {
    pub round: RoundId,
    pub honba: u8,
    pub riichi_sticks: u8,
    pub remaining_draws: u8,
    pub phase: RoundPhase,
    pub dora_indicators: Vec<Tile>,
    /// 包含尚未打出的摸牌，按领域牌顺序排列。
    pub concealed: Vec<Tile>,
    /// 按整场固定玩家索引 0..3 排列，不含任何玩家的暗牌。
    pub players: [PublicPlayer; 4],
}

/// 每位玩家公开可见的信息。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicPlayer {
    pub score: i32,
    pub riichi: RiichiState,
    pub discards: Vec<Discard>,
    pub melds: Vec<Meld>,
}

/// 复盘失败的步骤与上下文；底层错误保留在 source 中。
#[derive(Debug)]
pub enum ReviewError {
    EventOutOfRange {
        event_index: usize,
        event_count: usize,
    },
    NoRound {
        event_index: usize,
    },
    Replay {
        event_index: usize,
        source: ReplayError,
    },
    Start(MortalError),
    Inference {
        event_index: usize,
        source: MortalError,
    },
    Finish(MortalError),
    /// 引擎给出切牌候选，但领域局面并未轮到指定玩家切牌。
    UnexpectedDiscard {
        event_index: usize,
        player: PlayerIndex,
        phase: RoundPhase,
    },
    Analysis {
        event_index: usize,
        discard: Tile,
        source: AnalysisError,
    },
}

/// 从牌谱开头回放至指定事件，返回该事件应用后的复盘数据。
///
/// 先校验所需历史，再启动一次 Mortal 会话并按原顺序输入相同历史；不执行模型推荐，
/// 不读取目标之后的事件。成功返回前关闭并检查引擎退出状态。每次调用重新加载模型。
/// 输入格式转换由调用方负责；无行动机会仍返回局面，尚未开局则返回 NoRound。
pub fn review_at(
    events: &[Event],
    player: PlayerIndex,
    event_index: usize,
    config: &MortalConfig<'_>,
) -> Result<Review, ReviewError> {
    if event_index >= events.len() {
        return Err(ReviewError::EventOutOfRange {
            event_index,
            event_count: events.len(),
        });
    }
    let history = &events[..=event_index];
    let mut replay = Replayer::new();
    for (index, event) in history.iter().enumerate() {
        replay.apply(event).map_err(|source| ReviewError::Replay {
            event_index: index,
            source,
        })?;
    }
    let state = replay.state().ok_or(ReviewError::NoRound { event_index })?;
    let mut mortal = Mortal::start(config, player).map_err(ReviewError::Start)?;
    let model = ModelInfo {
        version: mortal.model().version,
        tag: mortal.model().tag.clone(),
        sha256: mortal.model().sha256.clone(),
    };
    let mut decision = None;
    for (index, event) in history.iter().enumerate() {
        decision = mortal
            .react(event)
            .map_err(|source| ReviewError::Inference {
                event_index: index,
                source,
            })?;
    }
    mortal.finish().map_err(ReviewError::Finish)?;
    let discards = analyze_discards(state, player, event_index, decision.as_ref())?;
    Ok(Review {
        event_index,
        player,
        position: visible_position(state, player),
        model,
        decision,
        discards,
    })
}

fn visible_position(state: &RoundState, player: PlayerIndex) -> VisiblePosition {
    VisiblePosition {
        round: state.round(),
        honba: state.honba(),
        riichi_sticks: state.riichi_sticks(),
        remaining_draws: state.remaining_draws(),
        phase: state.phase(),
        dora_indicators: state.dora_indicators().to_vec(),
        concealed: state.player(player).hand().concealed().to_vec(),
        players: std::array::from_fn(|index| {
            let public = &state.players()[index];
            PublicPlayer {
                score: public.score(),
                riichi: public.riichi(),
                discards: public.discards().to_vec(),
                melds: public.hand().melds().to_vec(),
            }
        }),
    }
}

fn analyze_discards(
    state: &RoundState,
    player: PlayerIndex,
    event_index: usize,
    decision: Option<&Decision>,
) -> Result<Vec<DiscardEfficiency>, ReviewError> {
    let mut discards = Vec::new();
    if let Some(decision) = decision {
        for candidate in &decision.candidates {
            if let Action::Discard(discard) = candidate.action {
                // 牌效率本身只检查手牌张数，这里补上与决策时刻的对应约束。
                if !matches!(state.phase(), RoundPhase::AfterDraw { player: actor, .. } | RoundPhase::AfterCall { player: actor } if actor == player)
                {
                    return Err(ReviewError::UnexpectedDiscard {
                        event_index,
                        player,
                        phase: state.phase(),
                    });
                }
                discards.push(
                    discard_efficiency(state, player, discard).map_err(|source| {
                        ReviewError::Analysis {
                            event_index,
                            discard,
                            source,
                        }
                    })?,
                );
            }
        }
    }
    Ok(discards)
}

impl fmt::Display for ReviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EventOutOfRange {
                event_index,
                event_count,
            } => write!(
                f,
                "event {event_index} is outside the event log ({event_count} events)"
            ),
            Self::NoRound { event_index } => write!(f, "no round at G{event_index:03}"),
            Self::Replay {
                event_index,
                source,
            } => write!(f, "replay at G{event_index:03}: {source}"),
            Self::Start(source) => write!(f, "starting review engine: {source}"),
            Self::Inference {
                event_index,
                source,
            } => write!(f, "inference at G{event_index:03}: {source}"),
            Self::Finish(source) => write!(f, "finishing review engine: {source}"),
            Self::UnexpectedDiscard {
                event_index,
                player,
                phase,
            } => write!(
                f,
                "discard candidate at G{event_index:03} for P{} is incompatible with {phase:?}",
                player.get_id()
            ),
            Self::Analysis {
                event_index,
                discard,
                source,
            } => write!(
                f,
                "discard {discard:?} analysis at G{event_index:03}: {source}"
            ),
        }
    }
}

impl Error for ReviewError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Replay { source, .. } => Some(source),
            Self::Start(source) | Self::Inference { source, .. } | Self::Finish(source) => {
                Some(source)
            }
            Self::Analysis { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
