use super::*;

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
    let mut replay = Replayer::new();
    for (event_index, event) in events.iter().enumerate() {
        replay.apply(event).map_err(|source| ReviewError::Replay {
            event_index,
            source,
        })?;
    }
    if events.is_empty() {
        return Ok(GameReview { decisions: vec![] });
    }
    let mut mortal = Mortal::start(config, player).map_err(ReviewError::Start)?;
    let mut replay = Replayer::new();
    let mut decisions = Vec::new();
    for (event_index, event) in events.iter().enumerate() {
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
            decisions.push(DecisionPoint {
                turn: state.player(player).discards().len() + 1,
                actual: recorded_action(events, event_index, player),
                review: Review {
                    event_index,
                    player,
                    position: visible_position(state, player),
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
    }
    mortal.finish().map_err(ReviewError::Finish)?;
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
