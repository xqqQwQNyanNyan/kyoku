use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::Manager;
use url::Url;

use crate::{UiError, config::development_home, replay::MAX_LOG_BYTES};

const RESPONSE_LIMIT: u64 = MAX_LOG_BYTES as u64 + 1024;
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(45);
const LOGIN_COOLDOWN: Duration = Duration::from_secs(30);

// 凭据不实现 Debug，不落盘，也不通过环境变量或命令行参数传给子进程。
#[derive(Deserialize, Serialize)]
pub(crate) struct Credentials {
    username: String,
    password: String,
    accept_risk: bool,
}

impl Credentials {
    fn validate(&self) -> Result<(), UiError> {
        if !self.accept_risk {
            return Err(service_error("risk_not_accepted"));
        }
        if self.username.trim().is_empty()
            || self.username.len() > 256
            || self.password.is_empty()
            || self.password.len() > 1024
        {
            return Err(service_error("invalid_credentials"));
        }
        Ok(())
    }
}

pub(crate) struct Paths {
    node: PathBuf,
    script: PathBuf,
}

impl Paths {
    pub(crate) fn resolve(app: &tauri::AppHandle) -> Result<Self, UiError> {
        let resources = app
            .path()
            .resource_dir()
            .map_err(|_| UiError::new("majsoul_runtime", "无法定位雀魂下载组件"))?;
        Ok(Self::from_roots(
            &resources,
            development_home().as_deref(),
            cfg!(target_os = "windows"),
        ))
    }

    fn from_roots(resources: &Path, development: Option<&Path>, windows: bool) -> Self {
        let root = resources.join("majsoul");
        if !root.exists()
            && let Some(home) = development
        {
            return Self {
                node: "node".into(),
                script: home.join("services/majsoul/desktop.cjs"),
            };
        }
        Self {
            node: root.join(if windows {
                "node/node.exe"
            } else {
                "node/bin/node"
            }),
            script: root.join("service/desktop.cjs"),
        }
    }
}

#[derive(Default)]
pub(crate) struct Account {
    session: Option<Session>,
    next_login: Option<Instant>,
}

impl Account {
    pub(crate) fn logged_in(&mut self) -> bool {
        let alive = self
            .session
            .as_mut()
            .is_some_and(|session| matches!(session.child.try_wait(), Ok(None)));
        if !alive {
            self.session = None;
        }
        alive
    }

    pub(crate) fn login(&mut self, paths: &Paths, credentials: Credentials) -> Result<(), UiError> {
        credentials.validate()?;
        if self.logged_in() {
            return Err(UiError::new(
                "majsoul_logged_in",
                "请先退出当前雀魂账号再切换账号",
            ));
        }
        if self.next_login.is_some_and(|next| Instant::now() < next) {
            return Err(UiError::new(
                "majsoul_login_limited",
                "登录尝试过于频繁，请在上次尝试 30 秒后重试",
            ));
        }
        if !paths.script.is_file() {
            return Err(runtime_error());
        }
        let mut command = Command::new(&paths.node);
        command
            .arg(&paths.script)
            // 不继承 NODE_OPTIONS、服务 .env 凭据等；仅保留启动运行库所需的系统路径。
            .env_clear();
        for name in runtime_environment_names() {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        let mut session = Session::spawn(command)?;
        self.next_login = Some(Instant::now() + LOGIN_COOLDOWN);
        let response = session.request(&json!({
            "action": "login", "username": credentials.username,
            "password": credentials.password, "accept_risk": credentials.accept_risk,
        }))?;
        if let Some(code) = response.error {
            return Err(service_error(&code));
        }
        if response.logged_in != Some(true) || response.log.is_some() {
            return Err(connection_error());
        }
        self.session = Some(session);
        Ok(())
    }

    pub(crate) fn logout(&mut self) {
        self.session = None;
    }

    pub(crate) fn download(&mut self, url: &Url) -> Result<String, UiError> {
        let id = super::paipu_id(url)?;
        if !self.logged_in() {
            return Err(service_error("login_required"));
        }
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| service_error("login_required"))?;
        let response = match session.request(&json!({ "action": "download", "id": id })) {
            Ok(response) => response,
            Err(error) => {
                self.session = None;
                return Err(error);
            }
        };
        if let Some(code) = response.error {
            return Err(service_error(&code));
        }
        if response.logged_in.is_some() {
            self.session = None;
            return Err(connection_error());
        }
        let Some(log) = response.log.filter(Value::is_object) else {
            self.session = None;
            return Err(connection_error());
        };
        let json = serde_json::to_string(&log).map_err(|_| connection_error())?;
        if json.len() > MAX_LOG_BYTES {
            return Err(service_error("log_too_large"));
        }
        Ok(json)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    error: Option<String>,
    logged_in: Option<bool>,
    log: Option<Value>,
}

struct Session {
    child: Child,
    stdin: Option<ChildStdin>,
    output: Receiver<Result<String, UiError>>,
}

impl Session {
    fn spawn(mut command: Command) -> Result<Self, UiError> {
        hide_console_window(&mut command);
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| runtime_error())?;
        let stdin = child.stdin.take();
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(connection_error());
        };
        let (sender, output) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                let message = match reader
                    .by_ref()
                    .take(RESPONSE_LIMIT + 1)
                    .read_line(&mut line)
                {
                    Ok(0) => break,
                    Ok(_) if line.len() as u64 > RESPONSE_LIMIT => {
                        Err(service_error("log_too_large"))
                    }
                    Ok(_) if line.ends_with('\n') => Ok(line),
                    _ => Err(connection_error()),
                };
                let failed = message.is_err();
                if sender.send(message).is_err() || failed {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            output,
        })
    }

    fn request(&mut self, input: &Value) -> Result<Response, UiError> {
        let stdin = self.stdin.as_mut().ok_or_else(connection_error)?;
        serde_json::to_writer(&mut *stdin, input).map_err(|_| connection_error())?;
        stdin
            .write_all(b"\n")
            .and_then(|()| stdin.flush())
            .map_err(|_| connection_error())?;
        self.read_response(RESPONSE_TIMEOUT)
    }

    fn read_response(&self, timeout: Duration) -> Result<Response, UiError> {
        let line = match self.output.recv_timeout(timeout) {
            Ok(line) => line?,
            Err(RecvTimeoutError::Timeout) => {
                return Err(UiError::new(
                    "majsoul_timeout",
                    "连接或下载雀魂牌谱超时，会话已结束，请重新登录后重试",
                ));
            }
            Err(RecvTimeoutError::Disconnected) => return Err(connection_error()),
        };
        serde_json::from_str(&line).map_err(|_| connection_error())
    }
}

fn runtime_environment_names() -> &'static [&'static str] {
    if cfg!(target_os = "windows") {
        &["PATH", "SystemRoot", "WINDIR", "TEMP", "TMP", "USERPROFILE"]
    } else {
        &["PATH"]
    }
}

#[cfg(target_os = "windows")]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn hide_console_window(_: &mut Command) {}

impl Drop for Session {
    fn drop(&mut self) {
        self.stdin.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn runtime_error() -> UiError {
    UiError::new(
        "majsoul_runtime",
        if cfg!(debug_assertions) {
            "雀魂下载组件不可用，请确认已安装 Node.js 22.12+ 并运行 npm --prefix services/majsoul ci"
        } else {
            "雀魂下载组件缺失或无法启动，请重新安装包含下载组件的完整安装包"
        },
    )
}

fn connection_error() -> UiError {
    UiError::new(
        "majsoul_connection",
        "雀魂连接已断开，请重新登录；如另一客户端正在使用该账号，请先退出",
    )
}

fn service_error(code: &str) -> UiError {
    // 上游任意文本、账号和 token 均不能进入 UI、日志或 Agent 会话。
    match code {
        "risk_not_accepted" => UiError::new("majsoul_risk", "请先阅读并确认雀魂账号使用风险"),
        "invalid_credentials" => UiError::new("majsoul_credentials", "请输入有效的雀魂账号和密码"),
        "login_required" => UiError::new("majsoul_login_required", "请先登录雀魂账号"),
        "login_failed" => UiError::new(
            "majsoul_login",
            "雀魂登录失败，请检查国际中文服账号、密码和网络；如需安全验证，请先在官方客户端完成。暂不支持第三方账号或短信登录",
        ),
        "client_outdated" => UiError::new(
            "majsoul_version",
            "雀魂接口或资源版本已变化，请更新 Kyoku 后重试",
        ),
        "invalid_id" => UiError::new("invalid_link", "雀魂牌谱编号无效，请重新复制完整分享链接"),
        "busy" | "rate_limited" => {
            UiError::new("majsoul_rate_limited", "下载过于频繁，请稍等几秒后重试")
        }
        "unsupported_rules" | "unsupported_record" => UiError::new(
            "majsoul_unsupported",
            "暂不支持这份雀魂牌谱的规则或记录格式；目前支持普通四人段位场",
        ),
        "log_too_large" => UiError::new("log_too_large", "牌谱文件不能超过 16 MiB"),
        _ => UiError::new(
            "majsoul_download",
            "雀魂牌谱下载失败，请确认链接有效且账号可访问该牌谱，稍后重试",
        ),
    }
}

#[cfg(test)]
mod tests;
