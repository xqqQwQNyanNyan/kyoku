use std::time::{Duration, Instant};

use serde_json::{Value, json};
use ureq::http::{HeaderValue, Uri};

use super::{
    AgentConfig, AgentError, INSTRUCTIONS, QuestionControl, RequestMode, invalid, tool_definitions,
};

mod chat;
mod provider_error;

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
}

impl Client {
    pub(super) fn new(config: &AgentConfig<'_>) -> Result<Self, AgentError> {
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
        })
    }

    fn request(&self, input: &[Value], mode: RequestMode) -> Result<Value, AgentError> {
        Ok(match self.protocol {
            Protocol::ChatCompletions => chat::request(&self.model, input, mode)?,
            Protocol::Responses => json!({
            "model": self.model, "instructions": INSTRUCTIONS,
            "input": input, "tools": available_tools(mode),
            "tool_choice": if mode == RequestMode::Repair { "none" } else { "auto" },
            "parallel_tool_calls": mode == RequestMode::Analysis, "store": false,
            "include": ["reasoning.encrypted_content"],
            "max_output_tokens": 4096,
            }),
        })
    }

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
        // 丢弃请求 future 会关闭本机等待；同步领域 API 仍可在 CLI 和桌面阻塞任务中使用。
        self.runtime.block_on(async {
            tokio::select! {
                biased;
                _ = control.cancelled() => Err(AgentError::Cancelled),
                result = self.respond_async(input, mode) => result,
            }
        })
    }

    async fn respond_async(&self, input: &[Value], mode: RequestMode) -> Result<Value, AgentError> {
        let request = self.request(input, mode)?;
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
        let response =
            serde_json::from_str(&body).map_err(|_| invalid("body is not valid JSON"))?;
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
            })
            .unwrap();
            for mode in [
                RequestMode::ReviewProbe,
                RequestMode::Analysis,
                RequestMode::Repair,
            ] {
                let request = client
                    .request(&[json!({"role":"user","content":"测试"})], mode)
                    .unwrap();
                assert_eq!(
                    request["tool_choice"],
                    if mode == RequestMode::Repair {
                        "none"
                    } else {
                        "auto"
                    }
                );
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
    fn chat_request_is_independent_of_provider_hostname() {
        let build = |endpoint| {
            Client::new(&AgentConfig {
                endpoint,
                model: "test-model",
                api_key: Some("test-only-key"),
            })
            .unwrap()
            .request(
                &[json!({"role":"user","content":"测试"})],
                RequestMode::Analysis,
            )
            .unwrap()
        };
        assert_eq!(
            build("https://api.deepseek.com/chat/completions"),
            build("https://example.com/chat/completions")
        );
    }
}
