#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod library;
mod log_link;
mod majsoul;
mod replay;
mod sessions;
mod settings;

use convlog::Event;
use kyoku::{
    agent::{AgentContext, review_evidence},
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
    majsoul: Arc<Mutex<majsoul::Account>>,
}

struct Game {
    id: u64,
    key: String,
    mortal_supported: bool,
    events: Vec<Event>,
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
    let library = app.state::<Arc<library::ReplayLibrary>>().inner().clone();
    let (events, data, name) = tauri::async_runtime::spawn_blocking(move || {
        let (events, data) = library::parse_input(&json)?;
        let name = library.save(
            &sessions::SessionGame {
                key: sessions::SessionGame::key(&events)?,
                events: events.clone(),
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
    let library = app.state::<Arc<library::ReplayLibrary>>().inner().clone();
    let (events, data, name) = tauri::async_runtime::spawn_blocking(move || {
        let json = log_link::download(&link, &account)?;
        let (events, data) = replay::parse(&json)?;
        let name = library.save(
            &sessions::SessionGame {
                key: sessions::SessionGame::key(&events)?,
                events: events.clone(),
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
        let migration = app.state::<sessions::SessionStore>().migrate_embedded();
        let mut list = app.state::<Arc<library::ReplayLibrary>>().list()?;
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
async fn rename_replay(
    key: String,
    name: String,
    app: tauri::AppHandle,
) -> Result<library::ReplaySummary, UiError> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Arc<library::ReplayLibrary>>()
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
        let saved = app.state::<Arc<library::ReplayLibrary>>().get(&key)?;
        let data = replay::replay(&saved.game.events)?;
        Ok::<_, UiError>((saved.game.events, data, saved.name))
    })
    .await
    .map_err(|_| UiError::new("task", "打开已保存牌谱任务异常结束"))??;
    finish_import(&state, id, events, data, name)
}

#[tauri::command]
async fn open_data_directory(app: tauri::AppHandle) -> Result<(), UiError> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Arc<library::ReplayLibrary>>().open_directory()
    })
    .await
    .map_err(|_| UiError::new("task", "打开数据文件夹任务异常结束"))?
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
    context_label: String,
}

#[tauri::command]
async fn ask(
    question: Question,
    state: tauri::State<'_, Desktop>,
    app: tauri::AppHandle,
) -> Result<sessions::SessionView, UiError> {
    PlayerIndex::try_from(question.player)
        .map_err(|_| UiError::new("player", "玩家编号必须为 0..3"))?;
    if question.conversation_id.is_empty() || question.conversation_id.len() > 128 {
        return Err(UiError::new("conversation", "问答会话编号无效"));
    }
    let game = state.game(question.id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let context = game.context(question.player, question.event_index)?;
        let config = app.state::<settings::SettingsStore>().load()?;
        let saved_game = sessions::SessionGame {
            key: game.key.clone(),
            events: game.events.clone(),
        };
        app.state::<sessions::SessionStore>().ask(
            &question.conversation_id,
            &question.text,
            &config.borrowed(),
            Some(sessions::SessionSource {
                game: &saved_game,
                label: &question.context_label,
                context: &context,
            }),
        )
    })
    .await
    .map_err(|_| UiError::new("task", "问答任务异常结束"))?
}

#[tauri::command]
async fn list_sessions(app: tauri::AppHandle) -> Result<sessions::SessionList, UiError> {
    tauri::async_runtime::spawn_blocking(move || app.state::<sessions::SessionStore>().list())
        .await
        .map_err(|_| UiError::new("task", "读取历史会话任务异常结束"))?
}

#[tauri::command]
async fn get_session(id: String, app: tauri::AppHandle) -> Result<sessions::SessionView, UiError> {
    tauri::async_runtime::spawn_blocking(move || app.state::<sessions::SessionStore>().get(&id))
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
        app.state::<sessions::SessionStore>().rename(&id, &title)
    })
    .await
    .map_err(|_| UiError::new("task", "修改会话标题任务异常结束"))?
}

#[tauri::command]
async fn continue_session(
    id: String,
    text: String,
    app: tauri::AppHandle,
) -> Result<sessions::SessionView, UiError> {
    tauri::async_runtime::spawn_blocking(move || {
        let config = app.state::<settings::SettingsStore>().load()?;
        app.state::<sessions::SessionStore>()
            .ask(&id, &text, &config.borrowed(), None)
    })
    .await
    .map_err(|_| UiError::new("task", "问答任务异常结束；问题已保存在历史会话中"))?
}

#[tauri::command]
async fn retry_session(
    id: String,
    turn: Option<usize>,
    app: tauri::AppHandle,
) -> Result<sessions::SessionView, UiError> {
    tauri::async_runtime::spawn_blocking(move || {
        let config = app.state::<settings::SettingsStore>().load()?;
        app.state::<sessions::SessionStore>()
            .retry(&id, turn, &config.borrowed())
    })
    .await
    .map_err(|_| UiError::new("task", "重试任务异常结束；问题已保存在历史会话中"))?
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
        app.state::<sessions::SessionStore>().open_game(&id)
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
        let data = replay::replay(&game.events)?;
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
        app.state::<sessions::SessionStore>()
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
        app.state::<sessions::SessionStore>().import(&json)
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
        app.state::<sessions::SessionStore>()
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
            let data_directory = app.path().app_data_dir()?;
            let library = Arc::new(library::ReplayLibrary::new(data_directory.clone()));
            library
                .initialize()
                .map_err(|error| std::io::Error::other(error.message))?;
            app.manage(settings::SettingsStore::new(
                app.path().app_config_dir()?,
                config::development_home(),
            ));
            app.manage(sessions::SessionStore::new(
                data_directory.join("sessions"),
                library.clone(),
            ));
            app.manage(library);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            import_log,
            import_link,
            list_replays,
            rename_replay,
            open_replay,
            open_data_directory,
            majsoul_status,
            login_majsoul,
            logout_majsoul,
            analyze_game,
            ask,
            list_sessions,
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
            reviews: Mutex::new(std::array::from_fn(|_| None)),
        }));
        assert_eq!(state.game(1).err().unwrap().code, "stale_game");
        assert_eq!(state.game(2).unwrap().id, 2);
    }
}
