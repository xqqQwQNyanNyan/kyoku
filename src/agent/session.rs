use super::*;
use serde::{Deserialize, Serialize};

/// 可移植的会话文件格式错误；读取文件不会创建客户端或发起请求。
#[derive(Debug)]
pub enum SessionFormatError {
    TooLarge,
    InvalidJson,
    InvalidContext(&'static str),
    IncompatibleVersion,
}

impl fmt::Display for SessionFormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => write!(f, "会话文件不能超过 32 MiB"),
            Self::InvalidJson => write!(f, "会话 JSON 格式无效"),
            Self::InvalidContext(reason) => write!(f, "会话上下文无效：{reason}"),
            Self::IncompatibleVersion => write!(f, "会话版本或提示词与当前版本不兼容"),
        }
    }
}
impl Error for SessionFormatError {}

/// 完整的可续聊上下文和执行轨迹；不包含连接密钥或完整牌谱。
/// 字段只通过序列化查看，加载必须经过 `from_json` 校验。
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionArchive {
    version: u32,
    instructions: String,
    tools: Vec<Value>,
    endpoint: String,
    model: String,
    evidence: Value,
    history: Vec<Value>,
    turns: Vec<Turn>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Turn {
    question: String,
    answer: Option<String>,
    error: Option<String>,
    trace: Vec<Value>,
}

impl SessionArchive {
    pub(super) fn new(evidence: Value, config: &AgentConfig<'_>) -> Self {
        Self {
            version: 1,
            instructions: INSTRUCTIONS.into(),
            tools: tool_definitions(),
            endpoint: config.endpoint.into(),
            model: config.model.into(),
            evidence,
            history: Vec::new(),
            turns: Vec::new(),
        }
    }

    pub(super) fn record(
        &mut self,
        question: &str,
        answer: Option<&str>,
        error: Option<String>,
        trace: Vec<Value>,
        history: &[Value],
    ) {
        self.history = history.to_vec();
        self.turns.push(Turn {
            question: question.trim().into(),
            answer: answer.map(str::to_owned),
            error,
            trace,
        });
    }

    /// 读取并校验版本、历史消息和工具证据，不使用文件内地址发起连接。
    pub fn from_json(json: &str) -> Result<Self, SessionFormatError> {
        if json.len() > 32 * 1024 * 1024 {
            return Err(SessionFormatError::TooLarge);
        }
        let archive: Self =
            serde_json::from_str(json).map_err(|_| SessionFormatError::InvalidJson)?;
        archive.validate()?;
        Ok(archive)
    }

    /// 最近一轮的失败说明，用于历史任务状态；成功或尚未提问时为 None。
    pub fn last_error(&self) -> Option<&str> {
        self.turns.last().and_then(|turn| turn.error.as_deref())
    }

    /// 会话绑定的可见局面，用于历史列表定位。
    pub fn evidence(&self) -> &Value {
        &self.evidence
    }

    fn validate(&self) -> Result<(), SessionFormatError> {
        use SessionFormatError as E;
        if self.version != 1 {
            return Err(E::IncompatibleVersion);
        }
        if self.evidence["schema_version"] != 2
            || self.evidence["event_index"].as_u64().is_none()
            || self.evidence["player"].as_u64().is_none_or(|p| p >= 4)
            || !self.evidence["position"].is_object()
            || !self.evidence["discards"].is_array()
            || !self.evidence["mortal"].is_object()
        {
            return Err(E::InvalidContext("缺少有效的固定局面证据"));
        }
        let players = self.evidence["position"]["players"]
            .as_array()
            .ok_or(E::InvalidContext("缺少四家公开信息"))?;
        if players.len() != 4
            || players.iter().enumerate().any(|(i, p)| {
                p["player"].as_u64() != Some(i as u64)
                    || p.get("concealed").is_some()
                    || !p["discards"].is_array()
                    || !p["melds"].is_array()
            })
            || !self.evidence["position"]["concealed"].is_array()
        {
            return Err(E::InvalidContext("玩家公开信息或自家手牌无效"));
        }
        check_history(&self.history).map_err(|_| E::TooLarge)?;
        let mut calls = std::collections::HashMap::new();
        let mut pending = HashSet::new();
        let chat = self
            .endpoint
            .trim_end_matches('/')
            .ends_with("/chat/completions")
            || self
                .endpoint
                .trim_end_matches('/')
                .ends_with("/chat/completion");
        let mut chat_group_end = 0;
        for (index, item) in self.history.iter().enumerate() {
            let bad = || E::InvalidContext("历史消息或工具调用格式不正确");
            // Chat 原始消息必须与标准化轨迹一致，不能夹带额外角色或工具结果。
            if let Some(original) = item.get("_chat_message") {
                if !chat || index < chat_group_end {
                    return Err(bad());
                }
                let normalized =
                    client::validate_chat_history(original.clone()).map_err(|_| bad())?;
                let count = item["_chat_output_count"].as_u64().ok_or_else(bad)? as usize;
                if normalized.len() != count
                    || self.history.get(index..index.saturating_add(count))
                        != Some(normalized.as_slice())
                {
                    return Err(bad());
                }
                chat_group_end = index + count;
            }
            if chat
                && matches!(item["type"].as_str(), Some("message" | "function_call"))
                && index >= chat_group_end
            {
                return Err(bad());
            }
            match item["type"].as_str() {
                None if item["role"] == "user" && item["content"].is_string() => {
                    if !pending.is_empty() {
                        return Err(bad());
                    }
                }
                Some("reasoning") if !chat && item.get("role").is_none() => {}
                Some("message") if item["role"] == "assistant" && item["status"] == "completed" => {
                    let parts = item["content"].as_array().ok_or_else(bad)?;
                    if parts
                        .iter()
                        .any(|p| p["type"] != "output_text" || !p["text"].is_string())
                    {
                        return Err(bad());
                    }
                }
                Some("function_call") if item["status"] == "completed" => {
                    let id = required_string(item, "call_id").map_err(|_| bad())?;
                    let name = required_string(item, "name").map_err(|_| bad())?;
                    let args = item["arguments"].as_str().ok_or_else(bad)?;
                    if calls
                        .insert(id, execute_tool(&self.evidence, name, args).0)
                        .is_some()
                    {
                        return Err(bad());
                    }
                    pending.insert(id);
                }
                Some("function_call_output") => {
                    let id = required_string(item, "call_id").map_err(|_| bad())?;
                    let result: Value =
                        serde_json::from_str(item["output"].as_str().ok_or_else(bad)?)
                            .map_err(|_| bad())?;
                    if !pending.remove(id) || calls.get(id) != Some(&result) {
                        return Err(bad());
                    }
                }
                _ => return Err(bad()),
            }
        }
        if !pending.is_empty() {
            return Err(E::InvalidContext("工具调用缺少结果"));
        }
        for turn in &self.turns {
            if turn.answer.is_some() == turn.error.is_some()
                || turn.question.len() > MAX_QUESTION_BYTES
            {
                return Err(E::InvalidContext("问答状态不完整"));
            }
            for step in &turn.trace {
                let valid = match step["kind"].as_str() {
                    Some("request") => {
                        step["input"].is_array() && step["needs_evidence"].is_boolean()
                    }
                    Some("response") => step.get("output").is_some(),
                    Some("tool") => {
                        step["name"].is_string()
                            && step["call_id"].is_string()
                            && step["arguments"].is_string()
                            && step["result"].is_object()
                    }
                    Some("validation") => step["error"].is_string(),
                    _ => false,
                };
                if !valid {
                    return Err(E::InvalidContext("执行轨迹格式不正确"));
                }
            }
        }
        Ok(())
    }
}

impl AgentSession {
    /// 获取可保存的上下文；失败请求的轨迹也会保留，但不会进入后续模型历史。
    pub fn archive(&self) -> &SessionArchive {
        &self.archive
    }

    /// 使用当前用户配置恢复会话。地址和模型必须与原会话一致，密钥由调用方重新提供。
    pub fn from_archive(
        archive: &SessionArchive,
        config: &AgentConfig<'_>,
    ) -> Result<Self, AgentError> {
        archive
            .validate()
            .map_err(|_| invalid("invalid session archive"))?;
        if archive.instructions != INSTRUCTIONS || archive.tools != tool_definitions() {
            return Err(AgentError::InvalidConfig {
                field: "session",
                reason: "此会话的提示词或工具版本已改变，可以查看和导出历史；继续问答请新建会话",
            });
        }
        if archive.endpoint != config.endpoint || archive.model != config.model {
            return Err(AgentError::InvalidConfig {
                field: "session",
                reason: "请在设置中恢复此会话的服务地址和模型，或在当前局面新建会话",
            });
        }
        Ok(Self {
            client: client::Client::new(config)?,
            evidence: archive.evidence.clone(),
            history: archive.history.clone(),
            has_evidence: archive.history.iter().any(|item| {
                item["type"] == "function_call"
                    && item["name"] == "get_review"
                    && item["arguments"].as_str().is_some_and(|args| {
                        serde_json::from_str::<Value>(args).is_ok_and(|v| v == json!({}))
                    })
            }),
            archive: archive.clone(),
        })
    }
}
