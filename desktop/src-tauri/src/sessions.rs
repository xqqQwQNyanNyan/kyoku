use crate::{
    UiError,
    library::{ReplayLibrary, ReplayOrigin},
    lock,
};
use kyoku::agent::{AgentContext, AgentSession, QuestionControl, SessionArchive};
use kyoku::mahjong::player_index::PlayerIndex;

mod game;
pub(crate) use game::{SessionGame, SessionPosition, SessionSource};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_TITLE_CHARS: usize = 32;
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<SessionGame>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<SessionPosition>,
}

#[derive(Serialize)]
pub(crate) struct SessionView {
    #[serde(flatten)]
    pub document: SessionDocument,
    busy: bool,
}

#[derive(Serialize)]
pub(crate) struct SessionSummary {
    id: String,
    game_key: Option<String>,
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

#[derive(Serialize)]
pub(crate) struct ReplayDeletion {
    name: String,
    session_ids: Vec<String>,
}

/// 逐文件删除发生 I/O 失败时，仍返回已删除的会话，供界面清理缓存。
#[derive(Serialize)]
pub(crate) struct ReplayDeletionResult {
    session_ids: Vec<String>,
    pub replay_deleted: bool,
    error: Option<UiError>,
}

enum SessionQuestion<'a> {
    New(&'a str),
    Retry(usize),
    Pending,
}

pub(crate) struct SessionStore {
    directory: PathBuf,
    library: Arc<ReplayLibrary>,
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
pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

fn default_title(question: &str) -> String {
    question
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_TITLE_CHARS)
        .collect()
}

fn parse(text: &str) -> Result<SessionDocument, UiError> {
    if text.len() as u64 > MAX_FILE_BYTES {
        return Err(UiError::new("session_format", "会话文件不能超过 32 MiB"));
    }
    let mut document: SessionDocument = serde_json::from_str(text)
        .map_err(|_| UiError::new("session_format", "不是有效的 Kyoku 会话 JSON"))?;
    if !matches!(document.version, 1..=3)
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
    if let Some(game) = &document.game {
        let position = document
            .position
            .ok_or_else(|| UiError::new("session_format", "缺少会话浏览位置"))?;
        if document.version == 3 && game.events.is_empty() {
            if !SessionGame::valid_key(&game.key) || position.player >= 4 {
                return Err(UiError::new("session_format", "会话牌谱标识或玩家无效"));
            }
        } else {
            game.validate()?;
            position.validate(game)?;
            SessionPosition::from_evidence(document.archive.evidence())?.validate(game)?;
        }
    } else if document.version >= 2 {
        return Err(UiError::new("session_format", "缺少会话关联的牌谱"));
    }
    document
        .archive
        .validate_for_display()
        .map_err(|e| UiError::new("session_format", e.to_string()))?;
    // 兼容旧文件的长标题，不因显示限制而拒绝整份历史记录。
    document.title = default_title(&document.title);
    if document.title.is_empty() {
        document.title = "新会话".into();
    }
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
    pub fn new(directory: PathBuf, library: Arc<ReplayLibrary>) -> Self {
        Self {
            directory,
            library,
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
        let mut compact = document.clone();
        if let Some(game) = &mut compact.game {
            // 牌谱先持久化，再原子替换会话；失败时原会话仍带有完整牌谱。
            if !game.events.is_empty() {
                self.library
                    .save(game, &document.context_label, ReplayOrigin::Session)?;
                game.events.clear();
                game.round_details.clear();
            }
            compact.version = 3;
        }
        let bytes = serde_json::to_vec(&compact).map_err(|_| io_error())?;
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

    /// 打开牌桌或续聊时才读取关联牌谱；历史列表不依赖牌谱文件存在。
    pub fn open_game(&self, id: &str) -> Result<SessionDocument, UiError> {
        self.resolve_game(read(&self.path(id)?)?)
    }

    fn resolve_game(&self, mut document: SessionDocument) -> Result<SessionDocument, UiError> {
        if let Some(game) = &mut document.game {
            if game.events.is_empty() {
                let saved = self.library.get(&game.key)?;
                document.context_label = saved.name;
                *game = saved.game;
            }
            document
                .position
                .ok_or_else(|| UiError::new("session_format", "缺少会话浏览位置"))?
                .validate(game)?;
            SessionPosition::from_evidence(document.archive.evidence())?.validate(game)?;
        }
        Ok(document)
    }

    /// 打开牌谱库时迁移旧的内嵌牌谱；单个坏文件不影响其他记录。
    pub fn migrate_embedded(&self) -> Result<Vec<String>, UiError> {
        let mut warnings = Vec::new();
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(warnings),
            Err(_) => return Err(io_error()),
        };
        for entry in entries {
            let path = entry.map_err(|_| io_error())?.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let migrate = || -> Result<(), UiError> {
                let document = read(&path)?;
                if document
                    .game
                    .as_ref()
                    .is_none_or(|game| game.events.is_empty())
                {
                    return Ok(());
                }
                if path.file_stem().and_then(|s| s.to_str()) != Some(&document.id) {
                    return Err(UiError::new("session_format", "会话文件名与编号不一致"));
                }
                let _operation = self.begin(&document.id)?;
                // 取得写锁后重新读取，避免覆盖刚完成的问答。
                self.save(&read(&path)?)
            };
            if let Err(error) = migrate() {
                if error.code == "busy" {
                    continue;
                }
                warnings.push(format!(
                    "会话 {} 未迁移，原文件已保留：{}",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    error.message
                ));
            }
        }
        Ok(warnings)
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
                Ok(doc) if path.file_stem().and_then(|stem| stem.to_str()) == Some(&doc.id) => {
                    result.sessions.push(SessionSummary {
                        failed: doc.archive.last_error().is_some(),
                        busy: busy.contains(&doc.id),
                        interrupted: doc.pending_question.is_some() && !busy.contains(&doc.id),
                        game_key: doc.game.map(|game| game.key),
                        id: doc.id,
                        title: doc.title,
                        context_label: doc.context_label,
                        updated_at: doc.updated_at,
                    })
                }
                _ => result.warnings.push(format!(
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

    /// 修改本地标题，不改变问题和模型上下文。
    pub fn rename(&self, id: &str, title: &str) -> Result<SessionView, UiError> {
        let title = title.trim();
        if title.is_empty()
            || title.chars().count() > MAX_TITLE_CHARS
            || title.chars().any(char::is_control)
        {
            return Err(UiError::new(
                "session_title",
                "会话标题需为 1–32 个字符，且不能换行",
            ));
        }
        let _operation = self.begin(id)?;
        let mut document = read(&self.path(id)?)?;
        document.title = title.into();
        document.updated_at = now().max(document.updated_at.saturating_add(1));
        self.save(&document)?;
        Ok(SessionView {
            document,
            busy: false,
        })
    }

    /// 删除会话文件，保留关联牌谱；与问答和位置保存互斥。
    pub fn delete(&self, id: &str) -> Result<(), UiError> {
        let _operation = self.begin(id)?;
        fs::remove_file(self.path(id)?).map_err(|_| io_error())
    }

    fn replay_session_ids(&self, key: &str) -> Result<Vec<String>, UiError> {
        let mut ids = Vec::new();
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(ids),
            Err(_) => return Err(io_error()),
        };
        for entry in entries {
            let path = entry.map_err(|_| io_error())?.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            // 无法确认关联关系时不跳过坏文件，避免漏删后留下失去牌谱的会话。
            let document = read(&path).map_err(|_| {
                UiError::new(
                    "session_format",
                    "有会话文件无法读取，暂时无法确认牌谱的关联会话，请先检查数据文件夹",
                )
            })?;
            if path.file_stem().and_then(|stem| stem.to_str()) != Some(&document.id) {
                return Err(UiError::new(
                    "session_format",
                    "会话文件名与编号不一致，无法删除牌谱",
                ));
            }
            if document.game.as_ref().is_some_and(|game| game.key == key) {
                ids.push(document.id);
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// 确认框展示的关联会话集合；实际删除前再次核对。
    pub fn preview_replay_deletion(&self, key: &str) -> Result<ReplayDeletion, UiError> {
        let _guard = lock(&self.gate)?;
        Ok(ReplayDeletion {
            name: self.library.get(key)?.name,
            session_ids: self.replay_session_ids(key)?,
        })
    }

    pub fn delete_replay(
        &self,
        key: &str,
        mut expected: Vec<String>,
    ) -> Result<ReplayDeletionResult, UiError> {
        // 新会话在首次落盘前还没有关联记录，因此有写操作时暂不删除牌谱。
        let busy = lock(&self.gate)?;
        if !busy.is_empty() {
            return Err(UiError::new("busy", "有会话正在处理，请稍后再删除牌谱"));
        }
        self.library.get(key)?;
        let ids = self.replay_session_ids(key)?;
        expected.sort();
        if ids != expected {
            return Err(UiError::new(
                "session_changed",
                "关联会话已变化，请取消后重新确认删除",
            ));
        }
        let mut result = ReplayDeletionResult {
            session_ids: Vec::new(),
            replay_deleted: false,
            error: None,
        };
        let remove = || -> Result<(), UiError> {
            for id in ids {
                fs::remove_file(self.path(&id)?).map_err(|_| io_error())?;
                result.session_ids.push(id);
            }
            self.library.delete(key)?;
            result.replay_deleted = true;
            Ok(())
        };
        result.error = remove().err();
        Ok(result)
    }

    pub fn set_position(
        &self,
        id: &str,
        game_key: &str,
        position: SessionPosition,
    ) -> Result<(), UiError> {
        let _operation = self.begin(id)?;
        let mut document = self.open_game(id)?;
        let game = document
            .game
            .as_ref()
            .filter(|game| game.key == game_key)
            .ok_or_else(|| UiError::new("session", "此会话属于另一份牌谱"))?;
        position.validate(game)?;
        if document.position != Some(position) {
            document.position = Some(position);
            document.updated_at = now().max(document.updated_at.saturating_add(1));
            self.save(&document)?;
        }
        Ok(())
    }

    pub fn import(&self, text: &str) -> Result<SessionView, UiError> {
        let mut document = parse(text)?;
        if let Some(game) = &document.game {
            if game.events.is_empty() {
                return Err(UiError::new(
                    "session_format",
                    "此文件仅引用本机牌谱，请在原应用中使用“导出 JSON”后再加载",
                ));
            }
            crate::replay::replay(&game.events)?;
        }
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
        // 用户主动加载导出文件，允许恢复此前删除的牌谱。
        if let Some(game) = &document.game {
            self.library
                .save(game, &document.context_label, ReplayOrigin::File)?;
        }
        self.save(&document)?;
        Ok(SessionView {
            document,
            busy: false,
        })
    }

    pub fn export(&self, id: &str, directory: &Path) -> Result<String, UiError> {
        let mut document = self.open_game(id)?;
        if document.game.is_some() {
            document.version = 2;
        }
        let path = directory.join(format!(
            "Kyoku-session-{id}-{}-{}.json",
            now(),
            FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let bytes = serde_json::to_vec(&document).map_err(|_| io_error())?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(UiError::new(
                "session_size",
                "包含牌谱的导出会话超过 32 MiB，请先备份数据文件夹",
            ));
        }
        write_new(&path, &bytes)?;
        Ok(path.to_string_lossy().into_owned())
    }

    pub fn ask_with_control(
        &self,
        id: &str,
        question: &str,
        config: &kyoku::agent::AgentConfig<'_>,
        source: Option<SessionSource<'_>>,
        control: &QuestionControl,
    ) -> Result<SessionView, UiError> {
        self.ask_question(id, SessionQuestion::New(question), config, source, control)
    }

    pub fn retry_with_control(
        &self,
        id: &str,
        turn: Option<usize>,
        config: &kyoku::agent::AgentConfig<'_>,
        control: &QuestionControl,
    ) -> Result<SessionView, UiError> {
        let question = match turn {
            Some(index) => SessionQuestion::Retry(index),
            None => SessionQuestion::Pending,
        };
        self.ask_question(id, question, config, None, control)
    }

    fn ask_question(
        &self,
        id: &str,
        request: SessionQuestion<'_>,
        config: &kyoku::agent::AgentConfig<'_>,
        source: Option<SessionSource<'_>>,
        control: &QuestionControl,
    ) -> Result<SessionView, UiError> {
        let _operation = self.begin(id)?;
        let path = self.path(id)?;
        let exists = path.try_exists().map_err(|_| io_error())?;
        let mut document = if exists {
            self.resolve_game(read(&path)?)?
        } else {
            let source = source
                .as_ref()
                .ok_or_else(|| UiError::new("session", "会话不存在"))?;
            if source.label.len() > 2048 {
                return Err(UiError::new("session", "牌谱名称过长"));
            }
            let session = AgentSession::with_context(source.context, config)
                .map_err(|e| UiError::new("agent", e.to_string()))?;
            SessionDocument {
                version: 2,
                id: id.into(),
                title: "新会话".into(),
                context_label: source.label.into(),
                created_at: now(),
                updated_at: now(),
                archive: session.archive().clone(),
                pending_question: None,
                game: Some(source.game.clone()),
                position: Some(SessionPosition::from_evidence(source.context.evidence())?),
            }
        };
        let (question, retry_context) = match &request {
            SessionQuestion::New(text) => (text.trim().to_owned(), None),
            SessionQuestion::Retry(index) => {
                let (text, context) = document
                    .archive
                    .failed_turn_context(*index)
                    .ok_or_else(|| UiError::new("question", "找不到可重试的失败问题"))?;
                (text.to_owned(), Some(context))
            }
            SessionQuestion::Pending => (
                document
                    .pending_question
                    .clone()
                    .ok_or_else(|| UiError::new("question", "没有未完成的问题"))?,
                None,
            ),
        };
        if question.is_empty() || question.len() > 16 * 1024 {
            return Err(UiError::new("question", "问题不能为空，且不能超过 16 KiB"));
        }
        let mut session = AgentSession::from_archive(&document.archive, config)
            .map_err(|e| UiError::new("agent", e.to_string()))?;
        if let Some(context) = retry_context {
            session.set_context(&context);
        } else if let Some(source) = source {
            if document
                .game
                .as_ref()
                .is_none_or(|game| game.key != source.game.key || game.events != source.game.events)
            {
                return Err(UiError::new("session", "此会话不属于当前牌谱，请新建会话"));
            }
            session.set_context(source.context);
            document.position = Some(SessionPosition::from_evidence(source.context.evidence())?);
        } else if matches!(request, SessionQuestion::New(_))
            && let (Some(game), Some(position)) = (&document.game, document.position)
            && position != SessionPosition::from_evidence(document.archive.evidence())?
        {
            let player = PlayerIndex::try_from(position.player)
                .map_err(|_| UiError::new("player", "玩家编号必须为 0..3"))?;
            let context = AgentContext::from_events(&game.events, player, position.event_index)
                .map_err(|e| UiError::new("session", e.to_string()))?;
            session.set_context(&context);
        }
        // 请求快照先落盘，退出或失败后仍能按原位置重试。
        document.archive = session.archive().clone();
        if !exists {
            document.title = default_title(&question);
        }
        document.pending_question = Some(question.clone());
        document.updated_at = now().max(document.updated_at.saturating_add(1));
        self.save(&document)?;
        let result = session.ask_with_control(&question, control);
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
