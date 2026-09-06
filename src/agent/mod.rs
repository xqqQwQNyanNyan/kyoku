//! 固定单局面的 Responses API 工具调用与中文复盘会话。

use std::{collections::HashSet, error::Error, fmt};

use serde_json::{Value, json};

use crate::review::Review;

mod client;
mod evidence;

pub use evidence::review_evidence;

const INSTRUCTIONS: &str = include_str!("prompt.txt");
const MAX_REQUESTS: usize = 6;
const MAX_HISTORY_BYTES: usize = 1024 * 1024;
const MAX_QUESTION_BYTES: usize = 16 * 1024;

/// Responses API 连接参数。密钥只用于 HTTP 认证，不进入提示词或调试输出。
pub struct AgentConfig<'a> {
    /// 完整的 Responses 地址，例如 `https://api.openai.com/v1/responses`。
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

    /// 发送一次不含牌谱的请求，检查认证、模型和 Responses 工具调用能力。
    /// 此操作可能产生服务商的调用费用，不保存服务端会话。
    pub fn test_connection(&self) -> Result<(), AgentError> {
        let response = client::Client::new(self)?.respond(
            &[json!({"role": "user", "content": "连接测试：请调用 get_review，不必回答。"})],
            true,
        )?;
        if response["status"] != "completed" {
            return Err(AgentError::IncompleteResponse);
        }
        let output = response["output"]
            .as_array()
            .ok_or(invalid("missing output array"))?;
        if output.iter().any(|item| {
            item["type"] == "function_call"
                && item["status"] == "completed"
                && item["name"] == "get_review"
                && item["call_id"].as_str().is_some_and(|id| !id.is_empty())
                && item["arguments"].as_str().is_some_and(|args| {
                    serde_json::from_str::<Value>(args).is_ok_and(|v| v == json!({}))
                })
        }) {
            Ok(())
        } else {
            Err(invalid("service did not return the requested tool call"))
        }
    }
}

/// Agent 调用失败。不会包含密钥、HTTP 响应正文或未经校验的模型输出。
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
            Self::Http { status } => write!(
                f,
                "LLM returned HTTP {status}; check endpoint, credentials, model and quota"
            ),
            Self::InvalidResponse { reason } => {
                write!(f, "invalid Responses API response: {reason}")
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
                "conversation exceeded {MAX_HISTORY_BYTES} bytes; start a new session"
            ),
        }
    }
}

impl Error for AgentError {}

/// 只持有可见复盘证据与内存对话的会话，不持有完整牌谱或 Mortal 进程。
///
/// 调用方先用 `review_at` 生成一次 `Review`。失败的问答不会写入对话历史，
/// 可以重试；已经发送的 HTTP 请求不会被撤销。
pub struct AgentSession {
    client: client::Client,
    evidence: Value,
    history: Vec<Value>,
    has_evidence: bool,
}

impl AgentSession {
    /// 创建会话并校验连接参数；此时不发起 HTTP 请求。
    pub fn new(review: &Review, config: &AgentConfig<'_>) -> Result<Self, AgentError> {
        Ok(Self {
            client: client::Client::new(config)?,
            evidence: review_evidence(review),
            history: Vec::new(),
            has_evidence: false,
        })
    }

    /// 当前局面的工具证据，供命令行展示和人工核对，协议见 `docs/agent/agent.md`。
    pub fn evidence(&self) -> &Value {
        &self.evidence
    }

    /// 提问或追问。首次必须成功取得工具证据，最多请求模型六次。
    pub fn ask(&mut self, question: &str) -> Result<String, AgentError> {
        let client = &self.client;
        let (answer, history) = answer(
            &self.evidence,
            &self.history,
            self.has_evidence,
            question,
            |input, needs_evidence| client.respond(input, needs_evidence),
        )?;
        self.history = history;
        self.has_evidence = true;
        Ok(answer)
    }
}

fn tool_definition() -> Value {
    json!({
        "type": "function", "name": "get_review",
        "description": "读取当前固定局面：自家暗牌、公开信息、切牌向听及进张、Mortal 最终推荐和原始 Q。无对手暗牌或未来事件。输入必须是空对象。",
        "strict": true,
        "parameters": {"type": "object", "properties": {}, "required": [], "additionalProperties": false},
    })
}

fn execute_tool(evidence: &Value, name: &str, arguments: &str) -> (Value, bool) {
    if name != "get_review" {
        return (
            json!({"ok": false, "error": {"code": "unknown_tool", "message": "Only get_review is available."}}),
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

fn answer(
    evidence: &Value,
    history: &[Value],
    mut has_evidence: bool,
    question: &str,
    mut respond: impl FnMut(&[Value], bool) -> Result<Value, AgentError>,
) -> Result<(String, Vec<Value>), AgentError> {
    let question = question.trim();
    if question.is_empty() || question.len() > MAX_QUESTION_BYTES {
        return Err(AgentError::InvalidQuestion);
    }
    let mut staged = history.to_vec();
    let mut call_ids: HashSet<String> = history
        .iter()
        .filter(|item| item["type"] == "function_call")
        .filter_map(|item| item["call_id"].as_str().map(str::to_owned))
        .collect();
    staged.push(json!({"role": "user", "content": question}));
    for _ in 0..MAX_REQUESTS {
        check_history(&staged)?;
        let response = respond(&staged, !has_evidence)?;
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
                    has_evidence |= success;
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
            let text = text.join("\n");
            if text.trim().is_empty() {
                return Err(invalid("no answer or tool call"));
            }
            check_history(&staged)?;
            return Ok((text, staged));
        }
        staged.extend(tool_results);
    }
    Err(AgentError::RequestLimit)
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
