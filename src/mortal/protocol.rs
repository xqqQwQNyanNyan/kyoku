use std::error::Error;
use std::fmt;

use convlog::Event;
use serde::Deserialize;
use serde_json::{Value, json};

use super::MortalError;
use crate::mahjong::player_index::PlayerIndex;
use crate::mahjong::tile::{Tile, TileKind};

/// Mortal 主决策的动作类别。鸣牌细节以 `Decision::recommended` 为准。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Discard(Tile),
    Riichi,
    /// 鸣入牌位于顺子的左侧，例如鸣入 3 组成 345。
    ChiLow,
    ChiMiddle,
    ChiHigh,
    Pon,
    /// 主决策只表示是否杠；暗杠／加杠的牌种另见 `kan_candidates`。
    Kan,
    Win,
    AbortiveDraw,
    Pass,
}

/// 一个可选动作的原始 Q 值；不是和牌率、置信度或期望点数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub action: Action,
    pub q_value: f32,
}

/// 杠牌种选择阶段的评价，不与主决策阶段的 Q 值混排。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KanCandidate {
    pub tile: TileKind,
    pub q_value: f32,
}

/// 单个牌谱事件之后的模型判断。
#[derive(Debug)]
pub struct Decision {
    pub recommended: Event,
    /// 按引擎动作编码排列，不预先按 Q 值排序。
    pub candidates: Vec<Candidate>,
    pub kan_candidates: Vec<KanCandidate>,
    /// 引擎提供的诊断值，与 Kyoku 自己的分析结果独立。
    pub shanten: Option<i8>,
    pub at_furiten: Option<bool>,
}

/// 外部推理输出违反协议的具体原因。
#[derive(Debug, PartialEq, Eq)]
pub enum ProtocolError {
    InvalidModelInfo,
    MissingMetadata,
    MissingMask,
    InvalidMask { bits: u64, action_count: u8 },
    QValueCount { expected: usize, actual: usize },
    NonFiniteQValue,
    UnexpectedAction,
    WrongActor { expected: u8, actual: u8 },
    RecommendationOutsideMask,
    InvalidKanSelection,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Error for ProtocolError {}

#[derive(Deserialize)]
struct Response {
    #[serde(flatten)]
    event: Event,
    meta: Option<Metadata>,
}

#[derive(Deserialize)]
struct Metadata {
    mask_bits: Option<u64>,
    q_values: Option<Vec<f32>>,
    shanten: Option<i8>,
    at_furiten: Option<bool>,
    kan_select: Option<Box<Metadata>>,
}

pub(super) fn visible_event(event: &Event, player: PlayerIndex) -> serde_json::Result<Value> {
    let mut value = serde_json::to_value(event)?;
    match event {
        Event::StartKyoku { .. } => {
            for actor in 0..4 {
                if actor != usize::from(player.get_id()) {
                    value["tehais"][actor] = json!(vec!["?"; 13]);
                }
            }
        }
        Event::Tsumo { actor, .. } if *actor != player.get_id() => value["pai"] = json!("?"),
        _ => {}
    }
    Ok(value)
}

pub(super) fn decision(line: &str, player: PlayerIndex) -> Result<Option<Decision>, MortalError> {
    let response: Response = serde_json::from_str(line).map_err(MortalError::Json)?;
    decode(response, player).map_err(MortalError::Protocol)
}

fn decode(response: Response, player: PlayerIndex) -> Result<Option<Decision>, ProtocolError> {
    let meta = response.meta.ok_or(ProtocolError::MissingMetadata)?;
    let values = unpack(&meta, 46)?;
    if values.is_empty() {
        if response.event != Event::None || meta.kan_select.is_some() {
            return Err(ProtocolError::UnexpectedAction);
        }
        return Ok(None);
    }
    if let Some(actor) = response.event.actor()
        && actor != player.get_id()
    {
        return Err(ProtocolError::WrongActor {
            expected: player.get_id(),
            actual: actor,
        });
    }
    let label = recommendation_label(&response.event)?;
    if !values.iter().any(|&(index, _)| index == label) {
        return Err(ProtocolError::RecommendationOutsideMask);
    }
    let candidates = values
        .into_iter()
        .map(|(index, q_value)| {
            let action = match index {
                0..=36 => Action::Discard(Tile::new(index).expect("validated tile index")),
                37 => Action::Riichi,
                38 => Action::ChiLow,
                39 => Action::ChiMiddle,
                40 => Action::ChiHigh,
                41 => Action::Pon,
                42 => Action::Kan,
                43 => Action::Win,
                44 => Action::AbortiveDraw,
                45 => Action::Pass,
                _ => unreachable!("validated action index"),
            };
            Candidate { action, q_value }
        })
        .collect();
    let kan_candidates = if let Some(kan) = &meta.kan_select {
        if meta.mask_bits.is_none_or(|bits| bits & (1 << 42) == 0) || kan.kan_select.is_some() {
            return Err(ProtocolError::InvalidKanSelection);
        }
        let values = unpack(kan, 34)?;
        if values.is_empty() {
            return Err(ProtocolError::InvalidKanSelection);
        }
        values
            .into_iter()
            .map(|(index, q_value)| KanCandidate {
                tile: TileKind::new(index).expect("validated tile kind index"),
                q_value,
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok(Some(Decision {
        recommended: response.event,
        candidates,
        kan_candidates,
        shanten: meta.shanten,
        at_furiten: meta.at_furiten,
    }))
}

fn unpack(meta: &Metadata, action_count: u8) -> Result<Vec<(u8, f32)>, ProtocolError> {
    let bits = meta.mask_bits.ok_or(ProtocolError::MissingMask)?;
    if bits >> action_count != 0 {
        return Err(ProtocolError::InvalidMask { bits, action_count });
    }
    let values = meta.q_values.as_deref().unwrap_or_default();
    let expected = bits.count_ones() as usize;
    if values.len() != expected {
        return Err(ProtocolError::QValueCount {
            expected,
            actual: values.len(),
        });
    }
    if values.iter().any(|value| !value.is_finite()) {
        return Err(ProtocolError::NonFiniteQValue);
    }
    Ok((0..action_count)
        .filter(|index| bits & (1 << index) != 0)
        .zip(values.iter().copied())
        .collect())
}

fn recommendation_label(event: &Event) -> Result<u8, ProtocolError> {
    let valid_tile = |tile: convlog::Tile| tile.as_u8() < 37;
    match event {
        Event::Dahai { pai, .. } if valid_tile(*pai) => Ok(pai.as_u8()),
        Event::Reach { .. } => Ok(37),
        Event::Chi {
            target,
            pai,
            consumed,
            ..
        } if *target < 4 && valid_tile(*pai) && consumed.iter().all(|tile| valid_tile(*tile)) => {
            let called = pai.deaka().as_u8();
            let mut tiles = [
                called,
                consumed[0].deaka().as_u8(),
                consumed[1].deaka().as_u8(),
            ];
            tiles.sort();
            if tiles[0] >= 27
                || tiles[0] / 9 != tiles[2] / 9
                || tiles[1] != tiles[0] + 1
                || tiles[2] != tiles[0] + 2
            {
                return Err(ProtocolError::UnexpectedAction);
            }
            Ok(38 + called - tiles[0])
        }
        Event::Pon {
            target,
            pai,
            consumed,
            ..
        } if *target < 4
            && valid_tile(*pai)
            && consumed
                .iter()
                .all(|tile| valid_tile(*tile) && tile.deaka() == pai.deaka()) =>
        {
            Ok(41)
        }
        Event::Daiminkan {
            target,
            pai,
            consumed,
            ..
        } if *target < 4
            && valid_tile(*pai)
            && consumed
                .iter()
                .all(|tile| valid_tile(*tile) && tile.deaka() == pai.deaka()) =>
        {
            Ok(42)
        }
        Event::Kakan { pai, consumed, .. }
            if valid_tile(*pai)
                && consumed
                    .iter()
                    .all(|tile| valid_tile(*tile) && tile.deaka() == pai.deaka()) =>
        {
            Ok(42)
        }
        Event::Ankan { consumed, .. }
            if consumed
                .iter()
                .all(|tile| valid_tile(*tile) && tile.deaka() == consumed[0].deaka()) =>
        {
            Ok(42)
        }
        Event::Hora { target, .. } if *target < 4 => Ok(43),
        Event::Ryukyoku { .. } => Ok(44),
        Event::None => Ok(45),
        _ => Err(ProtocolError::UnexpectedAction),
    }
}

#[cfg(test)]
mod tests;
