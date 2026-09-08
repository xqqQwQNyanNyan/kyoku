use super::*;
use convlog::Event;
use sha2::{Digest, Sha256};

/// 会话关联的牌谱；本地文件只保存 key，读取牌桌或导出时再补齐 events。
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionGame {
    pub key: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<Event>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub round_details: Vec<crate::replay::RoundDetails>,
}

impl SessionGame {
    pub fn key(events: &[Event]) -> Result<String, UiError> {
        let bytes = serde_json::to_vec(events).map_err(|_| io_error())?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }

    pub(crate) fn valid_key(key: &str) -> bool {
        key.len() == 64
            && key
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }

    pub(crate) fn validate(&self) -> Result<(), UiError> {
        if self.events.is_empty() || Self::key(&self.events)? != self.key {
            return Err(UiError::new("session_format", "会话关联的牌谱无效"));
        }
        if !self.round_details.is_empty() {
            crate::replay::replay(&self.events)?.with_details(&self.round_details)?;
        }
        Ok(())
    }
}

/// 会话中最后浏览的位置，独立于已经发送的问题快照。
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionPosition {
    pub player: u8,
    pub event_index: usize,
}

impl SessionPosition {
    pub fn from_evidence(evidence: &serde_json::Value) -> Result<Self, UiError> {
        let player = evidence["player"]
            .as_u64()
            .filter(|p| *p < 4)
            .ok_or_else(|| UiError::new("session_format", "会话玩家无效"))?;
        let event_index = evidence["event_index"]
            .as_u64()
            .and_then(|index| usize::try_from(index).ok())
            .ok_or_else(|| UiError::new("session_format", "会话事件编号无效"))?;
        Ok(Self {
            player: player as u8,
            event_index,
        })
    }

    pub(super) fn validate(&self, game: &SessionGame) -> Result<(), UiError> {
        if self.player >= 4
            || self.event_index >= game.events.len()
            || !game.events[..=self.event_index]
                .iter()
                .any(|event| matches!(event, Event::StartKyoku { .. }))
        {
            return Err(UiError::new("session_format", "会话位置超出牌谱范围"));
        }
        Ok(())
    }
}

pub(crate) struct SessionSource<'a> {
    pub game: &'a SessionGame,
    pub label: &'a str,
    pub context: &'a kyoku::agent::AgentContext,
}
