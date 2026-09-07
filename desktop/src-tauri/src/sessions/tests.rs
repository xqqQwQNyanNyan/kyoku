use super::*;
use kyoku::{
    agent::{AgentConfig, AgentContext},
    mahjong::player_index::PlayerIndex,
};

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
        },
    )
    .unwrap()
    .archive()
    .clone()
}

#[test]
fn restart_export_import_and_independent_sessions_preserve_context() {
    let directory = Directory::new();
    let store = SessionStore::new(directory.0.join("sessions"));
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
    let store = SessionStore::new(directory.0.join("sessions"));
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
    let store = SessionStore::new(directory.0.clone());
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
    let store = SessionStore::new(directory.0.clone());
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
                }
            )
            .is_err()
    );
    assert!(!store.get("old").unwrap().busy);
    assert_eq!(fs::read(directory.0.join("old.json")).unwrap(), original);
}

#[test]
fn session_operations_exclude_only_the_same_session_and_release_on_failure() {
    let directory = Directory::new();
    let store = SessionStore::new(directory.0.clone());
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
    };
    assert!(store.ask("missing", "无法读取", &config).is_err());
    store
        .create("missing", "失败后可重新创建".into(), context)
        .unwrap();
    assert!(!store.get("missing").unwrap().busy);
}

#[test]
fn unfinished_question_survives_restart_and_is_not_automatically_retried() {
    let directory = Directory::new();
    let store = SessionStore::new(directory.0.clone());
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
    let restarted = SessionStore::new(directory.0.clone());
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
    use std::io::{BufRead, BufReader, Read};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/responses", listener.local_addr().unwrap());
    let message = |text: &str| {
        json!({"status":"completed","output":[{
        "type":"message","role":"assistant","status":"completed","content":[{
            "type":"output_text","text":json!({"sections":[{"source":"limitation","text":text,"facts":[]}]}).to_string()
        }]
    }]}).to_string()
    };
    let responses = vec![
        (200, json!({"status":"completed","output":[{"type":"function_call","status":"completed","name":"get_review","arguments":"{}","call_id":"saved"}]}).to_string()),
        (200, message("原会话回答")),
        (503, "not-persisted-http-body".into()),
        (200, message("恢复后的回答")),
    ];
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
    let directory = Directory::new();
    let store = SessionStore::new(directory.0.clone());
    store
        .create("saved", "固定局面".into(), archive(&endpoint))
        .unwrap();
    let config = AgentConfig {
        endpoint: &endpoint,
        model: "test",
        api_key: None,
    };
    store.ask("saved", "最初的问题", &config).unwrap();
    let accepted =
        serde_json::to_value(store.get("saved").unwrap().document.archive).unwrap()["history"]
            .clone();
    drop(store);
    let store = SessionStore::new(directory.0.clone());
    assert!(store.ask("saved", "失败的问题", &config).is_err());
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
    store.ask("saved", "继续原来的讨论", &config).unwrap();
    let requests = server.join().unwrap();
    let mut expected = accepted.as_array().unwrap().clone();
    expected.push(json!({"role":"user","content":"继续原来的讨论"}));
    assert_eq!(requests[3]["input"], json!(expected));
    assert_eq!(store.list().unwrap().sessions.len(), 1);
}
