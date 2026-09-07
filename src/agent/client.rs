use std::time::Duration;

use serde_json::{Value, json};
use ureq::http::{HeaderValue, Uri};

use super::{AgentConfig, AgentError, INSTRUCTIONS, invalid, tool_definitions};

mod chat;
mod provider_error;

#[derive(Clone, Copy)]
enum Protocol {
    Responses,
    ChatCompletions,
}

pub(super) struct Client {
    agent: ureq::Agent,
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
        let mut builder = ureq::Agent::config_builder()
            // 多工具结果的归纳在实测中会超过一分钟，仍保留单次请求的明确上限。
            .timeout_global(Some(Duration::from_secs(120)))
            .max_redirects(0)
            .http_status_as_error(false);
        if local {
            builder = builder.proxy(None);
        }
        let agent = builder.build().into();
        let protocol = match uri.path().trim_end_matches('/') {
            path if path.ends_with("/chat/completions") || path.ends_with("/chat/completion") => {
                Protocol::ChatCompletions
            }
            _ => Protocol::Responses,
        };
        Ok(Self {
            agent,
            endpoint: config.endpoint.to_owned(),
            model: config.model.to_owned(),
            authorization,
            protocol,
        })
    }

    fn request(&self, input: &[Value], needs_evidence: bool) -> Result<Value, AgentError> {
        Ok(match self.protocol {
            Protocol::ChatCompletions => chat::request(&self.model, input, needs_evidence)?,
            Protocol::Responses => json!({
            "model": self.model, "instructions": INSTRUCTIONS,
            "input": input, "tools": available_tools(needs_evidence),
            "tool_choice": "auto",
            "parallel_tool_calls": false, "store": false,
            "include": ["reasoning.encrypted_content"],
            "max_output_tokens": 4096,
            }),
        })
    }

    pub(super) fn respond(
        &self,
        input: &[Value],
        needs_evidence: bool,
    ) -> Result<Value, AgentError> {
        let request = self.request(input, needs_evidence)?;
        let mut call = self
            .agent
            .post(&self.endpoint)
            .header("Content-Type", "application/json");
        if let Some(authorization) = &self.authorization {
            call = call.header("Authorization", authorization.clone());
        }
        let mut response = call.send(request.to_string()).map_err(transport)?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let secret = self
                .authorization
                .as_ref()
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "));
            // 只提取有界、脱敏后的结构化错误；读取失败仍保留原始 HTTP 状态。
            let error = response
                .body_mut()
                .with_config()
                .limit(64 * 1024)
                .read_to_string()
                .ok()
                .and_then(|body| provider_error::parse(&body, secret));
            return Err(AgentError::Http { status, error });
        }
        let body = response
            .body_mut()
            .with_config()
            .limit(2 * 1024 * 1024)
            .read_to_string()
            .map_err(transport)?;
        let response =
            serde_json::from_str(&body).map_err(|_| invalid("body is not valid JSON"))?;
        match self.protocol {
            Protocol::Responses => Ok(response),
            Protocol::ChatCompletions => chat::response(response),
        }
    }
}

fn available_tools(needs_evidence: bool) -> Vec<Value> {
    // 通过可用工具集合约束取证阶段，避免强制 tool_choice 与思考模式冲突。
    tool_definitions()
        .into_iter()
        .filter(|tool| !needs_evidence || tool["name"] == "get_review")
        .collect()
}

fn transport(error: ureq::Error) -> AgentError {
    AgentError::Transport {
        kind: match error {
            ureq::Error::Timeout(_) => "timeout",
            ureq::Error::BodyExceedsLimit(_) => "response_too_large",
            _ => "connection_or_body",
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
            for needs_evidence in [true, false] {
                let request = client
                    .request(&[json!({"role":"user","content":"测试"})], needs_evidence)
                    .unwrap();
                assert_eq!(request["tool_choice"], "auto");
                let tools = request["tools"].as_array().unwrap();
                if needs_evidence {
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
            .request(&[json!({"role":"user","content":"测试"})], false)
            .unwrap()
        };
        assert_eq!(
            build("https://api.deepseek.com/chat/completions"),
            build("https://example.com/chat/completions")
        );
    }
}
