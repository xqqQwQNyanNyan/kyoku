use super::{ChatTokenLimit, ModelOptions, QuestionProgress, RequestUsage, Thinking};
use std::{
    cell::RefCell,
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use ureq::http::{HeaderValue, Uri};

use super::{
    AgentConfig, AgentError, INSTRUCTIONS, QuestionControl, RequestMode, invalid, tool_definitions,
};

mod chat;
mod compact;
mod provider_error;

#[cfg(test)]
pub(super) use compact::tests::expand as expand_test;

#[derive(Clone, Copy)]
enum Protocol {
    Responses,
    ChatCompletions,
}

pub(super) struct Client {
    agent: reqwest::Client,
    runtime: tokio::runtime::Runtime,
    endpoint: String,
    model: String,
    authorization: Option<HeaderValue>,
    protocol: Protocol,
    deepseek_chat: bool,
    options: ModelOptions,
    usage: RefCell<Vec<RequestUsage>>,
}

impl Client {
    pub(super) fn new(config: &AgentConfig<'_>) -> Result<Self, AgentError> {
        config.options.validate()?;
        let bad_config = |field, reason| AgentError::InvalidConfig { field, reason };
        let uri: Uri = config
            .endpoint
            .parse()
            .map_err(|_| bad_config("endpoint", "expected an absolute API URL"))?;
        let local = matches!(uri.host(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if uri.host().is_none()
            || uri
                .authority()
                .is_some_and(|authority| authority.as_str().contains('@'))
            || uri.query().is_some()
            || config.endpoint.contains('#')
            || !(uri.scheme_str() == Some("https") || (local && uri.scheme_str() == Some("http")))
        {
            return Err(bad_config(
                "endpoint",
                "use HTTPS (HTTP only on loopback), without userinfo, query or fragment",
            ));
        }
        if config.model.trim().is_empty() {
            return Err(bad_config("model", "must not be empty"));
        }
        let authorization = match config.api_key {
            Some(key) if !key.trim().is_empty() => {
                let mut value = HeaderValue::from_str(&format!("Bearer {key}"))
                    .map_err(|_| bad_config("api_key", "invalid HTTP header characters"))?;
                value.set_sensitive(true);
                Some(value)
            }
            None if local => None,
            _ => {
                return Err(bad_config(
                    "api_key",
                    "provide an API key for the remote endpoint",
                ));
            }
        };
        let mut builder = reqwest::Client::builder()
            // 多工具结果的归纳在实测中会超过一分钟，仍保留单次请求的明确上限。
            .timeout(Duration::from_secs(120))
            .retry(reqwest::retry::never())
            .redirect(reqwest::redirect::Policy::none());
        if local {
            builder = builder.no_proxy();
        }
        let agent = builder.build().map_err(transport)?;
        // 返回到同步调用方后仍需驱动连接清理，避免取消或拒绝超大响应时留下阻塞连接。
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|_| AgentError::Transport { kind: "runtime" })?;
        let protocol = match uri.path().trim_end_matches('/') {
            path if path.ends_with("/chat/completions") || path.ends_with("/chat/completion") => {
                Protocol::ChatCompletions
            }
            _ => Protocol::Responses,
        };
        Ok(Self {
            agent,
            runtime,
            endpoint: config.endpoint.to_owned(),
            model: config.model.to_owned(),
            authorization,
            protocol,
            deepseek_chat: matches!(protocol, Protocol::ChatCompletions)
                && uri
                    .host()
                    .is_some_and(|host| host.eq_ignore_ascii_case("api.deepseek.com")),
            options: config.options.clone(),
            usage: RefCell::new(Vec::new()),
        })
    }

    fn request(&self, input: &[Value], mode: RequestMode) -> Result<Value, AgentError> {
        let mut input = compact::input(input);
        let disabled = self.options.thinking == Thinking::None;
        if disabled {
            input.retain(|item| item["type"] != "reasoning");
        }
        let mut request = match self.protocol {
            Protocol::ChatCompletions => {
                let mut request = chat::request(&self.model, &input, mode)?;
                let object = request
                    .as_object_mut()
                    .ok_or(invalid("invalid chat request"))?;
                object.remove("max_completion_tokens");
                if self.deepseek_chat || self.options.chat_token_limit == ChatTokenLimit::MaxTokens
                {
                    object.insert("max_tokens".into(), json!(self.options.max_output_tokens));
                } else {
                    object.insert(
                        "max_completion_tokens".into(),
                        json!(self.options.max_output_tokens),
                    );
                }
                if self.options.thinking != Thinking::Default {
                    if self.deepseek_chat {
                        object.insert(
                            "thinking".into(),
                            json!({"type": if disabled { "disabled" } else { "enabled" }}),
                        );
                        if !disabled {
                            object.insert("reasoning_effort".into(), json!(self.options.thinking));
                        }
                    } else {
                        object.insert("reasoning_effort".into(), json!(self.options.thinking));
                    }
                }
                if disabled
                    && let Some(messages) = object.get_mut("messages").and_then(Value::as_array_mut)
                {
                    for message in messages {
                        if let Some(fields) = message.as_object_mut() {
                            fields.remove("reasoning_content");
                        }
                    }
                }
                request
            }
            Protocol::Responses => {
                let mut request = json!({
                    "model": self.model, "instructions": INSTRUCTIONS,
                    "input": input, "tools": available_tools(mode), "tool_choice": "auto",
                    "parallel_tool_calls": mode == RequestMode::Analysis, "store": false,
                    "max_output_tokens": self.options.max_output_tokens,
                });
                if self.options.thinking != Thinking::Default {
                    request["reasoning"] = json!({"effort": self.options.thinking});
                }
                if !disabled {
                    request["include"] = json!(["reasoning.encrypted_content"]);
                }
                request
            }
        };
        // 无法为任意模型准确分词。用完整请求的 UTF-8 字节数加封装余量作保守预检，
        // 实际用量只认供应商 usage；既不截断工具消息，也不把估算显示为实耗。
        let estimated_input = request.to_string().len() as u64 + 1024;
        let mut output_limit = self.options.max_output_tokens.get();
        if let Some(budget) = self.options.token_budget {
            let used = self
                .usage
                .borrow()
                .iter()
                .try_fold(0u64, |sum, usage| {
                    usage.total().and_then(|n| sum.checked_add(n))
                })
                .ok_or(AgentError::UnknownUsage)?;
            let remaining = budget.get().saturating_sub(used);
            if remaining <= estimated_input {
                return Err(AgentError::TokenBudget);
            }
            output_limit = output_limit.min(remaining - estimated_input);
        }
        if self
            .options
            .context_tokens
            .is_some_and(|limit| estimated_input + output_limit > limit.get())
        {
            return Err(AgentError::ContextLimit);
        }
        let field = match self.protocol {
            Protocol::Responses => "max_output_tokens",
            Protocol::ChatCompletions
                if self.deepseek_chat
                    || self.options.chat_token_limit == ChatTokenLimit::MaxTokens =>
            {
                "max_tokens"
            }
            Protocol::ChatCompletions => "max_completion_tokens",
        };
        request[field] = json!(output_limit);
        Ok(request)
    }

    pub(super) fn options(&self) -> ModelOptions {
        self.options.clone()
    }

    pub(super) fn reset_usage(&self) {
        self.usage.borrow_mut().clear();
    }

    pub(super) fn usage(&self) -> Vec<RequestUsage> {
        self.usage.borrow().clone()
    }

    #[cfg(test)]
    pub(super) fn respond(&self, input: &[Value], mode: RequestMode) -> Result<Value, AgentError> {
        self.respond_with_control(input, mode, &QuestionControl::default())
    }

    pub(super) fn respond_with_control(
        &self,
        input: &[Value],
        mode: RequestMode,
        control: &QuestionControl,
    ) -> Result<Value, AgentError> {
        control.check()?;
        let request = self.request(input, mode)?;
        self.usage
            .borrow_mut()
            .push(RequestUsage::unknown(self.options.prices.clone()));
        // 丢弃 future 停止本机等待，已经发出的请求仍保留未知用量记录。
        let result = self.runtime.block_on(async {
            tokio::select! {
                biased;
                _ = control.cancelled() => Err(AgentError::Cancelled),
                result = self.respond_async(request) => result,
            }
        });
        let requests = self.usage();
        let _ = control.report(QuestionProgress::Usage {
            requests: requests.clone(),
            budget: self.options.token_budget.map(|v| v.get()),
        });
        let mut response = result?;
        if let Some(budget) = self.options.token_budget {
            let used = requests.iter().try_fold(0u64, |sum, usage| {
                usage.total().and_then(|n| sum.checked_add(n))
            });
            match used {
                Some(used) if used >= budget.get() => response["_budget_exhausted"] = json!(true),
                None => response["_budget_unknown"] = json!(true),
                _ => {}
            }
        }
        Ok(response)
    }

    async fn respond_async(&self, request: Value) -> Result<Value, AgentError> {
        let mut call = self
            .agent
            .post(&self.endpoint)
            .header("Content-Type", "application/json");
        if let Some(authorization) = &self.authorization {
            call = call.header("Authorization", authorization.clone());
        }
        let request = request.to_string();
        let request_bytes = request.len();
        let started = Instant::now();
        let mut response = call.body(request).send().await.map_err(transport)?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let secret = self
                .authorization
                .as_ref()
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "));
            // 只提取有界、脱敏后的结构化错误；读取失败仍保留原始 HTTP 状态。
            let error = read_limited(&mut response, 64 * 1024)
                .await
                .ok()
                .and_then(|body| provider_error::parse(&body, secret));
            return Err(AgentError::Http { status, error });
        }
        let body = read_limited(&mut response, 2 * 1024 * 1024).await?;
        let response: Value =
            serde_json::from_str(&body).map_err(|_| invalid("body is not valid JSON"))?;
        if let Some(usage) = self.usage.borrow_mut().last_mut() {
            *usage = RequestUsage::from_response(&response, self.options.prices.clone());
        }
        let mut response = match self.protocol {
            Protocol::Responses => response,
            Protocol::ChatCompletions => chat::response(response)?,
        };
        // 用量沿用供应商原值；字节数不能当成 token 数，未返回的用量不补零。
        let metrics = json!({
            "request_bytes": request_bytes,
            "response_bytes": body.len(),
            "elapsed_ms": started.elapsed().as_millis() as u64,
        });
        response
            .as_object_mut()
            .ok_or(invalid("response must be an object"))?
            .insert("_metrics".into(), metrics);
        Ok(response)
    }
}

fn available_tools(mode: RequestMode) -> Vec<Value> {
    // 通过可用工具集合约束取证阶段，避免强制 tool_choice 与思考模式冲突。
    tool_definitions()
        .into_iter()
        .filter(|tool| mode != RequestMode::ReviewProbe || tool["name"] == "get_review")
        .collect()
}

async fn read_limited(
    response: &mut reqwest::Response,
    limit: usize,
) -> Result<String, AgentError> {
    let too_large = || AgentError::Transport {
        kind: "response_too_large",
    };
    if response
        .content_length()
        .is_some_and(|size| size > limit as u64)
    {
        return Err(too_large());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport)? {
        if chunk.len() > limit.saturating_sub(body.len()) {
            return Err(too_large());
        }
        body.extend_from_slice(&chunk);
    }
    String::from_utf8(body).map_err(|_| invalid("body is not valid UTF-8"))
}

fn transport(error: reqwest::Error) -> AgentError {
    AgentError::Transport {
        kind: if error.is_timeout() {
            "timeout"
        } else {
            "connection_or_body"
        },
    }
}

pub(super) fn validate_chat_history(message: Value) -> Result<Vec<Value>, AgentError> {
    let finish = if message["tool_calls"]
        .as_array()
        .is_some_and(|c| !c.is_empty())
    {
        "tool_calls"
    } else {
        "stop"
    };
    let normalized =
        chat::response(json!({"choices": [{"finish_reason": finish, "message": message}]}))?;
    normalized["output"]
        .as_array()
        .cloned()
        .ok_or(invalid("missing chat output"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_phase_offers_only_review_without_forcing_a_tool() {
        for endpoint in [
            "https://api.deepseek.com/chat/completions",
            "https://example.com/v1/chat/completions",
            "https://example.com/v1/responses",
        ] {
            let client = Client::new(&AgentConfig {
                endpoint,
                model: "test-model",
                api_key: Some("test-only-key"),
                options: Default::default(),
            })
            .unwrap();
            for mode in [RequestMode::ReviewProbe, RequestMode::Analysis] {
                let request = client
                    .request(&[json!({"role":"user","content":"测试"})], mode)
                    .unwrap();
                assert_eq!(request["tool_choice"], "auto");
                assert_eq!(
                    request["parallel_tool_calls"],
                    mode == RequestMode::Analysis
                );
                let tools = request["tools"].as_array().unwrap();
                if mode == RequestMode::ReviewProbe {
                    assert_eq!(tools.len(), 1);
                    let name = if endpoint.ends_with("/responses") {
                        &tools[0]["name"]
                    } else {
                        &tools[0]["function"]["name"]
                    };
                    assert_eq!(name, "get_review");
                } else {
                    assert_eq!(tools.len(), tool_definitions().len());
                }
                assert!(!request.to_string().contains("test-only-key"));
            }
        }
    }

    #[test]
    fn deepseek_uses_its_documented_output_limit_without_changing_other_chat_fields() {
        let build = |endpoint| {
            Client::new(&AgentConfig {
                endpoint,
                model: "test-model",
                api_key: Some("test-only-key"),
                options: Default::default(),
            })
            .unwrap()
            .request(
                &[json!({"role":"user","content":"测试"})],
                RequestMode::Analysis,
            )
            .unwrap()
        };
        let mut deepseek = build("https://api.deepseek.com/chat/completions");
        let generic = build("https://example.com/chat/completions");
        assert_eq!(deepseek["max_tokens"], 4096);
        assert!(deepseek.get("max_completion_tokens").is_none());
        assert!(generic.get("max_tokens").is_none());
        deepseek.as_object_mut().unwrap().remove("max_tokens");
        deepseek["max_completion_tokens"] = json!(4096);
        assert_eq!(deepseek, generic);
        assert_eq!(
            build("https://api.deepseek.com.example.com/chat/completions"),
            generic
        );
        let request = Client::new(&AgentConfig {
            endpoint: "https://api.deepseek.com/chat/completions",
            model: "deepseek-v4-flash",
            api_key: Some("test-only-key"),
            options: Default::default(),
        })
        .unwrap()
        .request(
            &[json!({"role":"user","content":"测试"})],
            RequestMode::Analysis,
        )
        .unwrap();
        assert!(request.get("reasoning_effort").is_none());
        assert!(request.get("response_format").is_none());
        assert!(request.get("thinking").is_none());
        assert_eq!(request["max_tokens"], 4096);
        let response_request = Client::new(&AgentConfig {
            endpoint: "https://example.com/v1/responses",
            model: "deepseek-v4-flash",
            api_key: Some("test-only-key"),
            options: Default::default(),
        })
        .unwrap()
        .request(
            &[json!({"role":"user","content":"测试"})],
            RequestMode::Analysis,
        )
        .unwrap();
        assert_eq!(response_request["max_output_tokens"], 4096);
        assert!(response_request.get("reasoning").is_none());
        assert!(response_request.get("text").is_none());
    }

    #[test]
    fn deepseek_fast_requests_do_not_resend_reasoning_or_change_stored_history() {
        let response = chat::response(json!({"choices":[{"finish_reason":"tool_calls","message":{
            "role":"assistant","content":null,"reasoning_content":"历史思考保存在本地",
            "tool_calls":[{"id":"call_1","type":"function","function":{"name":"get_review","arguments":"{}"}}]
        }}]})).unwrap();
        let mut input = vec![json!({"role":"user","content":"比较这两张牌"})];
        input.extend(response["output"].as_array().unwrap().iter().cloned());
        input.push(
            json!({"type":"function_call_output","call_id":"call_1","output":"{\"ok\":true}"}),
        );
        let original = input.clone();
        for model in ["deepseek-v4-flash", "deepseek-v4-pro"] {
            let client = Client::new(&AgentConfig {
                endpoint: "https://api.deepseek.com/chat/completions",
                model,
                api_key: Some("test-only-key"),
                options: ModelOptions {
                    thinking: Thinking::None,
                    ..Default::default()
                },
            })
            .unwrap();
            let request = client.request(&input, RequestMode::Analysis).unwrap();
            assert_eq!(request["thinking"]["type"], "disabled");
            assert_eq!(request["max_tokens"], 4096);
            assert!(
                request["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|m| m.get("reasoning_content").is_none())
            );
            assert_eq!(request["messages"][2]["tool_calls"][0]["id"], "call_1");
            assert_eq!(input, original);

            let client = Client::new(&AgentConfig {
                endpoint: "https://api.deepseek.com/v1/responses",
                model,
                api_key: Some("test-only-key"),
                options: ModelOptions {
                    thinking: Thinking::None,
                    ..Default::default()
                },
            })
            .unwrap();
            let request = client
                .request(
                    &[
                        json!({"type":"reasoning","encrypted_content":"old-reasoning"}),
                        json!({"role":"user","content":"提问"}),
                    ],
                    RequestMode::Analysis,
                )
                .unwrap();
            assert_eq!(request["reasoning"]["effort"], "none");
            assert_eq!(request["max_output_tokens"], 4096);
            assert!(request.get("include").is_none());
            assert_eq!(request["input"].as_array().unwrap().len(), 1);
        }
    }

    #[test]
    fn custom_limits_and_optional_thinking_work_for_unrecognized_models() {
        for (endpoint, field) in [
            ("https://example.com/responses", "max_output_tokens"),
            (
                "https://example.com/chat/completions",
                "max_completion_tokens",
            ),
            ("https://api.deepseek.com/chat/completions", "max_tokens"),
        ] {
            for thinking in [Thinking::Default, Thinking::High, Thinking::None] {
                let client = Client::new(&AgentConfig {
                    endpoint,
                    model: "new-model-with-no-hardcoded-rules",
                    api_key: Some("test-key"),
                    options: ModelOptions {
                        max_output_tokens: std::num::NonZeroU64::new(16384).unwrap(),
                        thinking,
                        ..Default::default()
                    },
                })
                .unwrap();
                let request = client
                    .request(
                        &[json!({"role":"user","content":"hello"})],
                        RequestMode::Analysis,
                    )
                    .unwrap();
                assert_eq!(request[field], 16384);
                if thinking == Thinking::Default {
                    assert!(request.get("reasoning").is_none());
                    assert!(request.get("reasoning_effort").is_none());
                    assert!(request.get("thinking").is_none());
                } else if endpoint.contains("api.deepseek.com") {
                    assert_eq!(
                        request["thinking"]["type"],
                        if thinking == Thinking::None {
                            "disabled"
                        } else {
                            "enabled"
                        }
                    );
                    if thinking == Thinking::None {
                        assert!(request.get("reasoning_effort").is_none());
                    }
                } else if endpoint.ends_with("responses") {
                    assert_eq!(request["reasoning"]["effort"], json!(thinking));
                } else {
                    assert_eq!(request["reasoning_effort"], json!(thinking));
                }
            }
        }
    }

    #[test]
    fn remaining_budget_reduces_the_wire_output_limit_and_legacy_chat_is_selectable() {
        let mut config = AgentConfig {
            endpoint: "http://localhost/chat/completions",
            model: "legacy",
            api_key: None,
            options: ModelOptions {
                chat_token_limit: ChatTokenLimit::MaxTokens,
                ..Default::default()
            },
        };
        let input = [json!({"role":"user","content":"测试"})];
        let original = Client::new(&config)
            .unwrap()
            .request(&input, RequestMode::Analysis)
            .unwrap();
        assert!(original.get("max_completion_tokens").is_none());
        let estimated = original.to_string().len() as u64 + 1024;
        config.options.token_budget = std::num::NonZeroU64::new(estimated + 100);
        let request = Client::new(&config)
            .unwrap()
            .request(&input, RequestMode::Analysis)
            .unwrap();
        assert_eq!(request["max_tokens"], 100);
    }

    #[test]
    #[ignore = "只构造本地保存轨迹的请求，测量字节数；不调用模型"]
    fn measure_saved_requests() {
        let files: Vec<String> =
            serde_json::from_str(&std::env::var("KYOKU_REQUEST_INPUTS").unwrap()).unwrap();
        let old_prompt =
            std::fs::read_to_string(std::env::var("KYOKU_PREVIOUS_PROMPT").unwrap()).unwrap();
        let client = Client::new(&AgentConfig {
            endpoint: "https://api.deepseek.com/chat/completions",
            model: "deepseek-v4-flash",
            api_key: Some("test-only-key"),
            options: Default::default(),
        })
        .unwrap();
        for file in files {
            let saved: Value =
                serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
            for (index, step) in saved["trace"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|step| step["kind"] == "request")
                .enumerate()
            {
                let input = step["input"].as_array().unwrap();
                let mut before =
                    chat::request("deepseek-v4-flash", input, RequestMode::Analysis).unwrap();
                before["messages"][0]["content"] = json!(old_prompt);
                let started = Instant::now();
                let after = client.request(input, RequestMode::Analysis).unwrap();
                let micros = started.elapsed().as_micros();
                println!(
                    "{}",
                    json!({"source":file,"request":index,"before_bytes":before.to_string().len(),"after_bytes":after.to_string().len(),"elapsed_us":micros})
                );
            }
        }
    }
}
