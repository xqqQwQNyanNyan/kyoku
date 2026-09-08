use super::*;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

fn context() -> AgentContext {
    let log =
        convlog::tenhou::Log::from_json_str(include_str!("../../fixtures/tenhou/ranked_game.json"))
            .unwrap();
    let events = convlog::tenhou_to_mjai(&log).unwrap();
    AgentContext::from_events(&events, PlayerIndex::try_from(0u8).unwrap(), 2).unwrap()
}

#[test]
fn progress_orders_model_and_tool_steps_and_stops_before_the_next_tool() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let saved = events.clone();
    let control = QuestionControl::new(move |event| {
        saved
            .lock()
            .unwrap()
            .push(serde_json::to_value(event).unwrap())
    });
    let context = context();
    let mut trace = Vec::new();
    let mut count = 0;
    let result = answer_controlled(
        context.evidence(),
        &[],
        false,
        "看看局面",
        |_, _| {
            count += 1;
            if count == 1 {
                Ok(
                    json!({"status":"completed","output":[{"type":"function_call","status":"completed","call_id":"review","name":"get_review","arguments":"{}"}]}),
                )
            } else {
                control.cancel();
                Ok(
                    json!({"status":"completed","output":[{"type":"function_call","status":"completed","call_id":"never","name":"analyze_defense","arguments":"{}"}]}),
                )
            }
        },
        &mut trace,
        &control,
    );
    assert!(matches!(result, Err(AgentError::Cancelled)));
    assert_eq!(
        *events.lock().unwrap(),
        vec![
            json!({"phase":"model","request":1}),
            json!({"phase":"tool","name":"get_review"}),
            json!({"phase":"model","request":2}),
        ]
    );
    assert_eq!(
        trace.iter().filter(|step| step["kind"] == "tool").count(),
        1
    );
}

#[test]
fn cancellation_before_network_keeps_the_question_without_changing_accepted_history() {
    let config = AgentConfig {
        endpoint: "http://127.0.0.1:1/responses",
        model: "test",
        api_key: None,
        options: Default::default(),
    };
    let mut session = AgentSession::with_context(&context(), &config).unwrap();
    let control = QuestionControl::default();
    control.cancel();
    assert!(matches!(
        session.ask_draft_with_control("停止的问题", &control),
        Err(AgentError::Cancelled)
    ));
    let archive = serde_json::to_value(session.archive()).unwrap();
    assert_eq!(archive["history"], json!([]));
    assert_eq!(archive["turns"][0]["question"], "停止的问题");
    assert_eq!(archive["turns"][0]["error"], "已停止本次回答");
    assert_eq!(archive["turns"][0]["answer"], Value::Null);
    SessionArchive::from_json(&archive.to_string()).unwrap();
}

#[test]
fn stopping_interrupts_waiting_for_headers_and_body_in_both_protocols() {
    for path in ["/responses", "/chat/completions"] {
        for partial_body in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint = format!("http://{}{path}", listener.local_addr().unwrap());
            let (ready_tx, ready_rx) = mpsc::channel();
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = BufReader::new(&mut stream);
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                    if line == "\r\n" {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':')
                        && name.eq_ignore_ascii_case("content-length")
                    {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                reader.read_exact(&mut vec![0; length]).unwrap();
                if partial_body {
                    stream
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 9999\r\n\r\n{\"output\":")
                        .unwrap();
                }
                ready_tx.send(()).unwrap();
                // 请求停止后连接必须关闭，不能继续等完整的 5 分钟超时。
                let closed = stream.read(&mut [0]);
                assert!(
                    matches!(closed, Ok(0))
                        || matches!(closed, Err(ref e) if matches!(e.kind(), std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted)),
                    "连接仍未关闭: {closed:?}"
                );
            });
            let control = QuestionControl::default();
            let worker_control = control.clone();
            let (done_tx, done_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let worker = thread::spawn(move || {
                let config = AgentConfig {
                    endpoint: &endpoint,
                    model: "test",
                    api_key: None,
                    options: Default::default(),
                };
                let mut session = AgentSession::with_context(&context(), &config).unwrap();
                let result = session.ask_draft_with_control("等待中的问题", &worker_control);
                assert!(matches!(result, Err(AgentError::Cancelled)));
                let archive = serde_json::to_value(session.archive()).unwrap();
                assert_eq!(archive["history"], json!([]));
                assert_eq!(archive["turns"][0]["usage"].as_array().unwrap().len(), 1);
                assert!(archive["turns"][0]["usage"][0]["input_tokens"].is_null());
                assert!(archive["turns"][0]["usage"][0]["cost"].is_null());
                assert_eq!(archive["turns"][0]["trace"][0]["kind"], "request");
                SessionArchive::from_json(&archive.to_string()).unwrap();
                done_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                drop(session);
            });
            ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            control.cancel();
            done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            server.join().unwrap();
            release_tx.send(()).unwrap();
            worker.join().unwrap();
        }
    }
}
