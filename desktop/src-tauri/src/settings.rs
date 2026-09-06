use crate::{UiError, lock};
use kyoku::agent::AgentConfig;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::PathBuf, sync::Mutex};

const DEFAULT_ENDPOINT: &str = "https://api.openai.com/v1/responses";
#[cfg(target_os = "macos")]
const SERVICE: &str = "dev.kyoku.desktop.llm";

#[derive(Deserialize)]
pub(crate) struct SettingsInput {
    endpoint: String,
    model: String,
    /// 留空保留相同地址的现有密钥；换地址必须重新填写。
    api_key: String,
    clear_key: bool,
}

#[derive(Serialize)]
pub(crate) struct SettingsView {
    endpoint: String,
    model: String,
    has_api_key: bool,
    saved: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SavedSettings {
    endpoint: String,
    model: String,
    credential: Option<String>,
}

pub(crate) struct LlmConfig {
    endpoint: String,
    model: String,
    key: Option<String>,
}

impl LlmConfig {
    pub fn borrowed(&self) -> AgentConfig<'_> {
        AgentConfig {
            endpoint: &self.endpoint,
            model: &self.model,
            api_key: self.key.as_deref(),
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
                has_api_key: saved.credential.is_some(),
                saved: true,
            })
        } else {
            let config = self.legacy()?;
            Ok(SettingsView {
                endpoint: config.endpoint,
                model: config.model,
                has_api_key: config.key.is_some(),
                saved: false,
            })
        }
    }

    pub fn load(&self) -> Result<LlmConfig, UiError> {
        let _guard = lock(&self.gate)?;
        if let Some(saved) = self.read()? {
            Ok(LlmConfig {
                key: saved
                    .credential
                    .as_deref()
                    .map(|id| NativeSecrets.get(id))
                    .transpose()?,
                endpoint: saved.endpoint,
                model: saved.model,
            })
        } else {
            self.legacy()
        }
    }

    fn draft(&self, input: SettingsInput, secrets: &impl Secrets) -> Result<LlmConfig, UiError> {
        let endpoint = input.endpoint.trim().to_owned();
        let model = input.model.trim().to_owned();
        let key = if input.clear_key {
            None
        } else if !input.api_key.trim().is_empty() {
            Some(input.api_key.trim().to_owned())
        } else if let Some(saved) = self.read()? {
            if saved.endpoint == endpoint {
                saved
                    .credential
                    .as_deref()
                    .map(|id| secrets.get(id))
                    .transpose()?
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

    pub fn test(&self, input: SettingsInput) -> Result<(), UiError> {
        let config = {
            let _guard = lock(&self.gate)?;
            self.draft(input, &NativeSecrets)?
        };
        config
            .borrowed()
            .test_connection()
            .map_err(|error| UiError::new("connection", error.to_string()))
    }

    pub fn save(&self, input: SettingsInput) -> Result<SettingsView, UiError> {
        let _guard = lock(&self.gate)?;
        self.save_with(input, &NativeSecrets)
    }

    fn save_with(
        &self,
        input: SettingsInput,
        secrets: &impl Secrets,
    ) -> Result<SettingsView, UiError> {
        let config = self.draft(input, secrets)?;
        let previous = self.read()?;
        fs::create_dir_all(&self.directory)
            .map_err(|_| UiError::new("config", "无法创建设置目录"))?;
        // 新密钥和新文件先准备好，再切换配置；失败不会破坏正在使用的旧密钥。
        let credential = config.key.as_ref().map(|_| credential_id()).transpose()?;
        if let (Some(id), Some(key)) = (&credential, &config.key) {
            secrets.set(id, key)?;
        }
        let saved = SavedSettings {
            endpoint: config.endpoint,
            model: config.model,
            credential,
        };
        let temporary = self.directory.join("settings.json.tmp");
        let write_result = (|| {
            let bytes = serde_json::to_vec_pretty(&saved)
                .map_err(|_| UiError::new("config", "无法编码设置"))?;
            fs::write(&temporary, bytes).map_err(|_| UiError::new("config", "无法写入设置"))?;
            fs::rename(&temporary, self.directory.join("settings.json"))
                .map_err(|_| UiError::new("config", "无法保存设置"))
        })();
        if let Err(error) = write_result {
            if let Some(id) = &saved.credential {
                let _ = secrets.delete(id);
            }
            let _ = fs::remove_file(temporary);
            return Err(error);
        }
        if let Some(id) = previous.and_then(|saved| saved.credential) {
            let _ = secrets.delete(&id);
        }
        self.view_unlocked()
    }
}

fn credential_id() -> Result<String, UiError> {
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| UiError::new("config", "系统时间异常，无法保存密钥"))?
        .as_nanos();
    Ok(format!(
        "{}-{time}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
}

trait Secrets {
    fn get(&self, id: &str) -> Result<String, UiError>;
    fn set(&self, id: &str, key: &str) -> Result<(), UiError>;
    fn delete(&self, id: &str) -> Result<(), UiError>;
}

struct NativeSecrets;

#[cfg(target_os = "macos")]
impl Secrets for NativeSecrets {
    fn get(&self, id: &str) -> Result<String, UiError> {
        use security_framework::passwords::{PasswordOptions, generic_password};
        let bytes =
            generic_password(PasswordOptions::new_generic_password(SERVICE, id)).map_err(|_| {
                UiError::new(
                    "keychain",
                    "无法读取钥匙串密钥，请解锁钥匙串或重新填写 API Key",
                )
            })?;
        String::from_utf8(bytes)
            .map_err(|_| UiError::new("keychain", "钥匙串密钥格式异常，请重新填写 API Key"))
    }
    fn set(&self, id: &str, key: &str) -> Result<(), UiError> {
        security_framework::passwords::set_generic_password(SERVICE, id, key.as_bytes()).map_err(
            |_| {
                UiError::new(
                    "keychain",
                    "无法保存 API Key，请允许 Kyoku 访问钥匙串后重试",
                )
            },
        )
    }
    fn delete(&self, id: &str) -> Result<(), UiError> {
        security_framework::passwords::delete_generic_password(SERVICE, id)
            .map_err(|_| UiError::new("keychain", "无法删除旧钥匙串条目"))
    }
}

#[cfg(not(target_os = "macos"))]
impl Secrets for NativeSecrets {
    fn get(&self, _: &str) -> Result<String, UiError> {
        Err(UiError::new("keychain", "设置页的密钥存储目前仅支持 macOS"))
    }
    fn set(&self, _: &str, _: &str) -> Result<(), UiError> {
        Err(UiError::new("keychain", "设置页的密钥存储目前仅支持 macOS"))
    }
    fn delete(&self, _: &str) -> Result<(), UiError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
