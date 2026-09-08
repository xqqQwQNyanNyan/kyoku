use serde_json::json;

#[test]
fn stopped_questions_survive_restart_and_late_cancellation_cannot_stop_the_retry() {
    let (endpoint, server) = server(vec![(200, message("重试草稿")), (200, message("重试完成"))]);
    let config = AgentConfig {
        endpoint: &endpoint,
        model: "test",
        api_key: None,
        options: Default::default(),
    };
    let directory = Directory::new();
    let store = new_store(directory.0.join("sessions"));
    let game = fixture_game();
    let context = AgentContext::from_events(&game.events, PlayerIndex::new(0).unwrap(), 2).unwrap();
    let questions = Arc::new(crate::questions::Questions::default());
    let running = questions
        .begin("one", "first", QuestionControl::default())
        .unwrap();
    assert!(
        questions
            .begin("one", "duplicate", QuestionControl::default())
            .is_err()
    );
    questions.cancel("one", "first").unwrap();
    let error = store
        .ask_with_control(
            "one",
            "保留停止时的局面",
            &config,
            Some(SessionSource {
                game: &game,
                context: &context,
                label: "test.json",
            }),
            &running.control,
        )
        .err()
        .unwrap();
    assert_eq!(error.message, "已停止本次回答");
    drop(running);
    drop(store);

    let store = new_store(directory.0.join("sessions"));
    let stopped = store.get("one").unwrap();
    assert!(!stopped.busy);
    assert!(stopped.document.pending_question.is_none());
    let archive = serde_json::to_value(&stopped.document.archive).unwrap();
    assert_eq!(archive["history"], json!([]));
    assert_eq!(archive["turns"][0]["error"], "已停止本次回答");
    store
        .set_position(
            "one",
            &game.key,
            SessionPosition {
                player: 1,
                event_index: 4,
            },
        )
        .unwrap();

    let retry = questions
        .begin("one", "second", QuestionControl::default())
        .unwrap();
    questions.cancel("one", "first").unwrap();
    questions.cancel("another-session", "second").unwrap();
    let completed = store
        .retry_with_control("one", Some(0), &config, &retry.control)
        .unwrap();
    let archive = serde_json::to_value(completed.document.archive).unwrap();
    assert!(
        archive["turns"][1]["answer"]
            .as_str()
            .unwrap()
            .contains("重试完成")
    );
    assert_eq!(archive["turns"][1]["evidence"]["player"], 0);
    assert_eq!(archive["turns"][1]["evidence"]["event_index"], 2);
    assert_eq!(server.join().unwrap().len(), 2);
    drop(retry);
    assert!(
        questions
            .begin("one", "third", QuestionControl::default())
            .is_ok()
    );
}

fn saved_game_session(store: &SessionStore, id: &str, game: &SessionGame) {
    store
        .create(
            id,
            "待删除牌谱".into(),
            archive("http://localhost/responses"),
        )
        .unwrap();
    let mut document = store.get(id).unwrap().document;
    document.version = 2;
    document.game = Some(game.clone());
    document.position = Some(SessionPosition {
        player: 0,
        event_index: 2,
    });
    store.save(&document).unwrap();
}

#[test]
fn deleting_one_session_keeps_replay_and_other_sessions_and_rejects_busy_or_invalid_ids() {
    let directory = Directory::new();
    let store = new_store(directory.0.clone());
    let game = fixture_game();
    saved_game_session(&store, "one", &game);
    saved_game_session(&store, "two", &game);
    let operation = store.begin("one").unwrap();
    assert_eq!(store.delete("one").err().unwrap().code, "busy");
    drop(operation);
    assert!(store.delete("../one").is_err());
    store.delete("one").unwrap();
    assert!(store.get("one").is_err());
    assert!(store.get("two").is_ok());
    assert!(store.library.get(&game.key).is_ok());
    assert!(store.delete("one").is_err());
}

#[test]
fn replay_deletion_checks_confirmed_sessions_and_keeps_unrelated_history() {
    let directory = Directory::new();
    let store = new_store(directory.0.clone());
    let game = fixture_game();
    saved_game_session(&store, "one", &game);
    let preview = store.preview_replay_deletion(&game.key).unwrap();
    assert_eq!(preview.name, "待删除牌谱");
    assert_eq!(preview.session_ids, ["one"]);
    saved_game_session(&store, "two", &game);
    assert_eq!(
        store
            .delete_replay(&game.key, preview.session_ids)
            .err()
            .unwrap()
            .code,
        "session_changed"
    );
    let ids = store
        .preview_replay_deletion(&game.key)
        .unwrap()
        .session_ids;
    let operation = store.begin("new-pending").unwrap();
    assert_eq!(
        store
            .delete_replay(&game.key, ids.clone())
            .err()
            .unwrap()
            .code,
        "busy"
    );
    drop(operation);
    store
        .create(
            "unrelated",
            "独立记录".into(),
            archive("http://localhost/responses"),
        )
        .unwrap();
    // 旧版内嵌牌谱也在同一删除范围内。
    let embedded = store.open_game("two").unwrap();
    let mut value = serde_json::to_value(embedded).unwrap();
    value["version"] = json!(2);
    fs::write(store.path("two").unwrap(), value.to_string()).unwrap();
    let exports = Directory::new();
    let exported = fs::read_to_string(store.export("one", &exports.0).unwrap()).unwrap();
    let result = store.delete_replay(&game.key, ids).unwrap();
    assert!(result.error.is_none());
    assert!(result.replay_deleted);
    assert_eq!(result.session_ids, ["one", "two"]);
    assert!(store.get("unrelated").is_ok());
    assert!(store.get("one").is_err());
    assert!(store.get("two").is_err());
    assert!(store.library.get(&game.key).is_err());
    let imported = store.import(&exported).unwrap();
    assert!(store.open_game(&imported.document.id).is_ok());
}

#[test]
fn unreadable_session_blocks_cascade_without_deleting_anything() {
    let directory = Directory::new();
    let store = new_store(directory.0.clone());
    let game = fixture_game();
    saved_game_session(&store, "one", &game);
    fs::write(store.path("broken").unwrap(), "broken").unwrap();
    assert!(store.preview_replay_deletion(&game.key).is_err());
    assert!(store.delete_replay(&game.key, vec!["one".into()]).is_err());
    assert!(store.get("one").is_ok());
    assert!(store.library.get(&game.key).is_ok());
}

#[test]
fn mismatched_session_filename_cannot_offer_deletion_of_another_record() {
    let directory = Directory::new();
    let store = new_store(directory.0.clone());
    let game = fixture_game();
    saved_game_session(&store, "one", &game);
    fs::copy(
        store.path("one").unwrap(),
        store.path("wrong-name").unwrap(),
    )
    .unwrap();
    let list = store.list().unwrap();
    assert_eq!(list.sessions.len(), 1);
    assert_eq!(list.warnings.len(), 1);
    assert!(store.preview_replay_deletion(&game.key).is_err());
    assert!(store.get("one").is_ok());
}

#[cfg(unix)]
#[test]
fn replay_delete_io_failure_reports_removed_sessions_and_keeps_replay() {
    let directory = Directory::new();
    let store = new_store(directory.0.clone());
    let game = fixture_game();
    saved_game_session(&store, "one", &game);
    let marker = directory
        .0
        .join("data/replays/imported")
        .join(format!("{}.deleted", game.key));
    std::os::unix::fs::symlink(directory.0.join("missing"), marker).unwrap();
    let result = store.delete_replay(&game.key, vec!["one".into()]).unwrap();
    assert!(result.error.is_some());
    assert!(!result.replay_deleted);
    assert_eq!(result.session_ids, ["one"]);
    assert!(store.library.get(&game.key).is_ok());
}

#[test]
fn session_titles_are_bounded_and_renames_preserve_saved_context() {
    assert_eq!(default_title(&"🀄".repeat(40)).chars().count(), 32);
    assert_eq!(default_title("  第一行\n第二行  "), "第一行 第二行");
    let directory = Directory::new();
    let store = new_store(directory.0.clone());
    store
        .create(
            "title",
            "牌谱".into(),
            archive("http://localhost/responses"),
        )
        .unwrap();
    let before = serde_json::to_value(store.get("title").unwrap().document.archive).unwrap();
    let renamed = store
        .rename("title", &format!("  {}  ", "🀄".repeat(32)))
        .unwrap();
    assert_eq!(renamed.document.title, "🀄".repeat(32));
    let bytes = fs::read(directory.0.join("title.json")).unwrap();
    for invalid in [
        "".to_owned(),
        "   ".into(),
        "长".repeat(33),
        "一\n二".into(),
    ] {
        assert!(store.rename("title", &invalid).is_err());
        assert_eq!(fs::read(directory.0.join("title.json")).unwrap(), bytes);
    }
    let reopened = new_store(directory.0.clone()).get("title").unwrap();
    assert_eq!(reopened.document.title, "🀄".repeat(32));
    assert_eq!(
        serde_json::to_value(reopened.document.archive).unwrap(),
        before
    );
    let mut old: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    old["title"] = json!("旧".repeat(100));
    assert_eq!(parse(&old.to_string()).unwrap().title.chars().count(), 32);
    let _operation = store.begin("title").unwrap();
    assert!(store.rename("title", "不能覆盖正在写入的会话").is_err());
}

#[test]
fn local_sessions_share_replays_and_exports_restore_without_the_original_library() {
    let directory = Directory::new();
    let store = new_store(directory.0.clone());
    let mut game = fixture_game();
    let (_, data) =
        crate::replay::parse(include_str!("../../../../fixtures/tenhou/ranked_game.json")).unwrap();
    game.round_details = data.round_details();
    store
        .create("one", "对局".into(), archive("http://localhost/responses"))
        .unwrap();
    let mut original = store.get("one").unwrap().document;
    original.version = 2;
    original.game = Some(game.clone());
    original.position = Some(SessionPosition {
        player: 0,
        event_index: 2,
    });
    store.save(&original).unwrap();
    original.id = "two".into();
    store.save(&original).unwrap();
    let local = fs::read_to_string(directory.0.join("one.json")).unwrap();
    let compact: serde_json::Value = serde_json::from_str(&local).unwrap();
    assert_eq!(compact["version"], 3);
    assert_eq!(compact["game"], json!({"key":game.key}));
    assert!(store.import(&local).is_err());
    assert_eq!(store.library.list().unwrap().warnings.len(), 0);
    let replay_files = directory.0.join("data/replays/imported");
    assert_eq!(fs::read_dir(&replay_files).unwrap().count(), 1);
    store.library.rename(&game.key, "已改名的牌谱").unwrap();
    for id in ["one", "two"] {
        let opened = store.open_game(id).unwrap();
        assert_eq!(opened.context_label, "已改名的牌谱");
        assert_eq!(opened.game.unwrap().key, game.key);
    }
    let exports = Directory::new();
    let export = store.export("one", &exports.0).unwrap();
    let portable = fs::read_to_string(&export).unwrap();
    let portable_json: serde_json::Value = serde_json::from_str(&portable).unwrap();
    assert_eq!(portable_json["version"], 2);
    assert_eq!(
        portable_json["game"]["round_details"],
        serde_json::to_value(&game.round_details).unwrap()
    );
    assert!(
        !portable_json["game"]["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let other_directory = Directory::new();
    let other = new_store(other_directory.0.clone());
    let imported = other.import(&portable).unwrap();
    assert_eq!(
        serde_json::to_value(
            other
                .open_game(&imported.document.id)
                .unwrap()
                .game
                .unwrap()
                .round_details
        )
        .unwrap(),
        serde_json::to_value(&game.round_details).unwrap()
    );
    fs::remove_file(replay_files.join(format!("{}.json", game.key))).unwrap();
    assert_eq!(
        other
            .open_game(&imported.document.id)
            .unwrap()
            .game
            .unwrap()
            .events,
        game.events
    );
    assert_eq!(store.get("one").unwrap().document.title, "新会话");
    assert_eq!(store.list().unwrap().sessions.len(), 2);
    assert_eq!(store.open_game("one").err().unwrap().code, "replay_missing");
    assert!(store.export("one", &directory.0).is_err());
    assert_eq!(
        fs::read_to_string(directory.0.join("one.json")).unwrap(),
        local
    );
    store
        .library
        .save(&game, "重新导入", ReplayOrigin::File)
        .unwrap();
    assert_eq!(
        store.open_game("one").unwrap().game.unwrap().events,
        game.events
    );
}

#[test]
fn embedded_session_migration_saves_replay_first_and_preserves_failures() {
    let directory = Directory::new();
    let store = new_store(directory.0.clone());
    store
        .create(
            "old",
            "旧牌谱".into(),
            archive("http://localhost/responses"),
        )
        .unwrap();
    let mut document = store.get("old").unwrap().document;
    let game = fixture_game();
    document.version = 2;
    document.game = Some(game.clone());
    document.position = Some(SessionPosition {
        player: 0,
        event_index: 2,
    });
    let bytes = serde_json::to_vec(&document).unwrap();
    fs::write(directory.0.join("old.json"), &bytes).unwrap();
    fs::write(directory.0.join("broken.json"), "broken").unwrap();
    fs::write(directory.0.join("data"), "阻止创建牌谱目录").unwrap();
    assert_eq!(store.migrate_embedded().unwrap().len(), 2);
    assert_eq!(fs::read(directory.0.join("old.json")).unwrap(), bytes);
    fs::remove_file(directory.0.join("data")).unwrap();
    assert_eq!(store.migrate_embedded().unwrap().len(), 1);
    assert!(
        store
            .get("old")
            .unwrap()
            .document
            .game
            .unwrap()
            .events
            .is_empty()
    );
    let restored = store.open_game("old").unwrap();
    assert_eq!(restored.game.unwrap().events, game.events);
    assert_eq!(
        serde_json::to_value(restored.archive).unwrap(),
        serde_json::to_value(document.archive).unwrap()
    );
    assert_eq!(
        fs::read_to_string(directory.0.join("broken.json")).unwrap(),
        "broken"
    );
}
fn server(
    responses: Vec<(u16, String)>,
) -> (String, std::thread::JoinHandle<Vec<serde_json::Value>>) {
    use std::io::{BufRead, BufReader, Read};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/responses", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut requests = Vec::<serde_json::Value>::new();
        for (status, body) in responses {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "等待测试请求超时");
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("{e}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(&mut stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut length = 0;
            loop {
                line.clear();
                assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                if line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse().unwrap();
                }
            }
            let mut bytes = vec![0; length];
            reader.read_exact(&mut bytes).unwrap();
            requests.push(serde_json::from_slice(&bytes).unwrap());
            write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
        requests
    });
    (endpoint, server)
}

fn message(text: &str) -> String {
    json!({"status":"completed","output":[{
        "type":"message","role":"assistant","status":"completed","content":[{
            "type":"output_text","text":text
        }]
    }]})
    .to_string()
}

use super::*;

impl SessionStore {
    fn ask(
        &self,
        id: &str,
        question: &str,
        config: &kyoku::agent::AgentConfig<'_>,
        source: Option<SessionSource<'_>>,
    ) -> Result<SessionView, UiError> {
        self.ask_with_control(id, question, config, source, &QuestionControl::default())
    }

    fn retry(
        &self,
        id: &str,
        turn: Option<usize>,
        config: &kyoku::agent::AgentConfig<'_>,
    ) -> Result<SessionView, UiError> {
        self.retry_with_control(id, turn, config, &QuestionControl::default())
    }
}
use kyoku::{
    agent::{AgentConfig, AgentContext},
    mahjong::player_index::PlayerIndex,
};

fn new_store(directory: PathBuf) -> SessionStore {
    let library = Arc::new(crate::library::ReplayLibrary::new(directory.join("data")));
    SessionStore::new(directory, library)
}

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "kyoku-sessions-test-{}-{}-{}",
            std::process::id(),
            now(),
            FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn archive(endpoint: &str) -> SessionArchive {
    let log = convlog::tenhou::Log::from_json_str(include_str!(
        "../../../../fixtures/tenhou/ranked_game.json"
    ))
    .unwrap();
    let events = convlog::tenhou_to_mjai(&log).unwrap();
    let context = AgentContext::from_events(&events, PlayerIndex::new(0).unwrap(), 2).unwrap();
    AgentSession::with_context(
        &context,
        &AgentConfig {
            endpoint,
            model: "test",
            api_key: None,
            options: Default::default(),
        },
    )
    .unwrap()
    .archive()
    .clone()
}

#[test]
fn restart_export_import_and_independent_sessions_preserve_context() {
    let directory = Directory::new();
    let store = new_store(directory.0.join("sessions"));
    let original = archive("http://localhost/responses");
    store
        .create("one", "牌谱 A · 玩家 0 · G2".into(), original.clone())
        .unwrap();
    store
        .create("two", "牌谱 B · 玩家 1 · G20".into(), original)
        .unwrap();
    assert!(
        store
            .create(
                "one",
                "重复编号".into(),
                archive("http://localhost/responses")
            )
            .is_err()
    );
    drop(store);
    let store = new_store(directory.0.join("sessions"));
    assert_eq!(store.list().unwrap().sessions.len(), 2);
    let restored = store.get("one").unwrap();
    assert_eq!(restored.document.context_label, "牌谱 A · 玩家 0 · G2");
    assert_eq!(restored.document.archive.evidence()["event_index"], 2);
    let exported = store.export("one", &directory.0).unwrap();
    let json = fs::read_to_string(exported).unwrap();
    assert!(!json.contains("api_key"));
    let imported = store.import(&json).unwrap();
    assert_ne!(imported.document.id, "one");
    assert_eq!(
        serde_json::to_value(imported.document.archive).unwrap(),
        serde_json::to_value(restored.document.archive).unwrap()
    );
    assert_eq!(store.list().unwrap().sessions.len(), 3);
    assert_eq!(
        store.get("two").unwrap().document.context_label,
        "牌谱 B · 玩家 1 · G20"
    );
}

#[test]
fn invalid_import_path_and_corrupt_files_do_not_replace_saved_sessions() {
    let directory = Directory::new();
    let store = new_store(directory.0.clone());
    store
        .create(
            "valid",
            "原会话".into(),
            archive("http://localhost/responses"),
        )
        .unwrap();
    let original = fs::read(directory.0.join("valid.json")).unwrap();
    assert!(store.get("../valid").is_err());
    assert!(store.import("{}").is_err());
    let mut tampered: serde_json::Value = serde_json::from_slice(&original).unwrap();
    tampered["id"] = serde_json::json!("../overwrite");
    assert!(store.import(&tampered.to_string()).is_err());
    assert_eq!(fs::read(directory.0.join("valid.json")).unwrap(), original);
    fs::write(directory.0.join("broken.json"), "{").unwrap();
    let list = store.list().unwrap();
    assert_eq!(list.sessions.len(), 1);
    assert_eq!(list.warnings.len(), 1);
    assert!(directory.0.join("broken.json").exists());
}

#[test]
fn history_browsing_keeps_saved_answers_even_when_tool_results_no_longer_match() {
    let directory = Directory::new();
    let store = new_store(directory.0.clone());
    let mut saved = serde_json::to_value(archive("http://localhost/responses")).unwrap();
    saved["history"] = serde_json::json!([
        {"type":"function_call","status":"completed","call_id":"old-score",
            "name":"analyze_score_targets","arguments":"{\"target\":1}"},
        {"type":"function_call_output","call_id":"old-score","output":
            "{\"ok\":true,\"analysis\":{\"old_result\":true}}"}
    ]);
    saved["turns"] = serde_json::json!([
        {"question":"以前的问题","answer":"已经保存的回答","error":null,"trace":[]}
    ]);
    let old = serde_json::from_value(saved.clone()).unwrap();
    store.create("old", "旧会话".into(), old).unwrap();
    let original = fs::read(directory.0.join("old.json")).unwrap();
    let list = store.list().unwrap();
    assert_eq!(list.sessions.len(), 1);
    assert!(list.warnings.is_empty());
    assert_eq!(
        serde_json::to_value(store.get("old").unwrap().document.archive).unwrap(),
        saved
    );
    let export_directory = directory.0.join("exports");
    fs::create_dir(&export_directory).unwrap();
    let exported = store.export("old", &export_directory).unwrap();
    let json = fs::read_to_string(&exported).unwrap();
    assert!(json.contains("已经保存的回答"));
    // 展示旧记录不意味着允许把不一致的证据作为新导入或续聊的上下文。
    assert!(store.import(&json).is_err());
    assert!(
        store
            .ask(
                "old",
                "继续",
                &AgentConfig {
                    endpoint: "http://localhost/responses",
                    model: "test",
                    api_key: None,
                    options: Default::default(),
                },
                None
            )
            .is_err()
    );
    assert!(!store.get("old").unwrap().busy);
    assert_eq!(fs::read(directory.0.join("old.json")).unwrap(), original);
}

#[test]
fn session_operations_exclude_only_the_same_session_and_release_on_failure() {
    let directory = Directory::new();
    let store = new_store(directory.0.clone());
    let context = archive("http://localhost/responses");
    store
        .create("one", "会话一".into(), context.clone())
        .unwrap();
    let operation = store.begin("one").unwrap();
    assert!(store.begin("one").is_err());
    assert!(store.get("one").unwrap().busy);
    store
        .create("two", "会话二".into(), context.clone())
        .unwrap();
    assert!(!store.get("two").unwrap().busy);
    assert_eq!(store.list().unwrap().sessions.len(), 2);
    drop(operation);
    assert!(!store.get("one").unwrap().busy);

    let config = AgentConfig {
        endpoint: "http://localhost/responses",
        model: "test",
        api_key: None,
        options: Default::default(),
    };
    assert!(store.ask("missing", "无法读取", &config, None).is_err());
    store
        .create("missing", "失败后可重新创建".into(), context)
        .unwrap();
    assert!(!store.get("missing").unwrap().busy);
}

#[test]
fn unfinished_question_survives_restart_and_is_not_automatically_retried() {
    let directory = Directory::new();
    let store = new_store(directory.0.clone());
    store
        .create(
            "pending",
            "原会话".into(),
            archive("http://localhost/responses"),
        )
        .unwrap();
    let mut doc = store.get("pending").unwrap().document;
    doc.pending_question = Some("还没有回答的问题".into());
    store.save(&doc).unwrap();
    lock(&store.gate).unwrap().insert("pending".into());
    assert!(store.get("pending").unwrap().busy);
    drop(store);
    let restarted = new_store(directory.0.clone());
    let session = restarted.get("pending").unwrap();
    assert!(!session.busy);
    assert_eq!(
        session.document.pending_question.as_deref(),
        Some("还没有回答的问题")
    );
    assert!(restarted.list().unwrap().sessions[0].interrupted);
}

#[test]
fn answers_and_failed_traces_are_saved_before_reopening_and_continuing() {
    use serde_json::json;
    let responses = vec![
        (200, json!({"status":"completed","output":[{"type":"function_call","status":"completed","name":"get_review","arguments":"{}","call_id":"saved"}]}).to_string()),
        (200, message("原会话草稿")),
        (200, message("原会话回答")),
        (503, "not-persisted-http-body".into()),
        (200, message("恢复后的草稿")),
        (200, message("恢复后的回答")),
    ];
    let (endpoint, server) = server(responses);
    let directory = Directory::new();
    let store = new_store(directory.0.clone());
    store
        .create("saved", "固定局面".into(), archive(&endpoint))
        .unwrap();
    let config = AgentConfig {
        endpoint: &endpoint,
        model: "test",
        api_key: None,
        options: Default::default(),
    };
    store.ask("saved", "最初的问题", &config, None).unwrap();
    let accepted =
        serde_json::to_value(store.get("saved").unwrap().document.archive).unwrap()["history"]
            .clone();
    drop(store);
    let store = new_store(directory.0.clone());
    assert!(store.ask("saved", "失败的问题", &config, None).is_err());
    let failed = serde_json::to_value(store.get("saved").unwrap().document).unwrap();
    assert!(failed["pending_question"].is_null());
    assert!(
        failed["archive"]["turns"][1]["error"]
            .as_str()
            .unwrap()
            .contains("503")
    );
    assert_eq!(failed["archive"]["history"], accepted);
    assert!(store.list().unwrap().sessions[0].failed);
    assert!(!failed.to_string().contains("not-persisted-http-body"));
    store.ask("saved", "继续原来的讨论", &config, None).unwrap();
    let requests = server.join().unwrap();
    // 存档保留完整证据，线上请求只接续核查后的问答正文。
    let continuation = requests[4]["input"].as_array().unwrap();
    assert!(
        continuation
            .iter()
            .any(|item| item["role"] == "assistant" && item["content"] == "原会话回答")
    );
    assert!(
        continuation
            .iter()
            .any(|item| item["role"] == "user" && item["content"] == "继续原来的讨论")
    );
    assert!(!requests[4]["input"].to_string().contains("function_call"));
    assert_eq!(store.list().unwrap().sessions.len(), 1);
}

impl SessionStore {
    fn create(&self, id: &str, label: String, archive: SessionArchive) -> Result<(), UiError> {
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
            game: None,
            position: None,
        })
    }
}

fn fixture_game() -> SessionGame {
    let (events, _) =
        crate::replay::parse(include_str!("../../../../fixtures/tenhou/ranked_game.json")).unwrap();
    SessionGame {
        key: SessionGame::key(&events).unwrap(),
        events,
        round_details: Vec::new(),
    }
}

#[test]
fn game_sessions_preserve_turn_snapshots_browsing_position_and_portable_replay() {
    let (endpoint, server) = server(vec![
        (200, message("第一处草稿")),
        (200, message("第一处")),
        (200, message("第二处草稿")),
        (200, message("第二处")),
        (200, message("独立会话草稿")),
        (200, message("独立会话")),
    ]);
    let config = AgentConfig {
        endpoint: &endpoint,
        model: "test",
        api_key: None,
        options: Default::default(),
    };
    let game = fixture_game();
    let first = AgentContext::from_events(&game.events, PlayerIndex::new(0).unwrap(), 1).unwrap();
    let next = AgentContext::from_events(&game.events, PlayerIndex::new(1).unwrap(), 4).unwrap();
    let directory = Directory::new();
    let store = new_store(directory.0.join("sessions"));
    for (id, question, context) in [
        ("one", "未分析也能问", &first),
        ("one", "换位置继续", &next),
        ("two", "另一段讨论", &first),
    ] {
        store
            .ask(
                id,
                question,
                &config,
                Some(SessionSource {
                    game: &game,
                    context,
                    label: "test.json",
                }),
            )
            .unwrap();
    }
    let original = store.get("one").unwrap().document;
    let value = serde_json::to_value(&original.archive).unwrap();
    assert_eq!(value["turns"].as_array().unwrap().len(), 2);
    assert_eq!(value["turns"][0]["evidence"]["event_index"], 1);
    assert_eq!(value["turns"][1]["evidence"]["event_index"], 4);
    assert_eq!(value["turns"][1]["evidence"]["player"], 1);
    let position = SessionPosition {
        player: 2,
        event_index: 10,
    };
    store.set_position("one", &game.key, position).unwrap();
    assert_eq!(
        serde_json::to_value(store.get("one").unwrap().document.archive).unwrap(),
        value
    );
    assert!(store.set_position("one", "another-game", position).is_err());
    assert!(
        store
            .set_position(
                "one",
                &game.key,
                SessionPosition {
                    player: 4,
                    event_index: 10
                }
            )
            .is_err()
    );
    assert!(
        store
            .set_position(
                "one",
                &game.key,
                SessionPosition {
                    player: 0,
                    event_index: game.events.len()
                }
            )
            .is_err()
    );
    drop(store);
    let reopened = new_store(directory.0.join("sessions"));
    let restored = reopened.open_game("one").unwrap();
    assert!(restored.position == Some(position));
    assert_eq!(restored.game.as_ref().unwrap().events, game.events);
    let list = reopened.list().unwrap();
    assert_eq!(list.sessions.len(), 2);
    assert!(
        list.sessions
            .iter()
            .all(|session| session.game_key.as_deref() == Some(&game.key))
    );
    let exported = reopened.export("one", &directory.0).unwrap();
    let imported = reopened
        .import(&fs::read_to_string(exported).unwrap())
        .unwrap()
        .document;
    assert_ne!(imported.id, "one");
    assert!(imported.position == Some(position));
    assert_eq!(imported.game.as_ref().unwrap().events, game.events);
    let imported_replay = crate::replay::replay(&imported.game.unwrap().events).unwrap();
    assert!(
        imported_replay
            .frames
            .iter()
            .any(|frame| frame.event_index == position.event_index)
    );
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 6);
    // 保存完整牌谱不改变网络投影：每份快照仍只提供当前观察玩家的手牌。
    for request in requests {
        assert!(!request.to_string().contains("tehais"));
        assert!(!request.to_string().contains(&game.key));
        assert!(!request.to_string().contains("test.json"));
    }
}

#[test]
fn retries_use_original_question_snapshots_even_after_browsing_and_other_questions() {
    let (endpoint, server) = server(vec![
        (503, "failed".into()),
        (200, message("新问题草稿")),
        (200, message("新问题成功")),
        (200, message("旧问题重试草稿")),
        (200, message("旧问题重试成功")),
        (200, message("继续浏览位置草稿")),
        (200, message("继续浏览位置")),
        (200, message("未完成问题恢复草稿")),
        (200, message("未完成问题恢复")),
    ]);
    let config = AgentConfig {
        endpoint: &endpoint,
        model: "test",
        api_key: None,
        options: Default::default(),
    };
    let game = fixture_game();
    let first = AgentContext::from_events(&game.events, PlayerIndex::new(0).unwrap(), 2).unwrap();
    let next = AgentContext::from_events(&game.events, PlayerIndex::new(1).unwrap(), 4).unwrap();
    let directory = Directory::new();
    let store = new_store(directory.0.clone());
    assert!(
        store
            .ask(
                "one",
                "原问题",
                &config,
                Some(SessionSource {
                    game: &game,
                    context: &first,
                    label: "game"
                })
            )
            .is_err()
    );
    store
        .ask(
            "one",
            "新局面的问题",
            &config,
            Some(SessionSource {
                game: &game,
                context: &next,
                label: "game",
            }),
        )
        .unwrap();
    let position = SessionPosition {
        player: 2,
        event_index: 10,
    };
    store.set_position("one", &game.key, position).unwrap();
    store.retry("one", Some(0), &config).unwrap();
    assert!(store.get("one").unwrap().document.position == Some(position));
    assert!(store.retry("one", Some(1), &config).is_err());
    assert!(store.retry("one", None, &config).is_err());
    store.ask("one", "继续浏览位置", &config, None).unwrap();
    let mut pending = store.get("one").unwrap().document;
    pending.pending_question = Some("退出前的问题".into());
    store.save(&pending).unwrap();
    store
        .set_position(
            "one",
            &game.key,
            SessionPosition {
                player: 3,
                event_index: 12,
            },
        )
        .unwrap();
    store.retry("one", None, &config).unwrap();
    let archive = serde_json::to_value(store.get("one").unwrap().document.archive).unwrap();
    assert_eq!(archive["turns"][2]["evidence"], first.evidence().clone());
    assert_eq!(archive["turns"][3]["evidence"]["event_index"], 10);
    assert_eq!(archive["turns"][4]["evidence"]["event_index"], 10);
    assert_eq!(archive["turns"][4]["question"], "退出前的问题");
    assert_eq!(server.join().unwrap().len(), 9);
}

#[test]
fn a_session_cannot_be_reused_for_another_game_and_invalid_import_keeps_original() {
    let (endpoint, server) = server(vec![(200, message("已有草稿")), (200, message("已有回答"))]);
    let config = AgentConfig {
        endpoint: &endpoint,
        model: "test",
        api_key: None,
        options: Default::default(),
    };
    let game = fixture_game();
    let context = AgentContext::from_events(&game.events, PlayerIndex::new(0).unwrap(), 2).unwrap();
    let directory = Directory::new();
    let store = new_store(directory.0.clone());
    store
        .ask(
            "one",
            "问题",
            &config,
            Some(SessionSource {
                game: &game,
                context: &context,
                label: "game",
            }),
        )
        .unwrap();
    let original = fs::read(directory.0.join("one.json")).unwrap();
    let mut another = game.clone();
    another.events.pop();
    another.key = SessionGame::key(&another.events).unwrap();
    assert!(
        store
            .ask(
                "one",
                "错误牌谱",
                &config,
                Some(SessionSource {
                    game: &another,
                    context: &context,
                    label: "other"
                })
            )
            .is_err()
    );
    assert_eq!(fs::read(directory.0.join("one.json")).unwrap(), original);
    let mut json: serde_json::Value = serde_json::from_slice(&original).unwrap();
    json["game"]["key"] = json!("forged-key");
    assert!(store.import(&json.to_string()).is_err());
    assert_eq!(fs::read(directory.0.join("one.json")).unwrap(), original);
    assert_eq!(server.join().unwrap().len(), 2);
}
