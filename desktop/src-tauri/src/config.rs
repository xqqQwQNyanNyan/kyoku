use crate::UiError;
use kyoku::agent::AgentConfig;
use std::{collections::BTreeMap, path::PathBuf};

pub(crate) fn home() -> PathBuf {
    std::env::var_os("KYOKU_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

pub(crate) struct LlmConfig {
    endpoint: String,
    model: String,
    key: Option<String>,
}

impl LlmConfig {
    pub fn load() -> Result<Self, UiError> {
        let mut values = BTreeMap::new();
        match std::fs::File::open(home().join(".env")) {
            Ok(file) => {
                for entry in dotenvy::from_read_iter(file) {
                    // 解析错误可能包含密钥所在原始行，只报告类别。
                    let (name, value) = entry
                        .map_err(|_| UiError::new("config", ".env 读取或语法错误，请检查配置"))?;
                    values.entry(name).or_insert(value);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(UiError::new("config", "无法读取 .env 文件")),
        }
        Self::from_values(|name| {
            std::env::var(name)
                .ok()
                .or_else(|| values.get(name).cloned())
        })
    }

    fn from_values(mut get: impl FnMut(&str) -> Option<String>) -> Result<Self, UiError> {
        let (endpoint, key) = match get("KYOKU_OPENAI_ENDPOINT") {
            Some(endpoint) => (endpoint, get("AGENT_API_KEY")),
            None => (
                "https://api.openai.com/v1/responses".into(),
                get("OPENAI_API_KEY"),
            ),
        };
        let model = get("OPENAI_MODEL").ok_or_else(|| {
            UiError::new("config", "请在项目 .env 中配置 OPENAI_MODEL 和对应服务密钥")
        })?;
        Ok(Self {
            endpoint,
            model,
            key,
        })
    }

    pub fn borrowed(&self) -> AgentConfig<'_> {
        AgentConfig {
            endpoint: &self.endpoint,
            model: &self.model,
            api_key: self.key.as_deref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_endpoint_never_uses_official_key() {
        let config = LlmConfig::from_values(|name| match name {
            "KYOKU_OPENAI_ENDPOINT" => Some("http://localhost:8080/v1/responses".into()),
            "OPENAI_API_KEY" => Some("official-secret".into()),
            "OPENAI_MODEL" => Some("test-model".into()),
            _ => None,
        })
        .unwrap();
        assert!(config.key.is_none());
    }
}
