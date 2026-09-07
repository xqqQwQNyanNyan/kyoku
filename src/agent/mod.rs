//! 携带每轮可见局面的 LLM 工具调用与中文复盘会话。

use std::{collections::HashSet, error::Error, fmt};

use serde_json::{Value, json};

use crate::{
    mahjong::player_index::PlayerIndex,
    replay::replayer::Replayer,
    review::{PublicPlayer, Review, ReviewError, VisiblePosition},
};

mod client;
mod comparison;
mod evidence;
mod output;
mod position;
mod session;
mod strategy;

pub use session::{SessionArchive, SessionFormatError};

pub use evidence::review_evidence;

const INSTRUCTIONS: &str = include_str!("prompt.txt");
const MAX_REQUESTS: usize = 10;
const MAX_HISTORY_BYTES: usize = 1024 * 1024;
const MAX_QUESTION_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RequestMode {
    ReviewProbe,
    Analysis,
    Repair,
}

/// 固定事件的可见证据；可从已有分析或只回放牌谱建立，不要求运行 Mortal。
pub struct AgentContext {
    evidence: Value,
}

impl From<&Review> for AgentContext {
    fn from(review: &Review) -> Self {
        Self {
            evidence: review_evidence(review),
        }
    }
}

impl AgentContext {
    /// 只回放至目标事件，隐藏对手暗牌；不运行分析或 Mortal，状态明确标为未分析。
    pub fn from_events(
        events: &[convlog::Event],
        player: PlayerIndex,
        event_index: usize,
    ) -> Result<Self, ReviewError> {
        if event_index >= events.len() {
            return Err(ReviewError::EventOutOfRange {
                event_index,
                event_count: events.len(),
            });
        }
        let mut replay = Replayer::new();
        for (index, event) in events[..=event_index].iter().enumerate() {
            replay.apply(event).map_err(|source| ReviewError::Replay {
                event_index: index,
                source,
            })?;
        }
        let state = replay.state().ok_or(ReviewError::NoRound { event_index })?;
        let position = VisiblePosition {
            history: crate::review::public_history(&events[..=event_index]),
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
        };
        Ok(Self {
            evidence: evidence::position_evidence(event_index, player.get_id(), &position),
        })
    }

    /// 当前快照的只读证据，与会话提供给模型的内容一致。
    pub fn evidence(&self) -> &Value {
        &self.evidence
    }
}

/// Responses / Chat Completions API 连接参数。密钥只用于 HTTP 认证，不进入提示词或调试输出。
pub struct AgentConfig<'a> {
    /// 完整的 API 地址；`/chat/completions`（也接受 `/chat/completion`）使用 Chat 协议，其余使用 Responses。
    pub endpoint: &'a str,
    /// 服务端支持工具调用的模型名称。
    pub model: &'a str,
    /// 调用方为此地址明确提供的密钥；库不读取环境变量。
    /// 本地无认证服务可省略；远程服务要求提供密钥。
    pub api_key: Option<&'a str>,
}

impl AgentConfig<'_> {
    /// 校验连接参数，不发送请求。
    pub fn validate(&self) -> Result<(), AgentError> {
        client::Client::new(self).map(|_| ())
    }

    /// 通过两次不含牌谱的请求，检查工具调用及返回工具结果后的续答能力。
    /// 此操作可能产生服务商的调用费用，不保存服务端会话。
    pub fn test_connection(&self) -> Result<(), AgentError> {
        let client = client::Client::new(self)?;
        let mut input = vec![
            json!({"role": "user", "content": "连接测试：请调用 get_review；收到 connection_test=true 后直接确认连接成功，不再调用工具。这不是牌谱分析。"}),
        ];
        let response = client.respond(&input, RequestMode::ReviewProbe)?;
        if response["status"] != "completed" {
            return Err(AgentError::IncompleteResponse);
        }
        let output = response["output"]
            .as_array()
            .ok_or(invalid("missing output array"))?;
        let calls: Vec<_> = output
            .iter()
            .filter(|item| item["type"] == "function_call")
            .collect();
        if calls.len() != 1
            || !calls.iter().all(|item| {
                item["type"] == "function_call"
                    && item["status"] == "completed"
                    && item["name"] == "get_review"
                    && item["call_id"].as_str().is_some_and(|id| !id.is_empty())
                    && item["arguments"].as_str().is_some_and(|args| {
                        serde_json::from_str::<Value>(args).is_ok_and(|v| v == json!({}))
                    })
            })
        {
            return Err(invalid("service did not return the requested tool call"));
        }
        input.extend(output.iter().cloned());
        input.push(
            json!({"type":"function_call_output","call_id":calls[0]["call_id"],
            "output":json!({"ok":true,"connection_test":true}).to_string()}),
        );
        let response = client.respond(&input, RequestMode::Analysis)?;
        if response["status"] != "completed" {
            return Err(AgentError::IncompleteResponse);
        }
        let output = response["output"]
            .as_array()
            .ok_or(invalid("missing output array"))?;
        if output.iter().any(|item| item["type"] == "function_call") {
            return Err(invalid("service did not finish the tool roundtrip test"));
        }
        if !output.iter().any(|item| {
            item["type"] == "message"
                && item["role"] == "assistant"
                && item["status"] == "completed"
                && item["content"].as_array().is_some_and(|parts| {
                    parts.iter().any(|part| {
                        part["type"] == "output_text"
                            && part["text"]
                                .as_str()
                                .is_some_and(|text| !text.trim().is_empty())
                    })
                })
        }) {
            return Err(invalid(
                "service did not answer after receiving the tool result",
            ));
        }
        Ok(())
    }
}

/// 服务端结构化错误的有界、脱敏摘要，不包含完整 HTTP 响应正文。
#[derive(Debug)]
pub struct ProviderError {
    /// 服务端错误码。
    pub code: Option<String>,
    /// 服务端指出的相关参数。
    pub parameter: Option<String>,
    /// 服务端错误说明，已移除本次认证密钥及 Bearer 凭据。
    pub message: Option<String>,
}

/// Agent 调用失败，不包含认证密钥、完整 HTTP 正文或未经校验的模型输出。
#[derive(Debug)]
pub enum AgentError {
    InvalidConfig {
        field: &'static str,
        reason: &'static str,
    },
    InvalidQuestion,
    /// 网络或读取失败，仅保留错误类别，避免远端回显请求中的秘密。
    Transport {
        kind: &'static str,
    },
    Http {
        status: u16,
        error: Option<ProviderError>,
    },
    InvalidResponse {
        reason: &'static str,
    },
    Refused,
    IncompleteResponse,
    MissingEvidence,
    RequestLimit,
    HistoryLimit,
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig { field, reason } => write!(f, "invalid {field}: {reason}"),
            Self::InvalidQuestion => write!(
                f,
                "question must contain 1..={MAX_QUESTION_BYTES} UTF-8 bytes after trimming"
            ),
            Self::Transport { kind } => write!(f, "LLM request failed ({kind})"),
            Self::Http { status, error } => {
                let hint = match status {
                    400 | 422 => "请求参数被服务端拒绝，请检查模型名和接口参数兼容性",
                    401 | 403 => "认证或访问权限失败，请检查该服务的 API Key 和模型权限",
                    402 => "服务账户余额不足，请检查账户额度",
                    404 => "接口或模型不存在，请检查完整服务地址和模型名",
                    429 => "请求频率或配额受限，请检查服务额度并稍后重试",
                    500..=599 => "服务端暂时不可用，请稍后重试",
                    _ => "请检查服务配置与状态",
                };
                write!(f, "LLM returned HTTP {status}: {hint}")?;
                if let Some(error) = error {
                    if let Some(code) = &error.code {
                        write!(f, " [code={code}]")?;
                    }
                    if let Some(parameter) = &error.parameter {
                        write!(f, " [param={parameter}]")?;
                    }
                    if let Some(message) = &error.message {
                        write!(f, "; {message}")?;
                    }
                }
                Ok(())
            }
            Self::InvalidResponse { reason } => {
                write!(f, "invalid LLM API response: {reason}")
            }
            Self::Refused => write!(f, "LLM declined this request"),
            Self::IncompleteResponse => write!(
                f,
                "LLM response did not complete; no partial answer was saved"
            ),
            Self::MissingEvidence => {
                write!(f, "LLM tried to answer before obtaining review evidence")
            }
            Self::RequestLimit => {
                write!(f, "LLM exceeded {MAX_REQUESTS} requests for one question")
            }
            Self::HistoryLimit => write!(
                f,
                "conversation or execution trace reached capacity; start a new session"
            ),
        }
    }
}

impl Error for AgentError {}

/// 只持有可见复盘证据与内存对话的会话，不持有完整牌谱或 Mortal 进程。
///
/// 调用方提供已有 `Review` 或不依赖分析的 `AgentContext`。失败的问答不会写入对话历史，
/// 可以重试；已经发送的 HTTP 请求不会被撤销。
pub struct AgentSession {
    client: client::Client,
    evidence: Value,
    history: Vec<Value>,
    has_evidence: bool,
    archive: SessionArchive,
}

impl AgentSession {
    /// 创建会话并校验连接参数；此时不发起 HTTP 请求。
    pub fn new(review: &Review, config: &AgentConfig<'_>) -> Result<Self, AgentError> {
        Self::with_context(&AgentContext::from(review), config)
    }

    /// 使用已建立的可见快照创建会话；快照不要求存在 Mortal 分析。
    pub fn with_context(
        context: &AgentContext,
        config: &AgentConfig<'_>,
    ) -> Result<Self, AgentError> {
        Ok(Self {
            client: client::Client::new(config)?,
            evidence: context.evidence.clone(),
            history: Vec::new(),
            has_evidence: false,
            archive: SessionArchive::new(context.evidence.clone(), config),
        })
    }

    /// 当前局面的工具证据，供命令行展示和人工核对，协议见 `docs/agent/agent.md`。
    pub fn evidence(&self) -> &Value {
        &self.evidence
    }

    /// 切换下一轮使用的局面，保留已有对话；历史计算只属于各自的快照。
    pub fn set_context(&mut self, context: &AgentContext) {
        if self.evidence != context.evidence {
            self.evidence = context.evidence.clone();
            self.archive.evidence = context.evidence.clone();
            self.has_evidence = false;
        }
    }

    /// 提问或追问。首次或局面改变后直接附带证据，最多请求模型十次。
    pub fn ask(&mut self, question: &str) -> Result<String, AgentError> {
        if serde_json::to_vec(&self.archive)
            .map_err(|_| AgentError::HistoryLimit)?
            .len()
            > 16 * 1024 * 1024
        {
            return Err(AgentError::HistoryLimit);
        }
        let mut trace = Vec::new();
        let result = answer_traced(
            &self.evidence,
            &self.history,
            self.has_evidence,
            question,
            |input, mode| self.client.respond(input, mode),
            &mut trace,
        );
        match result {
            Ok((answer, history)) => {
                self.history = history;
                self.has_evidence = true;
                self.archive
                    .record(question, Some(&answer), None, trace, &self.history);
                Ok(answer)
            }
            Err(error) => {
                self.archive.record(
                    question,
                    None,
                    Some(error.to_string()),
                    trace,
                    &self.history,
                );
                Err(error)
            }
        }
    }
}

fn tool_definition() -> Value {
    json!({
        "type": "function", "name": "get_review",
        "description": "读取当前固定局面的可见证据，以及切牌计算和 Mortal 的可用状态。未分析时只提供自家手牌和公开信息；不能补造分析。无对手暗牌或未来事件。输入必须是空对象。",
        "strict": true,
        "parameters": {"type": "object", "properties": {}, "required": [], "additionalProperties": false},
    })
}

fn tool_definitions() -> Vec<Value> {
    let mut definitions = vec![
        tool_definition(),
        comparison::definition(),
        comparison::all_definition(),
    ];
    definitions.extend(strategy::definitions());
    definitions
}

fn execute_tool(evidence: &Value, name: &str, arguments: &str) -> (Value, bool) {
    if name == "compare_discards" {
        // 比较成功也不能代替首次读取完整局面。
        return (comparison::execute(evidence, arguments), false);
    }
    if name == "compare_improvements" {
        return (comparison::execute_all(evidence, arguments), false);
    }
    if strategy::definitions()
        .iter()
        .any(|definition| definition["name"] == name)
    {
        return (strategy::execute(name, evidence, arguments), false);
    }
    if name != "get_review" {
        return (
            json!({"ok": false, "error": {"code": "unknown_tool", "message": "请使用当前 tools 中声明的工具。"}}),
            false,
        );
    }
    let valid = serde_json::from_str::<Value>(arguments)
        .is_ok_and(|value| value.as_object().is_some_and(|object| object.is_empty()));
    if !valid {
        return (
            json!({"ok": false, "error": {"code": "invalid_arguments", "message": "get_review requires exactly {}. The position is fixed by the user."}}),
            false,
        );
    }
    (json!({"ok": true, "review": evidence}), true)
}

#[cfg(test)]
fn answer(
    evidence: &Value,
    history: &[Value],
    has_evidence: bool,
    question: &str,
    respond: impl FnMut(&[Value], RequestMode) -> Result<Value, AgentError>,
) -> Result<(String, Vec<Value>), AgentError> {
    answer_traced(
        evidence,
        history,
        has_evidence,
        question,
        respond,
        &mut Vec::new(),
    )
}

fn answer_traced(
    evidence: &Value,
    history: &[Value],
    mut has_evidence: bool,
    question: &str,
    mut respond: impl FnMut(&[Value], RequestMode) -> Result<Value, AgentError>,
    trace: &mut Vec<Value>,
) -> Result<(String, Vec<Value>), AgentError> {
    let question = question.trim();
    if question.is_empty() || question.len() > MAX_QUESTION_BYTES {
        return Err(AgentError::InvalidQuestion);
    }
    let mut staged = history.to_vec();
    if !has_evidence {
        // 快照已由本地生成，不必让模型花一次请求来索取同一份证据。
        staged.push(initial_evidence(evidence));
        has_evidence = true;
    }
    let mut verified = evidence_with_history(evidence, &staged);
    let mut corrections = 0;
    let mut previous_error = None;
    let mut call_ids: HashSet<String> = history
        .iter()
        .filter(|item| item["type"] == "function_call")
        .filter_map(|item| item["call_id"].as_str().map(str::to_owned))
        .collect();
    staged.push(json!({"role": "user", "content": question}));
    // 纠错时临时给模型看被拒绝的输出，但不让错误回答进入后续追问的历史。
    let mut accepted = staged.clone();
    for _ in 0..MAX_REQUESTS {
        check_history(&staged)?;
        if json!(trace).to_string().len() > 8 * 1024 * 1024 {
            return Err(AgentError::HistoryLimit);
        }
        let mode = if corrections > 0 {
            RequestMode::Repair
        } else {
            RequestMode::Analysis
        };
        trace.push(
            json!({"kind": "request", "input": staged, "needs_evidence": !has_evidence,
            "answer_only": mode == RequestMode::Repair}),
        );
        let response = respond(&staged, mode)?;
        trace.push(json!({"kind": "response", "output": response}));
        if response["status"] != "completed" {
            return Err(AgentError::IncompleteResponse);
        }
        let output = response["output"]
            .as_array()
            .ok_or(invalid("missing output array"))?;
        let mut text = Vec::new();
        let mut tool_results = Vec::new();
        for item in output {
            match item["type"].as_str() {
                // 原样保留推理项及 encrypted_content，以支持 store=false 的后续请求。
                Some("reasoning") => {}
                Some("function_call") => {
                    if mode == RequestMode::Repair {
                        trace.push(
                            json!({"kind":"validation","error":"回答纠错阶段不允许再次调用工具。"}),
                        );
                        return Err(invalid("tool calls are not allowed during answer repair"));
                    }
                    if item["status"] != "completed" {
                        return Err(invalid("expected completed function call"));
                    }
                    let id = required_string(item, "call_id")?;
                    if !call_ids.insert(id.to_owned()) || tool_results.len() >= 8 {
                        return Err(invalid("duplicate tool call id or too many calls"));
                    }
                    let name = required_string(item, "name")?;
                    let arguments = item["arguments"]
                        .as_str()
                        .ok_or(invalid("missing function arguments"))?;
                    let (result, success) = execute_tool(evidence, name, arguments);
                    comparison::remember(&mut verified, &result);
                    strategy::remember(&mut verified, &result);
                    has_evidence |= success;
                    trace.push(json!({"kind": "tool", "call_id": id, "name": name, "arguments": arguments, "result": result}));
                    tool_results.push(json!({"type": "function_call_output", "call_id": id, "output": result.to_string()}));
                }
                Some("message") => {
                    if item["role"] != "assistant" || item["status"] != "completed" {
                        return Err(invalid("expected completed assistant message"));
                    }
                    let content = item["content"]
                        .as_array()
                        .ok_or(invalid("missing message content"))?;
                    for part in content {
                        match part["type"].as_str() {
                            Some("output_text") => text.push(required_string(part, "text")?),
                            Some("refusal") => return Err(AgentError::Refused),
                            _ => return Err(invalid("unsupported message content")),
                        }
                    }
                }
                _ => return Err(invalid("unsupported output item")),
            }
        }
        staged.extend(output.iter().cloned());
        if tool_results.is_empty() {
            if !has_evidence {
                return Err(AgentError::MissingEvidence);
            }
            let mut text = text.join("\n");
            if text.trim().is_empty() {
                return Err(invalid("no answer or tool call"));
            }
            let repair = output::repair_references(&text, &verified);
            let mut repair_hint = String::new();
            if let Some((repaired, paths)) = repair {
                text = repaired;
                repair_hint = format!(" 引用路径已在本地核对：{paths}，请使用这些完整路径。");
                trace.push(json!({"kind":"reference_repair","repairs":paths}));
            }
            match output::render(&text, &verified) {
                Ok(rendered) => {
                    accepted.extend(output.iter().cloned());
                    check_history(&accepted)?;
                    return Ok((rendered, accepted));
                }
                Err(reason) => {
                    trace.push(json!({"kind": "validation", "error": reason}));
                    if corrections >= 2 || previous_error.as_ref() == Some(&reason) {
                        return Err(invalid(
                            "answer format or evidence references failed validation",
                        ));
                    }
                    corrections += 1;
                    staged.push(json!({"role": "developer", "content": format!("回答校验失败：\n{reason}{repair_hint}\n仅修正这份回答的格式和证据引用，复用已有工具结果。不要调用工具、增加新分析或重写已正确的内容；缺少支持证据的判断应删去或改为说明证据不足。") }));
                    previous_error = Some(reason);
                }
            }
        }
        if !tool_results.is_empty() {
            accepted.extend(output.iter().cloned());
            accepted.extend(tool_results.iter().cloned());
        }
        staged.extend(tool_results);
    }
    Err(AgentError::RequestLimit)
}

fn initial_evidence(evidence: &Value) -> Value {
    json!({"role": "developer", "content": format!(
        "当前固定局面的 review 证据（已提供，无需再调用 get_review）：{}",
        evidence
    )})
}

// 只识别本地生成的快照消息；导入时还会校验其内容和消息边界。
fn context_evidence(item: &Value) -> Option<Value> {
    if item["role"] != "developer" {
        return None;
    }
    let text = item["content"]
        .as_str()?
        .strip_prefix("当前固定局面的 review 证据（已提供，无需再调用 get_review）：")?;
    let evidence = serde_json::from_str(text).ok()?;
    (*item == initial_evidence(&evidence)).then_some(evidence)
}

fn evidence_with_history(evidence: &Value, history: &[Value]) -> Value {
    let mut current = evidence.clone();
    current["comparisons"] = json!({});
    current["analyses"] = json!({});
    let mut past = Vec::new();
    let mut seen_context = false;
    for item in history {
        if let Some(snapshot) = context_evidence(item) {
            if seen_context {
                past.push(current);
            }
            current = snapshot;
            current["comparisons"] = json!({});
            current["analyses"] = json!({});
            seen_context = true;
        } else if item["type"] == "function_call_output"
            && let Some(output) = item["output"].as_str()
            && let Ok(result) = serde_json::from_str::<Value>(output)
        {
            comparison::remember(&mut current, &result);
            strategy::remember(&mut current, &result);
        }
    }
    if !past.is_empty() {
        current["past"] = json!(past);
    }
    current
}

fn check_history(history: &[Value]) -> Result<(), AgentError> {
    if json!(history).to_string().len() > MAX_HISTORY_BYTES {
        Err(AgentError::HistoryLimit)
    } else {
        Ok(())
    }
}

fn invalid(reason: &'static str) -> AgentError {
    AgentError::InvalidResponse { reason }
}

fn required_string<'a>(value: &'a Value, field: &str) -> Result<&'a str, AgentError> {
    value[field]
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .ok_or(invalid("missing or empty required string"))
}

#[cfg(test)]
mod tests;
