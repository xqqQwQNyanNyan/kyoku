use crate::{UiError, lock};
use kyoku::agent::{AgentConfig, ModelOptions, QuestionControl};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Mutex,
};

const DEFAULT_ENDPOINT: &str = "https://api.openai.com/v1/responses";

#[derive(Deserialize)]
pub(crate) struct SettingsInput {
    endpoint: String,
    model: String,
    /// 留空保留相同地址的现有密钥；换地址必须重新填写。
    api_key: String,
    clear_key: bool,
    #[serde(default)]
    options: ModelOptions,
}

#[derive(Serialize)]
pub(crate) struct SettingsView {
    endpoint: String,
    model: String,
    has_api_key: bool,
    saved: bool,
    options: ModelOptions,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SavedSettings {
    endpoint: String,
    model: String,
    api_key: Option<String>,
    #[serde(default)]
    options: ModelOptions,
    // 兼容旧设置，但不再访问钥匙串；用户重新填写密钥后保存为新格式。
    #[serde(rename = "credential", skip_serializing)]
    _legacy_credential: Option<String>,
}

pub(crate) struct LlmConfig {
    endpoint: String,
    model: String,
    key: Option<String>,
    options: ModelOptions,
}

impl LlmConfig {
    pub fn borrowed(&self) -> AgentConfig<'_> {
        AgentConfig {
            endpoint: &self.endpoint,
            model: &self.model,
            api_key: self.key.as_deref(),
            options: self.options.clone(),
        }
    }

    fn from_values(mut get: impl FnMut(&str) -> Option<String>) -> Self {
        let (endpoint, key) = match get("KYOKU_OPENAI_ENDPOINT") {
            Some(endpoint) => (endpoint, get("AGENT_API_KEY")),
            None => (DEFAULT_ENDPOINT.into(), get("OPENAI_API_KEY")),
        };
        Self {
            endpoint,
            key,
            model: get("OPENAI_MODEL").unwrap_or_default(),
            options: ModelOptions::default(),
        }
    }
}

pub(crate) struct SettingsStore {
    directory: PathBuf,
    development: Option<PathBuf>,
    gate: Mutex<()>,
}

impl SettingsStore {
    pub fn new(directory: PathBuf, development: Option<PathBuf>) -> Self {
        Self {
            directory,
            development,
            gate: Mutex::new(()),
        }
    }

    fn read(&self) -> Result<Option<SavedSettings>, UiError> {
        match fs::read(self.directory.join("settings.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|_| {
                UiError::new("config", "设置文件损坏，请移走 settings.json 后重新配置")
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(UiError::new("config", "无法读取设置文件")),
        }
    }

    fn legacy(&self) -> Result<LlmConfig, UiError> {
        let mut values = BTreeMap::new();
        let Some(home) = &self.development else {
            return Ok(LlmConfig::from_values(|_| None));
        };
        match fs::File::open(home.join(".env")) {
            Ok(file) => {
                for entry in dotenvy::from_read_iter(file) {
                    let (name, value) = entry
                        .map_err(|_| UiError::new("config", ".env 读取或语法错误，请检查配置"))?;
                    values.entry(name).or_insert(value);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(UiError::new("config", "无法读取 .env 文件")),
        }
        Ok(LlmConfig::from_values(|name| {
            std::env::var(name)
                .ok()
                .or_else(|| values.get(name).cloned())
        }))
    }

    pub fn view(&self) -> Result<SettingsView, UiError> {
        let _guard = lock(&self.gate)?;
        self.view_unlocked()
    }

    fn view_unlocked(&self) -> Result<SettingsView, UiError> {
        if let Some(saved) = self.read()? {
            Ok(SettingsView {
                endpoint: saved.endpoint,
                model: saved.model,
                has_api_key: saved.api_key.is_some(),
                saved: true,
                options: saved.options,
            })
        } else {
            let config = self.legacy()?;
            Ok(SettingsView {
                endpoint: config.endpoint,
                model: config.model,
                has_api_key: config.key.is_some(),
                saved: false,
                options: config.options,
            })
        }
    }

    pub fn load(&self) -> Result<LlmConfig, UiError> {
        let _guard = lock(&self.gate)?;
        if let Some(saved) = self.read()? {
            Ok(LlmConfig {
                key: saved.api_key,
                options: saved.options,
                endpoint: saved.endpoint,
                model: saved.model,
            })
        } else {
            self.legacy()
        }
    }

    fn draft(&self, input: SettingsInput) -> Result<LlmConfig, UiError> {
        let endpoint = input.endpoint.trim().to_owned();
        let model = input.model.trim().to_owned();
        let key = if input.clear_key {
            None
        } else if !input.api_key.trim().is_empty() {
            Some(input.api_key.trim().to_owned())
        } else if let Some(saved) = self.read()? {
            if saved.endpoint == endpoint {
                saved.api_key
            } else {
                None
            }
        } else {
            let legacy = self.legacy()?;
            if legacy.endpoint == endpoint {
                legacy.key
            } else {
                None
            }
        };
        let config = LlmConfig {
            endpoint,
            model,
            key,
            options: input.options,
        };
        let mut validation = config.borrowed();
        // 允许用户主动删除远程密钥；实际发请求时仍要求有效认证。
        if input.clear_key {
            validation.api_key = Some("validation-only");
        }
        validation
            .validate()
            .map_err(|error| UiError::new("config", error.to_string()))?;
        Ok(config)
    }

    pub fn test(&self, input: SettingsInput, control: &QuestionControl) -> Result<(), UiError> {
        let config = {
            let _guard = lock(&self.gate)?;
            self.draft(input)?
        };
        config
            .borrowed()
            .test_connection_with_control(control)
            .map_err(|error| UiError::new("connection", error.to_string()))
    }

    pub fn save(&self, input: SettingsInput) -> Result<SettingsView, UiError> {
        let _guard = lock(&self.gate)?;
        let config = self.draft(input)?;
        fs::create_dir_all(&self.directory)
            .map_err(|_| UiError::new("config", "无法创建设置目录"))?;
        let saved = SavedSettings {
            endpoint: config.endpoint,
            model: config.model,
            api_key: config.key,
            options: config.options,
            _legacy_credential: None,
        };
        let bytes = serde_json::to_vec_pretty(&saved)
            .map_err(|_| UiError::new("config", "无法编码设置"))?;
        let temporary = self.directory.join("settings.json.tmp");
        // 清理上次意外退出留下的文件；新建时设置权限，避免密钥短暂暴露。
        match fs::remove_file(&temporary) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(UiError::new("config", "无法清理临时设置文件")),
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|_| UiError::new("config", "无法创建临时设置文件"))?;
        let write_result = file.write_all(&bytes).and_then(|_| file.sync_all());
        // Windows 上替换文件前也先关闭句柄。
        drop(file);
        let result = write_result
            .map_err(|_| UiError::new("config", "无法写入设置"))
            .and_then(|_| {
                // 完整写入后再替换，保存失败时保留原配置和密钥。
                fs::rename(&temporary, self.directory.join("settings.json"))
                    .map_err(|_| UiError::new("config", "无法保存设置"))
            });
        if let Err(error) = result {
            let _ = fs::remove_file(temporary);
            return Err(error);
        }
        self.view_unlocked()
    }
}

#[cfg(test)]
mod tests;
