use super::*;
use serde::Serialize;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// 整场分析的实际阶段；事件计数不代表剩余时间。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum ReviewProgress {
    Preparing,
    Loading,
    Analyzing { completed: usize, total: usize },
    Finishing,
}

/// 一次整场分析的进度回调和取消信号；重试须新建实例。
#[derive(Clone)]
pub struct ReviewControl {
    cancelled: Arc<AtomicBool>,
    progress: Arc<dyn Fn(ReviewProgress) + Send + Sync>,
}

impl ReviewControl {
    /// 回调应及时返回，不阻塞推理。
    pub fn new(progress: impl Fn(ReviewProgress) + Send + Sync + 'static) -> Self {
        Self {
            cancelled: Arc::default(),
            progress: Arc::new(progress),
        }
    }

    /// 停止等待模型并回收进程；本地计算在事件边界停止。
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    fn check(&self) -> Result<(), ReviewError> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err(ReviewError::Cancelled)
        } else {
            Ok(())
        }
    }

    fn report(&self, progress: ReviewProgress) -> Result<(), ReviewError> {
        self.check()?;
        (self.progress)(progress);
        self.check()
    }
}

impl Default for ReviewControl {
    fn default() -> Self {
        Self::new(|_| {})
    }
}

/// 整份牌谱中指定玩家的决策缓存，按全局事件编号排列。
/// 只保留可见快照；查询不会再启动引擎或回放牌谱。
#[derive(Debug)]
pub struct GameReview {
    decisions: Vec<DecisionPoint>,
}

impl GameReview {
    /// 全部行动机会，包括只有一个候选及可以鸣牌但选择跳过的时刻。
    pub fn decisions(&self) -> &[DecisionPoint] {
        &self.decisions
    }

    /// 根据零基全局事件编号查询；该事件没有行动机会时返回 None。
    pub fn at_event(&self, event_index: usize) -> Option<&DecisionPoint> {
        self.decisions
            .binary_search_by_key(&event_index, |point| point.review.event_index)
            .ok()
            .map(|index| &self.decisions[index])
    }
}

/// 一次行动机会。实际动作来自后续牌谱，与当时可用于解释的证据分开。
#[derive(Debug)]
pub struct DecisionPoint {
    pub review: Review,
    /// 自家牌河长度加一；鸣牌响应也标在下一次出牌手番，杠不单独增加手番。
    pub turn: usize,
    pub actual: RecordedAction,
}

/// 牌谱能够确认的实际行动，不由模型推荐反推。
#[derive(Debug, PartialEq, Eq)]
pub enum RecordedAction {
    /// 玩家实际执行的动作及其零基事件编号；和牌不附带结算及里宝牌。
    Taken { event_index: usize, action: Event },
    /// 鸣牌或抢杠窗口结束，牌谱已进入下一次摸牌。
    Passed,
    /// 他家抢先行动、流局原因不明或牌谱截断，无法确认玩家的选择。
    Unresolved,
}

/// 校验整份输入后只启动一次 Mortal，顺序推理并缓存全部行动机会。
///
/// 沿真实牌谱推进，不执行模型建议。成功返回前检查引擎退出状态；失败不返回部分缓存。
/// 允许合法的牌谱前缀，末尾尚未发生的实际行动标为 Unresolved。空输入返回空列表。
pub fn review_game(
    events: &[Event],
    player: PlayerIndex,
    config: &MortalConfig<'_>,
) -> Result<GameReview, ReviewError> {
    review_game_with_control(events, player, config, &ReviewControl::default())
}

/// 分析整场并报告阶段和已完成事件；取消或失败不返回部分结果。
pub fn review_game_with_control(
    events: &[Event],
    player: PlayerIndex,
    config: &MortalConfig<'_>,
    control: &ReviewControl,
) -> Result<GameReview, ReviewError> {
    // 统一启动、推理和退出等待中的取消结果。
    let result = review_controlled(events, player, config, control);
    match result {
        Err(
            ReviewError::Start(MortalError::Cancelled)
            | ReviewError::Inference {
                source: MortalError::Cancelled,
                ..
            }
            | ReviewError::Finish(MortalError::Cancelled),
        ) => Err(ReviewError::Cancelled),
        result => result,
    }
}

fn review_controlled(
    events: &[Event],
    player: PlayerIndex,
    config: &MortalConfig<'_>,
    control: &ReviewControl,
) -> Result<GameReview, ReviewError> {
    control.report(ReviewProgress::Preparing)?;
    let mut replay = Replayer::new();
    for (event_index, event) in events.iter().enumerate() {
        control.check()?;
        replay.apply(event).map_err(|source| ReviewError::Replay {
            event_index,
            source,
        })?;
    }
    if events.is_empty() {
        return Ok(GameReview { decisions: vec![] });
    }
    control.report(ReviewProgress::Loading)?;
    let mut mortal = Mortal::start_with_cancellation(config, player, control.cancelled.clone())
        .map_err(ReviewError::Start)?;
    control.report(ReviewProgress::Analyzing {
        completed: 0,
        total: events.len(),
    })?;
    let mut replay = Replayer::new();
    let mut decisions = Vec::new();
    for (event_index, event) in events.iter().enumerate() {
        control.check()?;
        replay.apply(event).map_err(|source| ReviewError::Replay {
            event_index,
            source,
        })?;
        let decision = mortal
            .react(event)
            .map_err(|source| ReviewError::Inference {
                event_index,
                source,
            })?;
        if let Some(decision) = decision {
            let state = replay.state().ok_or(ReviewError::NoRound { event_index })?;
            let mut position = visible_position(state, player);
            position.history = public_history(&events[..=event_index]);
            decisions.push(DecisionPoint {
                turn: state.player(player).discards().len() + 1,
                actual: recorded_action(events, event_index, player),
                review: Review {
                    event_index,
                    player,
                    position,
                    model: ModelInfo {
                        version: mortal.model().version,
                        tag: mortal.model().tag.clone(),
                        sha256: mortal.model().sha256.clone(),
                    },
                    discards: analyze_discards(state, player, event_index, Some(&decision))?,
                    decision: Some(decision),
                },
            });
        }
        control.report(ReviewProgress::Analyzing {
            completed: event_index + 1,
            total: events.len(),
        })?;
    }
    control.report(ReviewProgress::Finishing)?;
    mortal.finish().map_err(ReviewError::Finish)?;
    control.check()?;
    Ok(GameReview { decisions })
}

fn recorded_action(events: &[Event], event_index: usize, player: PlayerIndex) -> RecordedAction {
    let id = player.get_id();
    let response_window = matches!(events[event_index],
        Event::Dahai { actor, .. } | Event::Kakan { actor, .. } | Event::Ankan { actor, .. } if actor != id);
    for (index, event) in events.iter().enumerate().skip(event_index + 1) {
        match event {
            Event::None | Event::Dora { .. } | Event::ReachAccepted { .. } => continue,
            // 双响的多个和牌事件属于同一个响应窗口。
            Event::Hora { actor, target, .. } if *actor == id => {
                return RecordedAction::Taken {
                    event_index: index,
                    action: Event::Hora {
                        actor: *actor,
                        target: *target,
                        deltas: None,
                        ura_markers: None,
                    },
                };
            }
            Event::Hora { .. } if response_window => continue,
            Event::Dahai { actor, .. }
            | Event::Reach { actor }
            | Event::Chi { actor, .. }
            | Event::Pon { actor, .. }
            | Event::Daiminkan { actor, .. }
            | Event::Ankan { actor, .. }
            | Event::Kakan { actor, .. }
                if *actor == id =>
            {
                return RecordedAction::Taken {
                    event_index: index,
                    action: event.clone(),
                };
            }
            Event::Tsumo { .. } if response_window => return RecordedAction::Passed,
            _ => return RecordedAction::Unresolved,
        }
    }
    RecordedAction::Unresolved
}

#[cfg(test)]
mod tests;
