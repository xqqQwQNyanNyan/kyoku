use crate::{UiError, lock};
use kyoku::agent::{AgentSession, SessionArchive};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
static FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionDocument {
    version: u32,
    pub id: String,
    pub title: String,
    pub context_label: String,
    created_at: u64,
    updated_at: u64,
    pub archive: SessionArchive,
    /// 请求前落盘；进程退出后仍能看见未完成的问题，并手动重试。
    pub pending_question: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct SessionView {
    #[serde(flatten)]
    document: SessionDocument,
    busy: bool,
}

#[derive(Serialize)]
pub(crate) struct SessionSummary {
    id: String,
    title: String,
    context_label: String,
    updated_at: u64,
    busy: bool,
    interrupted: bool,
    failed: bool,
}

#[derive(Serialize)]
pub(crate) struct SessionList {
    sessions: Vec<SessionSummary>,
    warnings: Vec<String>,
}

pub(crate) struct SessionStore {
    directory: PathBuf,
    gate: Mutex<HashSet<String>>,
}

// 只登记同一会话的写操作；计算和文件读写期间不持有全局锁。
struct SessionOperation<'a> {
    store: &'a SessionStore,
    id: String,
}

impl Drop for SessionOperation<'_> {
    fn drop(&mut self) {
        if let Ok(mut busy) = self.store.gate.lock() {
            busy.remove(&self.id);
        }
    }
}

fn io_error() -> UiError {
    UiError::new(
        "session_io",
        "无法读写本机会话文件，请检查存储空间和目录权限",
    )
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

fn parse(text: &str) -> Result<SessionDocument, UiError> {
    if text.len() as u64 > MAX_FILE_BYTES {
        return Err(UiError::new("session_format", "会话文件不能超过 32 MiB"));
    }
    let document: SessionDocument = serde_json::from_str(text)
        .map_err(|_| UiError::new("session_format", "不是有效的 Kyoku 会话 JSON"))?;
    if document.version != 1
        || !valid_id(&document.id)
        || document.title.len() > 1024
        || document.context_label.len() > 2048
        || document
            .pending_question
            .as_ref()
            .is_some_and(|q| q.len() > 16 * 1024)
    {
        return Err(UiError::new("session_format", "会话版本、编号或描述无效"));
    }
    document
        .archive
        .validate_for_display()
        .map_err(|e| UiError::new("session_format", e.to_string()))?;
    Ok(document)
}

fn read(path: &Path) -> Result<SessionDocument, UiError> {
    // 先限制大小，再分配文件内容；损坏文件不被自动覆盖。
    if fs::metadata(path).map_err(|_| io_error())?.len() > MAX_FILE_BYTES {
        return Err(UiError::new("session_format", "会话文件不能超过 32 MiB"));
    }
    parse(&fs::read_to_string(path).map_err(|_| io_error())?)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), UiError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| io_error())?;
    if file.write_all(bytes).and_then(|_| file.sync_all()).is_err() {
        let _ = fs::remove_file(path);
        return Err(io_error());
    }
    Ok(())
}

impl SessionStore {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            gate: Mutex::new(HashSet::new()),
        }
    }

    fn path(&self, id: &str) -> Result<PathBuf, UiError> {
        if !valid_id(id) {
            return Err(UiError::new("session", "会话编号无效"));
        }
        Ok(self.directory.join(format!("{id}.json")))
    }

    fn begin(&self, id: &str) -> Result<SessionOperation<'_>, UiError> {
        self.path(id)?;
        if !lock(&self.gate)?.insert(id.into()) {
            return Err(UiError::new("busy", "此会话仍在处理，请稍后重试"));
        }
        Ok(SessionOperation {
            store: self,
            id: id.into(),
        })
    }

    fn save(&self, document: &SessionDocument) -> Result<(), UiError> {
        fs::create_dir_all(&self.directory).map_err(|_| io_error())?;
        let bytes = serde_json::to_vec(document).map_err(|_| io_error())?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(UiError::new(
                "session_size",
                "会话已达到 32 MiB，请新建会话",
            ));
        }
        let target = self.path(&document.id)?;
        let temporary = self.directory.join(format!(
            ".{}-{}.tmp",
            document.id,
            FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        write_new(&temporary, &bytes)?;
        if fs::rename(&temporary, target).is_err() {
            let _ = fs::remove_file(temporary);
            return Err(io_error());
        }
        Ok(())
    }

    pub fn list(&self) -> Result<SessionList, UiError> {
        let busy = lock(&self.gate)?.clone();
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(SessionList {
                    sessions: vec![],
                    warnings: vec![],
                });
            }
            Err(_) => return Err(io_error()),
        };
        let mut result = SessionList {
            sessions: vec![],
            warnings: vec![],
        };
        for entry in entries {
            let path = entry.map_err(|_| io_error())?.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            match read(&path) {
                Ok(doc) => result.sessions.push(SessionSummary {
                    failed: doc.archive.last_error().is_some(),
                    busy: busy.contains(&doc.id),
                    interrupted: doc.pending_question.is_some() && !busy.contains(&doc.id),
                    id: doc.id,
                    title: doc.title,
                    context_label: doc.context_label,
                    updated_at: doc.updated_at,
                }),
                Err(_) => result.warnings.push(format!(
                    "无法读取会话 {}，原文件已保留",
                    path.file_name().unwrap_or_default().to_string_lossy()
                )),
            }
        }
        result
            .sessions
            .sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(result)
    }

    pub fn get(&self, id: &str) -> Result<SessionView, UiError> {
        let was_busy = lock(&self.gate)?.contains(id);
        let path = self.path(id)?;
        let mut document = read(&path)?;
        let busy = lock(&self.gate)?.contains(id);
        // 若读取期间恰好完成，返回已保存的结果，避免把旧的 pending 误报为中断。
        if was_busy && !busy {
            document = read(&path)?;
        }
        Ok(SessionView { document, busy })
    }

    pub fn create(&self, id: &str, label: String, archive: SessionArchive) -> Result<(), UiError> {
        let _operation = self.begin(id)?;
        if label.len() > 2048 {
            return Err(UiError::new("session", "会话局面描述过长"));
        }
        if self.path(id)?.exists() {
            return Err(UiError::new("session", "会话编号已存在，请打开原会话继续"));
        }
        self.save(&SessionDocument {
            version: 1,
            id: id.into(),
            title: "新会话".into(),
            context_label: label,
            created_at: now(),
            updated_at: now(),
            archive,
            pending_question: None,
        })
    }

    pub fn import(&self, text: &str) -> Result<SessionView, UiError> {
        let mut document = parse(text)?;
        // 外部导入仍完整重算证据；普通列表、打开和导出只做浏览校验。
        document.archive = SessionArchive::from_json(
            &serde_json::to_string(&document.archive).map_err(|_| io_error())?,
        )
        .map_err(|e| UiError::new("session_format", e.to_string()))?;
        // 导入始终创建副本，不能覆盖现有会话或正在运行的任务。
        document.id = format!(
            "import-{}-{}",
            now(),
            FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        document.updated_at = now().max(document.updated_at.saturating_add(1));
        let _operation = self.begin(&document.id)?;
        self.save(&document)?;
        Ok(SessionView {
            document,
            busy: false,
        })
    }

    pub fn export(&self, id: &str, directory: &Path) -> Result<String, UiError> {
        let document = read(&self.path(id)?)?;
        let path = directory.join(format!(
            "Kyoku-session-{id}-{}-{}.json",
            now(),
            FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        write_new(
            &path,
            &serde_json::to_vec(&document).map_err(|_| io_error())?,
        )?;
        Ok(path.to_string_lossy().into_owned())
    }

    pub fn ask(
        &self,
        id: &str,
        question: &str,
        config: &kyoku::agent::AgentConfig<'_>,
    ) -> Result<SessionView, UiError> {
        let question = question.trim();
        if question.is_empty() || question.len() > 16 * 1024 {
            return Err(UiError::new("question", "问题不能为空，且不能超过 16 KiB"));
        }
        let _operation = self.begin(id)?;
        let mut document = read(&self.path(id)?)?;
        let mut session = AgentSession::from_archive(&document.archive, config)
            .map_err(|e| UiError::new("agent", e.to_string()))?;
        if document.title == "新会话" {
            document.title = question.chars().take(40).collect();
        }
        document.pending_question = Some(question.into());
        document.updated_at = now().max(document.updated_at.saturating_add(1));
        self.save(&document)?;
        let result = session.ask(question);
        document.archive = session.archive().clone();
        document.pending_question = None;
        document.updated_at = now().max(document.updated_at.saturating_add(1));
        self.save(&document)?;
        result.map_err(|e| UiError::new("agent", e.to_string()))?;
        Ok(SessionView {
            document,
            busy: false,
        })
    }
}

#[cfg(test)]
mod tests;
