use super::*;

fn completion(message: Value, finish: &str) -> Value {
    json!({"choices": [{"index": 0, "message": message, "finish_reason": finish}]})
}

fn tool(id: &str, arguments: &str) -> Value {
    json!({"id": id, "type": "function", "function": {"name": "get_review", "arguments": arguments}})
}

fn tool_message(calls: Vec<Value>) -> Value {
    json!({"role": "assistant", "content": null, "tool_calls": calls})
}

fn text_message(text: &str) -> Value {
    json!({"role": "assistant", "content": text})
}

fn valid_answer(text: &str) -> String {
    text.to_owned()
}

#[test]
fn chat_roundtrip_preserves_tool_groups_markdown_and_followups_without_repairs() {
    let mut calls = tool_message(vec![tool("a", "{}"), tool("b", "{}")]);
    calls["content"] = json!("读取局面证据。");
    calls["reasoning_content"] = json!("test-reasoning");
    let first = valid_answer("现有证据不能解释推荐原因。");
    let second = valid_answer("只能比较已提供的指标。");
    let (endpoint, handle) = server_at(
        "/v1/chat/completions",
        vec![
            (200, completion(calls.clone(), "tool_calls").to_string()),
            (200, completion(text_message(&first), "stop").to_string()),
            (
                200,
                completion(text_message("截断正文"), "length").to_string(),
            ),
            (
                200,
                completion(text_message("**普通 Markdown 回答**"), "stop").to_string(),
            ),
            (200, completion(text_message(&second), "stop").to_string()),
        ],
    );
    let mut session = AgentSession::new(
        &review(),
        &AgentConfig {
            endpoint: &endpoint,
            model: "test",
            api_key: None,
            options: Default::default(),
        },
    )
    .unwrap();
    assert_eq!(
        session.ask_draft("第一问").unwrap(),
        "现有证据不能解释推荐原因。"
    );
    let history = session.history.clone();
    assert!(matches!(
        session.ask_draft("失败问题"),
        Err(AgentError::OutputLimit)
    ));
    assert_eq!(session.history, history);
    assert_eq!(
        session.ask_draft("第二问").unwrap(),
        "**普通 Markdown 回答**"
    );
    session.ask_draft("第三问").unwrap();
    let requests = handle.join().unwrap();
    let request = &requests[0];
    assert_eq!(request["messages"][0]["role"], "system");
    assert_eq!(
        request["messages"][2],
        json!({"role":"user", "content":"第一问"})
    );
    assert_eq!(request["tools"][0]["function"]["name"], "get_review");
    assert_eq!(
        request["tools"][0]["function"]["parameters"]["additionalProperties"],
        false
    );
    assert_eq!(request["tool_choice"], json!("auto"));
    assert_eq!(request["max_completion_tokens"], 4096);
    assert_eq!(request["store"], false);
    assert_eq!(request["parallel_tool_calls"], true);
    for field in ["instructions", "input", "include", "max_output_tokens"] {
        assert!(request.get(field).is_none());
    }
    let messages = requests[1]["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 7);
    assert_eq!(messages[3], calls);
    for (index, id) in [(4, "a"), (5, "b")] {
        assert_eq!(messages[index]["role"], "tool");
        assert_eq!(messages[index]["tool_call_id"], id);
        let output: Value =
            serde_json::from_str(messages[index]["content"].as_str().unwrap()).unwrap();
        assert_eq!(
            client::expand_test(output)["review"],
            review_evidence(&review())
        );
    }
    assert_eq!(requests[1]["tool_choice"], "auto");
    assert!(
        requests[3]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["role"] == "assistant"
                && item["content"]
                    .as_str()
                    .is_some_and(|text| text.contains(&first)))
    );
    assert!(!requests[3].to_string().contains("test-reasoning"));
    assert!(!requests[3].to_string().contains("失败问题"));
    assert_eq!(requests.len(), 5);
    assert!(requests[4].to_string().contains("普通 Markdown 回答"));
    assert_eq!(requests[4]["tool_choice"], "auto");
    assert_eq!(requests[4]["parallel_tool_calls"], true);
    assert!(!requests[4].to_string().contains("回答校验失败"));
    assert!(!requests[4].to_string().contains("_chat_message"));
}

#[test]
fn chat_endpoint_variants_and_connection_probe_use_chat_protocol() {
    for path in [
        "/v1/chat/completions",
        "/proxy/chat/completions/",
        "/chat/completion",
    ] {
        let (endpoint, handle) = server_at(
            path,
            vec![
                (200, completion(json!({"role":"assistant","content":null,"reasoning_content":"probe-reasoning","tool_calls":[tool("probe", "{}")]}), "tool_calls").to_string()),
                (200, completion(text_message("连接成功"), "stop").to_string()),
            ],
        );
        AgentConfig {
            endpoint: &endpoint,
            model: "test",
            api_key: None,
            options: Default::default(),
        }
        .test_connection()
        .unwrap();
        let requests = handle.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0]["messages"].as_array().unwrap().len(), 2);
        assert_eq!(
            requests[1]["messages"][2]["reasoning_content"],
            "probe-reasoning"
        );
        assert_eq!(requests[1]["messages"][3]["tool_call_id"], "probe");
        assert!(
            requests[1]["messages"][3]["content"]
                .as_str()
                .unwrap()
                .contains("connection_test")
        );
        assert_eq!(requests[0]["tool_choice"], "auto");
        assert!(!requests[0]["messages"][1].to_string().contains("concealed"));
    }
}

#[test]
fn chat_includes_evidence_and_can_correct_invalid_tool_arguments() {
    let (endpoint, handle) = server_at(
        "/v1/chat/completions",
        vec![
            (200, completion(text_message("截断"), "length").to_string()),
            (
                200,
                completion(
                    tool_message(vec![tool("a", "{\"event_index\":999}")]),
                    "tool_calls",
                )
                .to_string(),
            ),
            (
                200,
                completion(tool_message(vec![tool("b", "{}")]), "tool_calls").to_string(),
            ),
            (
                200,
                completion(text_message(&valid_answer("未运行切牌分析。")), "stop").to_string(),
            ),
        ],
    );
    let log =
        convlog::tenhou::Log::from_json_str(include_str!("../../fixtures/tenhou/ranked_game.json"))
            .unwrap();
    let events = convlog::tenhou_to_mjai(&log).unwrap();
    let context = AgentContext::from_events(&events, PlayerIndex::new(0).unwrap(), 2).unwrap();
    let mut session = AgentSession::with_context(
        &context,
        &AgentConfig {
            endpoint: &endpoint,
            model: "test",
            api_key: None,
            options: Default::default(),
        },
    )
    .unwrap();
    assert!(matches!(
        session.ask_draft("失败取证"),
        Err(AgentError::OutputLimit)
    ));
    assert!(session.history.is_empty());
    assert_eq!(session.ask_draft("查看局面").unwrap(), "未运行切牌分析。");
    let requests = handle.join().unwrap();
    assert_eq!(requests[2]["tool_choice"], "auto");
    assert_eq!(
        requests[2]["tools"].as_array().unwrap().len(),
        tool_definitions().len()
    );
    assert!(requests[2].to_string().contains("invalid_arguments"));
    assert!(!requests[2].to_string().contains("失败取证"));
}

#[test]
fn chat_rejects_incomplete_refused_and_malformed_messages() {
    let good = text_message(&valid_answer("示例。"));
    let invalid_messages = [
        json!({}),
        json!({"choices":[]}),
        json!({"choices":[{"message":good,"finish_reason":"stop"},{"message":good,"finish_reason":"stop"}]}),
        completion(json!({"role":"user","content":"wrong role"}), "stop"),
        completion(json!({"role":"assistant","content":[]}), "stop"),
        completion(tool_message(vec![tool("a", "{}")]), "stop"),
        completion(tool_message(vec![]), "tool_calls"),
        completion(
            json!({"role":"assistant","tool_calls":"invalid"}),
            "tool_calls",
        ),
        completion(
            tool_message(vec![
                json!({"id":"a","type":"function","function":{"name":"get_review","arguments":{}}}),
            ]),
            "tool_calls",
        ),
        completion(
            json!({"role":"assistant","content":"x","function_call":{"name":"get_review"}}),
            "stop",
        ),
        completion(good.clone(), "function_call"),
    ];
    for body in invalid_messages {
        let (endpoint, handle) = server_at("/v1/chat/completions", vec![(200, body.to_string())]);
        let error = client::Client::new(&AgentConfig {
            endpoint: &endpoint,
            model: "test",
            api_key: None,
            options: Default::default(),
        })
        .unwrap()
        .respond(&[], RequestMode::ReviewProbe)
        .unwrap_err();
        assert!(
            matches!(error, AgentError::InvalidResponse { .. }),
            "{error}"
        );
        handle.join().unwrap();
    }
    for (body, refused) in [
        (completion(good, "length"), false),
        (completion(text_message("filtered"), "content_filter"), true),
        (
            completion(
                json!({"role":"assistant","refusal":"secret refusal","content":null}),
                "stop",
            ),
            true,
        ),
    ] {
        let (endpoint, handle) = server_at("/v1/chat/completions", vec![(200, body.to_string())]);
        let error = client::Client::new(&AgentConfig {
            endpoint: &endpoint,
            model: "test",
            api_key: None,
            options: Default::default(),
        })
        .unwrap()
        .respond(&[], RequestMode::ReviewProbe)
        .unwrap_err();
        assert!(if refused {
            matches!(error, AgentError::Refused)
        } else {
            matches!(error, AgentError::OutputLimit)
        });
        assert!(!error.to_string().contains("secret"));
        handle.join().unwrap();
    }
    let (endpoint, handle) = server_at(
        "/v1/chat/completions",
        vec![(
            200,
            completion(
                tool_message(vec![tool("a", "{}"), tool("a", "{}")]),
                "tool_calls",
            )
            .to_string(),
        )],
    );
    let mut session = AgentSession::new(
        &review(),
        &AgentConfig {
            endpoint: &endpoint,
            model: "test",
            api_key: None,
            options: Default::default(),
        },
    )
    .unwrap();
    assert!(matches!(
        session.ask_draft("分析"),
        Err(AgentError::InvalidResponse { .. })
    ));
    handle.join().unwrap();
}

#[test]
fn chat_archive_restores_tool_messages_and_provider_reasoning() {
    let tool = json!({"role":"assistant", "content":null, "reasoning_content":"provider-state", "tool_calls":[{"id":"saved-call","type":"function","function":{"name":"get_review","arguments":"{}"}}]});
    let reply = |text: &str| {
        json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":text}}]})
            .to_string()
    };
    let (endpoint, server) = server_at(
        "/v1/chat/completions",
        vec![
            (
                200,
                json!({"choices":[{"finish_reason":"tool_calls","message":tool}]}).to_string(),
            ),
            (200, reply("第一轮")),
            (200, reply("第二轮")),
        ],
    );
    let config = AgentConfig {
        endpoint: &endpoint,
        model: "test",
        api_key: None,
        options: Default::default(),
    };
    let mut session = AgentSession::new(&review(), &config).unwrap();
    session.ask_draft("先解释").unwrap();
    let json = serde_json::to_string(session.archive()).unwrap();
    let archive = SessionArchive::from_json(&json).unwrap();
    let mut resumed = AgentSession::from_archive(&archive, &config).unwrap();
    resumed.ask_draft("再解释").unwrap();
    let requests = server.join().unwrap();
    assert_eq!(requests[1]["messages"][3], tool);
    assert_eq!(requests[2]["messages"][3], text_message("第一轮"));
    assert!(!requests[2].to_string().contains("provider-state"));
    assert!(
        serde_json::to_string(resumed.archive())
            .unwrap()
            .contains("provider-state")
    );
    assert_eq!(requests[2]["messages"].as_array().unwrap().len(), 6);
    let mut tampered: Value = serde_json::from_str(&json).unwrap();
    tampered["history"][2]["_chat_message"]["role"] = json!("system");
    assert!(SessionArchive::from_json(&tampered.to_string()).is_err());
}
