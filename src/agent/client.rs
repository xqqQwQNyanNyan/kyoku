use std::time::Duration;

use serde_json::{Value, json};
use ureq::http::{HeaderValue, Uri};

use super::{AgentConfig, AgentError, INSTRUCTIONS, invalid, tool_definition};

pub(super) struct Client {
    agent: ureq::Agent,
    endpoint: String,
    model: String,
    authorization: Option<HeaderValue>,
}

impl Client {
    pub(super) fn new(config: &AgentConfig<'_>) -> Result<Self, AgentError> {
        let bad_config = |field, reason| AgentError::InvalidConfig { field, reason };
        let uri: Uri = config
            .endpoint
            .parse()
            .map_err(|_| bad_config("endpoint", "expected an absolute Responses URL"))?;
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
            .timeout_global(Some(Duration::from_secs(60)))
            .max_redirects(0)
            .http_status_as_error(false);
        if local {
            builder = builder.proxy(None);
        }
        let agent = builder.build().into();
        Ok(Self {
            agent,
            endpoint: config.endpoint.to_owned(),
            model: config.model.to_owned(),
            authorization,
        })
    }

    pub(super) fn respond(
        &self,
        input: &[Value],
        needs_evidence: bool,
    ) -> Result<Value, AgentError> {
        let request = json!({
            "model": self.model, "instructions": INSTRUCTIONS,
            "input": input, "tools": [tool_definition()],
            "tool_choice": if needs_evidence { json!({"type": "function", "name": "get_review"}) } else { json!("auto") },
            "parallel_tool_calls": false, "store": false,
            "include": ["reasoning.encrypted_content"],
            "max_output_tokens": 4096,
        });
        let mut call = self
            .agent
            .post(&self.endpoint)
            .header("Content-Type", "application/json");
        if let Some(authorization) = &self.authorization {
            call = call.header("Authorization", authorization.clone());
        }
        let mut response = call.send(request.to_string()).map_err(transport)?;
        if !response.status().is_success() {
            return Err(AgentError::Http {
                status: response.status().as_u16(),
            });
        }
        let body = response
            .body_mut()
            .with_config()
            .limit(2 * 1024 * 1024)
            .read_to_string()
            .map_err(transport)?;
        serde_json::from_str(&body).map_err(|_| invalid("body is not valid JSON"))
    }
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
