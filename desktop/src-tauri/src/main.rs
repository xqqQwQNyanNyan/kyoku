#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod replay;
mod settings;

use convlog::Event;
use kyoku::{
    agent::{AgentSession, review_evidence},
    mahjong::player_index::PlayerIndex,
    mortal::Mortal,
    review::{GameReview, RecordedAction, review_game},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicU64, Ordering},
};
use tauri::Manager;

#[derive(Debug, Serialize)]
struct UiError {
    code: &'static str,
    message: String,
    event_index: Option<usize>,
}

impl UiError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            event_index: None,
        }
    }
    fn at(code: &'static str, message: impl Into<String>, event_index: usize) -> Self {
        Self {
            code,
            message: message.into(),
            event_index: Some(event_index),
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, UiError> {
    mutex
        .lock()
        .map_err(|_| UiError::new("state", "会话状态异常，请重新打开应用"))
}

#[derive(Default)]
struct Desktop {
    sequence: AtomicU64,
    game: Mutex<Option<Arc<Game>>>,
}

struct Game {
    id: u64,
    events: Vec<Event>,
    reviews: Mutex<[Option<Arc<GameReview>>; 4]>,
    conversation: Mutex<Option<Conversation>>,
}

struct Conversation {
    key: (u8, usize, String),
    session: AgentSession,
}

impl Desktop {
    fn game(&self, id: u64) -> Result<Arc<Game>, UiError> {
        lock(&self.game)?
            .as_ref()
            .filter(|g| g.id == id)
            .cloned()
            .ok_or_else(|| UiError::new("stale_game", "牌谱已更换，请在当前牌谱重试"))
    }
}

#[derive(Serialize)]
struct Imported {
    id: u64,
    #[serde(flatten)]
    data: replay::ReplayData,
}

#[tauri::command]
async fn import_log(json: String, state: tauri::State<'_, Desktop>) -> Result<Imported, UiError> {
    let id = state.sequence.fetch_add(1, Ordering::SeqCst) + 1;
    let (events, data) = tauri::async_runtime::spawn_blocking(move || replay::parse(&json))
        .await
        .map_err(|_| UiError::new("task", "读取牌谱任务异常结束"))??;
    let mut current = lock(&state.game)?;
    if state.sequence.load(Ordering::SeqCst) != id {
        return Err(UiError::new("stale_import", "已选择另一份牌谱"));
    }
    *current = Some(Arc::new(Game {
        id,
        events,
        reviews: Mutex::new(std::array::from_fn(|_| None)),
        conversation: Mutex::new(None),
    }));
    Ok(Imported { id, data })
}

#[derive(Serialize)]
struct DecisionView {
    event_index: usize,
    turn: usize,
    actual: Value,
    evidence: Value,
}

fn decision_views(review: &GameReview) -> Vec<DecisionView> {
    review
        .decisions()
        .iter()
        .map(|point| DecisionView {
            event_index: point.review.event_index,
            turn: point.turn,
            actual: match &point.actual {
                RecordedAction::Taken { action, .. } => json!({"kind": "taken", "action": action}),
                RecordedAction::Passed => json!({"kind": "passed"}),
                RecordedAction::Unresolved => json!({"kind": "unresolved"}),
            },
            evidence: review_evidence(&point.review),
        })
        .collect()
}

#[tauri::command]
async fn analyze_game(
    id: u64,
    player: u8,
    state: tauri::State<'_, Desktop>,
    app: tauri::AppHandle,
) -> Result<Vec<DecisionView>, UiError> {
    let player =
        PlayerIndex::try_from(player).map_err(|_| UiError::new("player", "玩家编号必须为 0..3"))?;
    let game = state.game(id)?;
    let paths = config::RuntimePaths::resolve(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        // 每份牌谱只允许一个推理任务；后台锁不影响前端已加载的回放。
        let mut cache = game
            .reviews
            .try_lock()
            .map_err(|_| UiError::new("busy", "该牌谱正在分析，请稍候"))?;
        let slot = &mut cache[usize::from(player.get_id())];
        if slot.is_none() {
            let review = review_game(&game.events, player, &paths.borrowed())
                .map_err(|error| UiError::new("analysis", format!("Mortal 分析失败：{error}")))?;
            *slot = Some(Arc::new(review));
        }
        match slot {
            Some(review) => Ok(decision_views(review)),
            None => Err(UiError::new("state", "分析缓存未建立")),
        }
    })
    .await
    .map_err(|_| UiError::new("task", "分析任务异常结束"))?
}

#[derive(Deserialize)]
struct Question {
    id: u64,
    player: u8,
    event_index: usize,
    conversation_id: String,
    text: String,
}

#[tauri::command]
async fn ask(
    question: Question,
    state: tauri::State<'_, Desktop>,
    app: tauri::AppHandle,
) -> Result<String, UiError> {
    PlayerIndex::try_from(question.player)
        .map_err(|_| UiError::new("player", "玩家编号必须为 0..3"))?;
    if question.conversation_id.is_empty() || question.conversation_id.len() > 128 {
        return Err(UiError::new("conversation", "问答会话编号无效"));
    }
    let game = state.game(question.id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let review = game
            .reviews
            .try_lock()
            .map_err(|_| UiError::new("busy", "请等待牌谱分析完成"))?[usize::from(question.player)]
        .clone()
        .ok_or_else(|| UiError::new("no_analysis", "请先分析所选玩家的牌谱"))?;
        let point = review
            .at_event(question.event_index)
            .ok_or_else(|| UiError::new("no_decision", "请切换到该玩家的决策点再提问"))?;
        let key = (
            question.player,
            question.event_index,
            question.conversation_id,
        );
        let mut conversation = game
            .conversation
            .try_lock()
            .map_err(|_| UiError::new("busy", "上一条回答仍在生成，请稍候重试"))?;
        if conversation.as_ref().is_none_or(|c| c.key != key) {
            let config = app.state::<settings::SettingsStore>().load()?;
            // 只从 Review 建立证据；完整回放与实际后续动作不会交给 Agent。
            let session = AgentSession::new(&point.review, &config.borrowed())
                .map_err(|error| UiError::new("agent", error.to_string()))?;
            *conversation = Some(Conversation { key, session });
        }
        match conversation.as_mut() {
            Some(c) => c
                .session
                .ask(&question.text)
                .map_err(|error| UiError::new("agent", error.to_string())),
            None => Err(UiError::new("state", "问答会话未建立")),
        }
    })
    .await
    .map_err(|_| UiError::new("task", "问答任务异常结束"))?
}

#[tauri::command]
async fn get_settings(app: tauri::AppHandle) -> Result<settings::SettingsView, UiError> {
    tauri::async_runtime::spawn_blocking(move || app.state::<settings::SettingsStore>().view())
        .await
        .map_err(|_| UiError::new("task", "读取设置任务异常结束"))?
}

#[tauri::command]
async fn save_settings(
    input: settings::SettingsInput,
    app: tauri::AppHandle,
) -> Result<settings::SettingsView, UiError> {
    tauri::async_runtime::spawn_blocking(move || app.state::<settings::SettingsStore>().save(input))
        .await
        .map_err(|_| UiError::new("task", "保存设置任务异常结束"))?
}

#[tauri::command]
async fn test_connection(
    input: settings::SettingsInput,
    app: tauri::AppHandle,
) -> Result<(), UiError> {
    tauri::async_runtime::spawn_blocking(move || app.state::<settings::SettingsStore>().test(input))
        .await
        .map_err(|_| UiError::new("task", "连接测试任务异常结束"))?
}

#[derive(Serialize)]
struct RuntimeStatus {
    bundled: bool,
    available: bool,
    checked: bool,
    model: String,
}

#[tauri::command]
async fn runtime_status(check: bool, app: tauri::AppHandle) -> Result<RuntimeStatus, UiError> {
    let paths = config::RuntimePaths::resolve(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut status = RuntimeStatus {
            bundled: paths.bundled,
            available: paths.python.is_file()
                && paths.checkpoint.is_file()
                && paths.runtime.join("mortal/libriichi.so").is_file(),
            checked: false,
            model: "Mortal V4 · mortal-582500 · CPU".into(),
        };
        if check {
            let player =
                PlayerIndex::try_from(0).map_err(|_| UiError::new("player", "玩家编号无效"))?;
            let engine = Mortal::start(&paths.borrowed(), player)
                .map_err(|error| UiError::new("runtime", format!("引擎检查失败：{error}")))?;
            status.model = format!(
                "Mortal V{} · {} · CPU",
                engine.model().version,
                engine.model().tag
            );
            engine
                .finish()
                .map_err(|error| UiError::new("runtime", format!("引擎检查失败：{error}")))?;
            status.available = true;
            status.checked = true;
        }
        Ok(status)
    })
    .await
    .map_err(|_| UiError::new("task", "引擎检查任务异常结束"))?
}

fn main() {
    let result = tauri::Builder::default()
        .manage(Desktop::default())
        .setup(|app| {
            app.manage(settings::SettingsStore::new(
                app.path().app_config_dir()?,
                config::development_home(),
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            import_log,
            analyze_game,
            ask,
            get_settings,
            save_settings,
            test_connection,
            runtime_status
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        eprintln!("无法启动 Kyoku：{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn obsolete_document_id_cannot_read_new_game() {
        let state = Desktop::default();
        *state.game.lock().unwrap() = Some(Arc::new(Game {
            id: 2,
            events: Vec::new(),
            reviews: Mutex::new(std::array::from_fn(|_| None)),
            conversation: Mutex::new(None),
        }));
        assert_eq!(state.game(1).err().unwrap().code, "stale_game");
        assert_eq!(state.game(2).unwrap().id, 2);
    }
}
