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
/// 导入用 `from_json` 完整校验；本地浏览可用 `validate_for_display`，续聊仍会检查全部工具证据。
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionArchive {
    version: u32,
    instructions: String,
    tools: Vec<Value>,
    endpoint: String,
    model: String,
    pub(super) evidence: Value,
    history: Vec<Value>,
    turns: Vec<Turn>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Turn {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    usage: Vec<RequestUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    options: Option<ModelOptions>,
    question: String,
    answer: Option<String>,
    error: Option<String>,
    trace: Vec<Value>,
    /// 这一轮发送时的局面；旧版会话沿用顶层 evidence。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    evidence: Option<Value>,
}

impl SessionArchive {
    pub(super) fn new(evidence: Value, config: &AgentConfig<'_>) -> Self {
        Self {
            version: 2,
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
        result: Result<&str, &AgentError>,
        trace: Vec<Value>,
        history: &[Value],
        usage: Vec<RequestUsage>,
        options: ModelOptions,
    ) {
        self.history = history.to_vec();
        let (answer, error) = match result {
            Ok(answer) => (Some(answer.to_owned()), None),
            Err(error) => (None, Some(error.to_string())),
        };
        self.turns.push(Turn {
            usage,
            options: Some(options),
            question: question.trim().into(),
            answer,
            error,
            trace,
            evidence: Some(self.evidence.clone()),
        });
    }

    /// 读取并校验版本、历史消息和工具证据，不使用文件内地址发起连接。
    pub fn from_json(json: &str) -> Result<Self, SessionFormatError> {
        if json.len() > 32 * 1024 * 1024 {
            return Err(SessionFormatError::TooLarge);
        }
        let archive: Self =
            serde_json::from_str(json).map_err(|_| SessionFormatError::InvalidJson)?;
        archive.validate(true)?;
        Ok(archive)
    }

    /// 校验浏览所需的格式、消息配对和各轮快照，不重新执行分析工具。
    /// 此检查不证明历史计算结果正确；导入用 `from_json`，续聊用 `AgentSession::from_archive`。
    pub fn validate_for_display(&self) -> Result<(), SessionFormatError> {
        self.validate(false)
    }

    /// 最近一轮的失败说明，用于历史任务状态；成功或尚未提问时为 None。
    pub fn last_error(&self) -> Option<&str> {
        self.turns.last().and_then(|turn| turn.error.as_deref())
    }

    /// 最近一次提问使用的可见局面。
    pub fn evidence(&self) -> &Value {
        &self.evidence
    }

    /// 取得失败问题及它发送时的快照，供显式重试使用；成功的轮次不可重试。
    pub fn failed_turn_context(&self, index: usize) -> Option<(&str, AgentContext)> {
        let turn = self.turns.get(index)?;
        turn.error.as_ref()?;
        Some((
            &turn.question,
            AgentContext {
                evidence: turn.evidence.as_ref().unwrap_or(&self.evidence).clone(),
            },
        ))
    }

    fn validate(&self, verify_results: bool) -> Result<(), SessionFormatError> {
        use SessionFormatError as E;
        if !matches!(self.version, 1 | 2) {
            return Err(E::IncompatibleVersion);
        }
        validate_evidence(&self.evidence)?;
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
        let mut current_evidence = self.evidence.clone();
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
                None if context_evidence(item).is_some() => {
                    if !pending.is_empty() || (self.version == 1 && index != 0) {
                        return Err(bad());
                    }
                    let snapshot = context_evidence(item).ok_or_else(bad)?;
                    validate_evidence(&snapshot)?;
                    if self.version == 1 && snapshot != self.evidence {
                        return Err(bad());
                    }
                    current_evidence = snapshot;
                }
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
                    // 浏览历史不能触发昂贵的牌形枚举；导入和续聊仍重算校验。
                    let expected = if verify_results || name == "get_review" {
                        Some(execute_tool(&current_evidence, name, args).0)
                    } else {
                        None
                    };
                    if calls.insert(id, expected).is_some() {
                        return Err(bad());
                    }
                    pending.insert(id);
                }
                Some("function_call_output") => {
                    let id = required_string(item, "call_id").map_err(|_| bad())?;
                    let result: Value =
                        serde_json::from_str(item["output"].as_str().ok_or_else(bad)?)
                            .map_err(|_| bad())?;
                    if !pending.remove(id)
                        || !result.is_object()
                        || result["ok"].as_bool().is_none()
                        || calls
                            .get(id)
                            .and_then(Option::as_ref)
                            .is_some_and(|expected| *expected != result)
                    {
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
            if let Some(options) = &turn.options {
                options
                    .validate()
                    .map_err(|_| E::InvalidContext("模型配置无效"))?;
            }
            if turn.usage.len() > MAX_REQUESTS {
                return Err(E::InvalidContext("请求用量记录过多"));
            }
            for usage in &turn.usage {
                usage.validate().map_err(E::InvalidContext)?;
            }
            if let Some(evidence) = &turn.evidence {
                validate_evidence(evidence)?;
            }
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
                    Some("reference_repair") => step["repairs"].as_array().is_some_and(|repairs| {
                        !repairs.is_empty()
                            && repairs.iter().all(|repair| {
                                repair["from"].is_string() && repair["to"].is_string()
                            })
                    }),
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

fn validate_evidence(evidence: &Value) -> Result<(), SessionFormatError> {
    use SessionFormatError as E;
    if evidence["schema_version"] != 2
        || evidence["event_index"].as_u64().is_none()
        || evidence["player"].as_u64().is_none_or(|p| p >= 4)
        || !evidence["position"].is_object()
        || !evidence["discards"].is_array()
        || !evidence["mortal"].is_object()
    {
        return Err(E::InvalidContext("缺少有效的固定局面证据"));
    }
    let players = evidence["position"]["players"]
        .as_array()
        .ok_or(E::InvalidContext("缺少四家公开信息"))?;
    if players.len() != 4
        || players.iter().enumerate().any(|(i, p)| {
            p["player"].as_u64() != Some(i as u64)
                || p.get("concealed").is_some()
                || !p["discards"].is_array()
                || !p["melds"].is_array()
        })
        || !evidence["position"]["concealed"].is_array()
    {
        return Err(E::InvalidContext("玩家公开信息或自家手牌无效"));
    }
    Ok(())
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
        // 旧协议不能续聊时直接返回原因，不先花时间重算历史分析。
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
        archive
            .validate(true)
            .map_err(|_| invalid("invalid session archive"))?;
        Ok(Self {
            client: client::Client::new(config)?,
            evidence: archive.evidence.clone(),
            history: archive.history.clone(),
            has_evidence: match archive
                .history
                .iter()
                .filter_map(context_evidence)
                .next_back()
            {
                Some(evidence) => evidence == archive.evidence,
                None => archive.history.iter().any(|item| {
                    item["type"] == "function_call"
                        && item["name"] == "get_review"
                        && item["arguments"].as_str().is_some_and(|args| {
                            serde_json::from_str::<Value>(args).is_ok_and(|v| v == json!({}))
                        })
                }),
            },
            archive: archive.clone(),
        })
    }
}
