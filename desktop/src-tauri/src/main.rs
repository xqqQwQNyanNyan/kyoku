#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod analyses;
mod config;
mod library;
mod log_link;
mod majsoul;
mod questions;
mod replay;
mod sessions;
mod settings;
mod storage;

use convlog::Event;
use kyoku::{
    agent::{AgentContext, QuestionControl, QuestionProgress, review_evidence},
    mahjong::player_index::PlayerIndex,
    mortal::Mortal,
    review::{GameReview, RecordedAction, ReviewControl, ReviewProgress, review_game_with_control},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicU64, Ordering},
};
use tauri::Manager;
use tauri::ipc::Channel;

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
    majsoul: Arc<Mutex<majsoul::Account>>,
}

struct Game {
    id: u64,
    key: String,
    mortal_supported: bool,
    events: Vec<Event>,
    round_details: Vec<replay::RoundDetails>,
    reviews: Mutex<[Option<Arc<GameReview>>; 4]>,
}

impl Game {
    fn context(&self, player: u8, event_index: usize) -> Result<AgentContext, UiError> {
        let player = PlayerIndex::try_from(player)
            .map_err(|_| UiError::new("player", "玩家编号必须为 0..3"))?;
        // 推理期间也能提问；未取得的分析结果明确留空，不等待整场推理。
        let review = match self.reviews.try_lock() {
            Ok(cache) => cache[usize::from(player.get_id())].clone(),
            Err(std::sync::TryLockError::WouldBlock) => None,
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err(UiError::new("state", "分析缓存异常，请重新打开牌谱"));
            }
        };
        if let Some(point) = review
            .as_ref()
            .and_then(|review| review.at_event(event_index))
        {
            return Ok(AgentContext::from(&point.review));
        }
        AgentContext::from_events(&self.events, player, event_index)
            .map_err(|error| UiError::at("context", error.to_string(), event_index))
    }
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
    game_key: String,
    name: String,
    #[serde(flatten)]
    data: replay::ReplayData,
}

#[tauri::command]
async fn import_log(
    json: String,
    name: String,
    state: tauri::State<'_, Desktop>,
    app: tauri::AppHandle,
) -> Result<Imported, UiError> {
    let id = state.sequence.fetch_add(1, Ordering::SeqCst) + 1;
    let (events, data, name) = tauri::async_runtime::spawn_blocking(move || {
        let storage = app.state::<storage::Storage>();
        let stores = storage.read()?;
        let (events, data) = library::parse_input(&json)?;
        let name = stores.library().save(
            &sessions::SessionGame {
                key: sessions::SessionGame::key(&events)?,
                events: events.clone(),
                round_details: data.round_details(),
            },
            &name,
            library::ReplayOrigin::File,
        )?;
        Ok::<_, UiError>((events, data, name))
    })
    .await
    .map_err(|_| UiError::new("task", "读取牌谱任务异常结束"))??;
    finish_import(&state, id, events, data, name)
}

#[tauri::command]
async fn import_link(
    link: String,
    state: tauri::State<'_, Desktop>,
    app: tauri::AppHandle,
) -> Result<Imported, UiError> {
    let id = state.sequence.fetch_add(1, Ordering::SeqCst) + 1;
    let account = Arc::clone(&state.majsoul);
    let (events, data, name) = tauri::async_runtime::spawn_blocking(move || {
        let storage = app.state::<storage::Storage>();
        let stores = storage.read()?;
        let json = log_link::download(&link, &account)?;
        let (events, data) = replay::parse(&json)?;
        let name = stores.library().save(
            &sessions::SessionGame {
                key: sessions::SessionGame::key(&events)?,
                events: events.clone(),
                round_details: data.round_details(),
            },
            &link,
            library::ReplayOrigin::Link,
        )?;
        Ok::<_, UiError>((events, data, name))
    })
    .await
    .map_err(|_| UiError::new("task", "下载牌谱任务异常结束"))??;
    finish_import(&state, id, events, data, name)
}

#[tauri::command]
async fn list_replays(app: tauri::AppHandle) -> Result<library::ReplayList, UiError> {
    tauri::async_runtime::spawn_blocking(move || {
        let storage = app.state::<storage::Storage>();
        let stores = storage.read()?;
        let migration = stores.sessions().migrate_embedded();
        let mut list = stores.library().list()?;
        match migration {
            Ok(warnings) => list.warnings.extend(warnings),
            Err(error) => list.warnings.push(error.message),
        }
        Ok(list)
    })
    .await
    .map_err(|_| UiError::new("task", "读取牌谱库任务异常结束"))?
}

#[tauri::command]
async fn preview_replay_deletion(
    key: String,
    app: tauri::AppHandle,
) -> Result<sessions::ReplayDeletion, UiError> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<storage::Storage>()
            .read()?
            .sessions()
            .preview_replay_deletion(&key)
    })
    .await
    .map_err(|_| UiError::new("task", "读取关联会话任务异常结束"))?
}

#[tauri::command]
async fn delete_replay(
    key: String,
    session_ids: Vec<String>,
    app: tauri::AppHandle,
) -> Result<sessions::ReplayDeletionResult, UiError> {
    tauri::async_runtime::spawn_blocking(move || {
        let result = app
            .state::<storage::Storage>()
            .read()?
            .sessions()
            .delete_replay(&key, session_ids)?;
        if result.replay_deleted {
            let state = app.state::<Desktop>();
            let mut game = lock(&state.game)?;
            if game.as_ref().is_some_and(|game| game.key == key) {
                *game = None;
            }
        }
        Ok(result)
    })
    .await
    .map_err(|_| UiError::new("task", "删除牌谱任务异常结束"))?
}

#[tauri::command]
async fn delete_session(id: String, app: tauri::AppHandle) -> Result<(), UiError> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<storage::Storage>()
            .read()?
            .sessions()
            .delete(&id)
    })
    .await
    .map_err(|_| UiError::new("task", "删除会话任务异常结束"))?
}

#[tauri::command]
async fn rename_replay(
    key: String,
    name: String,
    app: tauri::AppHandle,
) -> Result<library::ReplaySummary, UiError> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<storage::Storage>()
            .read()?
            .library()
            .rename(&key, &name)
    })
    .await
    .map_err(|_| UiError::new("task", "修改牌谱名称任务异常结束"))?
}

#[tauri::command]
async fn open_replay(
    key: String,
    state: tauri::State<'_, Desktop>,
    app: tauri::AppHandle,
) -> Result<Imported, UiError> {
    let id = state.sequence.fetch_add(1, Ordering::SeqCst) + 1;
    let (events, data, name) = tauri::async_runtime::spawn_blocking(move || {
        let saved = app
            .state::<storage::Storage>()
            .read()?
            .library()
            .get(&key)?;
        let data = replay::replay(&saved.game.events)?.with_details(&saved.game.round_details)?;
        Ok::<_, UiError>((saved.game.events, data, saved.name))
    })
    .await
    .map_err(|_| UiError::new("task", "打开已保存牌谱任务异常结束"))??;
    finish_import(&state, id, events, data, name)
}

#[tauri::command]
async fn open_data_directory(app: tauri::AppHandle) -> Result<(), UiError> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<storage::Storage>()
            .read()?
            .library()
            .open_directory()
    })
    .await
    .map_err(|_| UiError::new("task", "打开数据文件夹任务异常结束"))?
}

#[tauri::command]
fn get_storage(app: tauri::AppHandle) -> Result<storage::StorageView, UiError> {
    app.state::<storage::Storage>().view()
}

#[tauri::command]
async fn choose_data_directory(app: tauri::AppHandle) -> Result<Option<String>, UiError> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("选择牌谱与对话保存目录")
            .blocking_pick_folder()
            .map(|path| {
                let path = path
                    .into_path()
                    .map_err(|_| UiError::new("storage_path", "请选择本地文件夹"))?;
                path.to_str()
                    .map(str::to_owned)
                    .ok_or_else(|| UiError::new("storage_path", "目录路径需为 Unicode 文本"))
            })
            .transpose()
    })
    .await
    .map_err(|_| UiError::new("task", "选择目录异常结束"))?
}

#[tauri::command]
async fn migrate_data(
    directory: String,
    request_id: String,
    on_progress: Channel<storage::MigrationProgress>,
    app: tauri::AppHandle,
) -> Result<storage::StorageView, UiError> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<storage::Storage>().migrate(
            std::path::Path::new(&directory),
            &request_id,
            |progress| {
                let _ = on_progress.send(progress);
            },
        )
    })
    .await
    .map_err(|_| UiError::new("task", "数据迁移异常结束，请检查当前保存位置"))?
}

#[tauri::command]
fn cancel_data_migration(request_id: String, app: tauri::AppHandle) -> Result<(), UiError> {
    app.state::<storage::Storage>().cancel(&request_id)
}

#[tauri::command]
async fn majsoul_status(state: tauri::State<'_, Desktop>) -> Result<bool, UiError> {
    let account = Arc::clone(&state.majsoul);
    tauri::async_runtime::spawn_blocking(move || Ok(lock(&account)?.logged_in()))
        .await
        .map_err(|_| UiError::new("task", "雀魂状态检查异常结束"))?
}

#[tauri::command]
async fn login_majsoul(
    input: majsoul::Credentials,
    app: tauri::AppHandle,
    state: tauri::State<'_, Desktop>,
) -> Result<(), UiError> {
    let paths = majsoul::Paths::resolve(&app)?;
    let account = Arc::clone(&state.majsoul);
    tauri::async_runtime::spawn_blocking(move || lock(&account)?.login(&paths, input))
        .await
        .map_err(|_| UiError::new("task", "雀魂登录任务异常结束"))?
}

#[tauri::command]
async fn logout_majsoul(state: tauri::State<'_, Desktop>) -> Result<(), UiError> {
    let account = Arc::clone(&state.majsoul);
    tauri::async_runtime::spawn_blocking(move || {
        lock(&account)?.logout();
        Ok(())
    })
    .await
    .map_err(|_| UiError::new("task", "雀魂退出任务异常结束"))?
}

fn finish_import(
    state: &Desktop,
    id: u64,
    events: Vec<Event>,
    data: replay::ReplayData,
    name: String,
) -> Result<Imported, UiError> {
    let mut current = lock(&state.game)?;
    if state.sequence.load(Ordering::SeqCst) != id {
        return Err(UiError::new("stale_import", "已选择另一份牌谱"));
    }
    let game_key = sessions::SessionGame::key(&events)?;
    *current = Some(Arc::new(Game {
        id,
        key: game_key.clone(),
        mortal_supported: data.mortal_supported,
        round_details: data.round_details(),
        events,
        reviews: Mutex::new(std::array::from_fn(|_| None)),
    }));
    Ok(Imported {
        id,
        game_key,
        name,
        data,
    })
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
    request_id: String,
    on_progress: Channel<ReviewProgress>,
    state: tauri::State<'_, Desktop>,
    app: tauri::AppHandle,
) -> Result<Vec<DecisionView>, UiError> {
    let player =
        PlayerIndex::try_from(player).map_err(|_| UiError::new("player", "玩家编号必须为 0..3"))?;
    let game = state.game(id)?;
    if !game.mortal_supported {
        return Err(UiError::new(
            "mortal_rules",
            "当前 Mortal 仅支持四人半庄分析，东风场可继续回放",
        ));
    }
    let paths = config::RuntimePaths::resolve(&app)?;
    let control = ReviewControl::new(move |progress| {
        let _ = on_progress.send(progress);
    });
    let running = app
        .state::<Arc<analyses::Analyses>>()
        .begin(id, &request_id, control.clone())?;
    tauri::async_runtime::spawn_blocking(move || {
        let _running = running;
        // 每份牌谱只允许一个推理任务；后台锁不影响前端已加载的回放。
        let mut cache = game
            .reviews
            .try_lock()
            .map_err(|_| UiError::new("busy", "该牌谱正在分析，请稍候"))?;
        let slot = &mut cache[usize::from(player.get_id())];
        if slot.is_none() {
            let review =
                review_game_with_control(&game.events, player, &paths.borrowed(), &control)
                    .map_err(|error| match error {
                        kyoku::review::ReviewError::Cancelled => {
                            UiError::new("cancelled", error.to_string())
                        }
                        _ => UiError::new("analysis", format!("Mortal 分析失败：{error}")),
                    })?;
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

#[tauri::command]
fn cancel_analysis(id: u64, request_id: String, app: tauri::AppHandle) -> Result<(), UiError> {
    app.state::<Arc<analyses::Analyses>>()
        .cancel(id, &request_id)
}

#[derive(Deserialize)]
struct Question {
    id: u64,
    player: u8,
    event_index: usize,
    conversation_id: String,
    text: String,
    context_label: String,
}

#[tauri::command]
async fn ask(
    question: Question,
    request_id: String,
    on_progress: Channel<QuestionProgress>,
    state: tauri::State<'_, Desktop>,
    app: tauri::AppHandle,
) -> Result<sessions::SessionView, UiError> {
    PlayerIndex::try_from(question.player)
        .map_err(|_| UiError::new("player", "玩家编号必须为 0..3"))?;
    if question.conversation_id.is_empty() || question.conversation_id.len() > 128 {
        return Err(UiError::new("conversation", "问答会话编号无效"));
    }
    let game = state.game(question.id)?;
    let id = question.conversation_id.clone();
    run_question(app, id, request_id, on_progress, move |app, control| {
        let context = game.context(question.player, question.event_index)?;
        let config = app.state::<settings::SettingsStore>().load()?;
        let saved_game = sessions::SessionGame {
            key: game.key.clone(),
            events: game.events.clone(),
            round_details: game.round_details.clone(),
        };
        app.state::<storage::Storage>()
            .read()?
            .sessions()
            .ask_with_control(
                &question.conversation_id,
                &question.text,
                &config.borrowed(),
                Some(sessions::SessionSource {
                    game: &saved_game,
                    label: &question.context_label,
                    context: &context,
                }),
                control,
            )
    })
    .await
}

async fn run_question(
    app: tauri::AppHandle,
    id: String,
    request_id: String,
    on_progress: Channel<QuestionProgress>,
    work: impl FnOnce(&tauri::AppHandle, &QuestionControl) -> Result<sessions::SessionView, UiError>
    + Send
    + 'static,
) -> Result<sessions::SessionView, UiError> {
    let channel = on_progress.clone();
    let control = QuestionControl::new(move |progress| {
        let _ = channel.send(progress);
    });
    let running = app
        .state::<Arc<questions::Questions>>()
        .begin(&id, &request_id, control)?;
    // 先登记再通知界面；收到此消息后，停止操作一定能找到对应的这一轮。
    let _ = on_progress.send(QuestionProgress::Preparing);
    tauri::async_runtime::spawn_blocking(move || work(&app, &running.control))
        .await
        .map_err(|_| UiError::new("task", "问答任务异常结束；可在历史会话中检查并重试"))?
}

#[tauri::command]
fn cancel_question(id: String, request_id: String, app: tauri::AppHandle) -> Result<(), UiError> {
    app.state::<Arc<questions::Questions>>()
        .cancel(&id, &request_id)
}

#[tauri::command]
async fn list_sessions(app: tauri::AppHandle) -> Result<sessions::SessionList, UiError> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<storage::Storage>().read()?.sessions().list()
    })
    .await
    .map_err(|_| UiError::new("task", "读取历史会话任务异常结束"))?
}

#[tauri::command]
async fn get_session(id: String, app: tauri::AppHandle) -> Result<sessions::SessionView, UiError> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<storage::Storage>().read()?.sessions().get(&id)
    })
    .await
    .map_err(|_| UiError::new("task", "读取会话任务异常结束"))?
}

#[tauri::command]
async fn rename_session(
    id: String,
    title: String,
    app: tauri::AppHandle,
) -> Result<sessions::SessionView, UiError> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<storage::Storage>()
            .read()?
            .sessions()
            .rename(&id, &title)
    })
    .await
    .map_err(|_| UiError::new("task", "修改会话标题任务异常结束"))?
}

#[tauri::command]
async fn continue_session(
    id: String,
    text: String,
    request_id: String,
    on_progress: Channel<QuestionProgress>,
    app: tauri::AppHandle,
) -> Result<sessions::SessionView, UiError> {
    run_question(
        app,
        id.clone(),
        request_id,
        on_progress,
        move |app, control| {
            let config = app.state::<settings::SettingsStore>().load()?;
            app.state::<storage::Storage>()
                .read()?
                .sessions()
                .ask_with_control(&id, &text, &config.borrowed(), None, control)
        },
    )
    .await
}

#[tauri::command]
async fn retry_session(
    id: String,
    turn: Option<usize>,
    request_id: String,
    on_progress: Channel<QuestionProgress>,
    app: tauri::AppHandle,
) -> Result<sessions::SessionView, UiError> {
    run_question(
        app,
        id.clone(),
        request_id,
        on_progress,
        move |app, control| {
            let config = app.state::<settings::SettingsStore>().load()?;
            app.state::<storage::Storage>()
                .read()?
                .sessions()
                .retry_with_control(&id, turn, &config.borrowed(), control)
        },
    )
    .await
}

#[derive(Serialize)]
struct OpenedSessionGame {
    replay: Imported,
    name: String,
    position: sessions::SessionPosition,
}

#[tauri::command]
async fn open_session_game(
    id: String,
    state: tauri::State<'_, Desktop>,
    app: tauri::AppHandle,
) -> Result<OpenedSessionGame, UiError> {
    let document = tauri::async_runtime::spawn_blocking(move || {
        app.state::<storage::Storage>()
            .read()?
            .sessions()
            .open_game(&id)
    })
    .await
    .map_err(|_| UiError::new("task", "读取会话牌谱异常结束"))??;
    let game = document
        .game
        .ok_or_else(|| UiError::new("session", "旧版会话未保存完整牌谱，可继续查看历史"))?;
    let position = document
        .position
        .ok_or_else(|| UiError::new("session", "会话没有浏览位置"))?;
    let existing = lock(&state.game)?
        .as_ref()
        .filter(|current| current.key == game.key)
        .cloned();
    let sequence = if existing.is_none() {
        Some(state.sequence.fetch_add(1, Ordering::SeqCst) + 1)
    } else {
        None
    };
    let (game, data) = tauri::async_runtime::spawn_blocking(move || {
        let data = replay::replay(&game.events)?.with_details(&game.round_details)?;
        Ok::<_, UiError>((game, data))
    })
    .await
    .map_err(|_| UiError::new("task", "恢复牌桌异常结束"))??;
    let replay = if let Some(current) = existing {
        state.game(current.id)?;
        Imported {
            id: current.id,
            game_key: current.key.clone(),
            name: document.context_label.clone(),
            data,
        }
    } else {
        finish_import(
            &state,
            sequence.ok_or_else(|| UiError::new("state", "缺少牌谱编号"))?,
            game.events,
            data,
            document.context_label.clone(),
        )?
    };
    Ok(OpenedSessionGame {
        replay,
        name: document.context_label,
        position,
    })
}

#[tauri::command]
async fn set_session_position(
    id: String,
    game_key: String,
    position: sessions::SessionPosition,
    app: tauri::AppHandle,
) -> Result<(), UiError> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<storage::Storage>()
            .read()?
            .sessions()
            .set_position(&id, &game_key, position)
    })
    .await
    .map_err(|_| UiError::new("task", "保存浏览位置异常结束"))?
}

#[tauri::command]
async fn import_session(
    json: String,
    app: tauri::AppHandle,
) -> Result<sessions::SessionView, UiError> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<storage::Storage>()
            .read()?
            .sessions()
            .import(&json)
    })
    .await
    .map_err(|_| UiError::new("task", "导入会话任务异常结束"))?
}

#[tauri::command]
async fn export_session(id: String, app: tauri::AppHandle) -> Result<String, UiError> {
    let directory = app
        .path()
        .download_dir()
        .map_err(|_| UiError::new("session_io", "无法定位下载目录"))?;
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<storage::Storage>()
            .read()?
            .sessions()
            .export(&id, &directory)
    })
    .await
    .map_err(|_| UiError::new("task", "导出会话任务异常结束"))?
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
    on_progress: Channel<QuestionProgress>,
    app: tauri::AppHandle,
) -> Result<(), UiError> {
    tauri::async_runtime::spawn_blocking(move || {
        let control = QuestionControl::new(move |progress| {
            let _ = on_progress.send(progress);
        });
        app.state::<settings::SettingsStore>().test(input, &control)
    })
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
                && paths
                    .runtime
                    .join(if cfg!(target_os = "windows") {
                        "mortal/libriichi.pyd"
                    } else {
                        "mortal/libriichi.so"
                    })
                    .is_file(),
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
        .plugin(tauri_plugin_dialog::init())
        .manage(Desktop::default())
        .manage(Arc::new(analyses::Analyses::default()))
        .setup(|app| {
            app.manage(
                storage::Storage::new(app.path().app_config_dir()?, app.path().app_data_dir()?)
                    .map_err(|error| std::io::Error::other(error.message))?,
            );
            app.manage(settings::SettingsStore::new(
                app.path().app_config_dir()?,
                config::development_home(),
            ));
            app.manage(Arc::new(questions::Questions::default()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            import_log,
            import_link,
            list_replays,
            preview_replay_deletion,
            delete_replay,
            rename_replay,
            open_replay,
            open_data_directory,
            get_storage,
            choose_data_directory,
            migrate_data,
            cancel_data_migration,
            majsoul_status,
            login_majsoul,
            logout_majsoul,
            analyze_game,
            cancel_analysis,
            ask,
            cancel_question,
            list_sessions,
            delete_session,
            get_session,
            rename_session,
            continue_session,
            retry_session,
            open_session_game,
            set_session_position,
            import_session,
            export_session,
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
    fn questions_can_use_unanalysed_and_non_decision_frames_even_during_analysis() {
        let (events, _) =
            replay::parse(include_str!("../../../fixtures/tenhou/ranked_game.json")).unwrap();
        let game = Game {
            id: 1,
            key: sessions::SessionGame::key(&events).unwrap(),
            mortal_supported: true,
            events,
            round_details: Vec::new(),
            reviews: Mutex::new(std::array::from_fn(|_| None)),
        };
        for (player, event) in [(0, 1), (0, 2), (0, 4), (1, 4)] {
            let context = game.context(player, event).unwrap();
            assert_eq!(context.evidence()["player"], player);
            assert_eq!(context.evidence()["event_index"], event);
            assert_eq!(context.evidence()["mortal"]["status"], "not_analyzed");
        }
        let _analyzing = game.reviews.lock().unwrap();
        assert!(game.context(0, 4).is_ok());
        assert!(game.context(4, 4).is_err());
        assert!(game.context(0, game.events.len()).is_err());
    }

    #[test]
    fn obsolete_document_id_cannot_read_new_game() {
        let state = Desktop::default();
        *state.game.lock().unwrap() = Some(Arc::new(Game {
            id: 2,
            key: "test".into(),
            mortal_supported: true,
            events: Vec::new(),
            round_details: Vec::new(),
            reviews: Mutex::new(std::array::from_fn(|_| None)),
        }));
        assert_eq!(state.game(1).err().unwrap().code, "stale_game");
        assert_eq!(state.game(2).unwrap().id, 2);
    }
}
