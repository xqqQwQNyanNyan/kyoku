use super::*;

fn contexts() -> (AgentContext, AgentContext) {
    let log =
        convlog::tenhou::Log::from_json_str(include_str!("../../fixtures/tenhou/ranked_game.json"))
            .unwrap();
    let events = convlog::tenhou_to_mjai(&log).unwrap();
    (
        AgentContext::from_events(&events, PlayerIndex::new(0).unwrap(), 2).unwrap(),
        AgentContext::from_events(&events, PlayerIndex::new(1).unwrap(), 4).unwrap(),
    )
}

fn chat_tool(id: &str, name: &str, arguments: &str) -> String {
    json!({"choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","content":null,"tool_calls":[
        {"id":id,"type":"function","function":{"name":name,"arguments":arguments}}
    ]}}]}).to_string()
}

fn chat_text(answer: &str) -> String {
    json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":answer}}]})
        .to_string()
}

fn chat_answer(text: &str) -> String {
    chat_text(&json!({"sections":[{"source":"limitation","text":text,"facts":[]}]}).to_string())
}

#[test]
fn switching_context_preserves_history_and_scopes_tool_results_in_both_protocols() {
    let (first, next) = contexts();
    let first_hand = first.evidence()["position"]["concealed"].clone();
    let next_hand = next.evidence()["position"]["concealed"].clone();
    let comparison_answer = json!({"sections":[
        {"source":"position","text":"之前和当前观察玩家的手牌分别来自各自的快照。","facts":[
            {"path":"/past/0/position/concealed","value":first_hand},
            {"path":"/position/concealed","value":next_hand}
        ]}
    ]});
    for chat in [false, true] {
        let responses = if chat {
            vec![
                chat_tool("old-score", "analyze_score_targets", r#"{"target":1}"#),
                chat_answer("第一处的回答"),
                chat_tool("new-review", "get_review", "{}"),
                chat_text(&comparison_answer.to_string()),
                chat_answer("恢复后的追问"),
            ]
        } else {
            vec![
                response(vec![call(
                    "old-score",
                    "analyze_score_targets",
                    r#"{"target":1}"#,
                )])
                .to_string(),
                response(vec![message("第一处的回答")]).to_string(),
                response(vec![call("new-review", "get_review", "{}")]).to_string(),
                response(vec![raw_message(&comparison_answer.to_string())]).to_string(),
                response(vec![message("恢复后的追问")]).to_string(),
            ]
        };
        let (endpoint, handle) = server_at(
            if chat {
                "/v1/chat/completions"
            } else {
                "/v1/responses"
            },
            responses.into_iter().map(|body| (200, body)).collect(),
        );
        let config = AgentConfig {
            endpoint: &endpoint,
            model: "test",
            api_key: None,
        };
        let mut session = AgentSession::with_context(&first, &config).unwrap();
        session.ask("看看这里的点差").unwrap();
        let original_history = session.history.clone();
        session.set_context(&next);
        let answer = session.ask("换到这里，和刚才的手牌相比呢？").unwrap();
        assert!(answer.contains("之前和当前"));
        assert_eq!(
            &session.history[..original_history.len()],
            original_history.as_slice()
        );
        let verified = evidence_with_history(session.evidence(), &session.history);
        assert!(verified["analyses"].as_object().unwrap().is_empty());
        assert!(
            verified["past"][0]["analyses"]
                .get("score_target_1")
                .is_some()
        );
        assert_eq!(verified["player"], 1);
        assert_eq!(verified["past"][0]["player"], 0);
        let archive_json = serde_json::to_string(session.archive()).unwrap();
        let archive = SessionArchive::from_json(&archive_json).unwrap();
        let saved: Value = serde_json::from_str(&archive_json).unwrap();
        assert_eq!(saved["turns"][0]["evidence"]["event_index"], 2);
        assert_eq!(saved["turns"][1]["evidence"]["event_index"], 4);
        assert_eq!(saved["turns"][1]["evidence"]["player"], 1);
        let mut restored = AgentSession::from_archive(&archive, &config).unwrap();
        restored.ask("继续").unwrap();
        assert_eq!(
            restored.history.iter().filter_map(context_evidence).count(),
            2
        );
        let requests = handle.join().unwrap();
        assert_eq!(requests.len(), 5);
        let final_input = if chat {
            &requests[4]["messages"]
        } else {
            &requests[4]["input"]
        };
        assert!(final_input.to_string().contains("看看这里的点差"));
        assert!(final_input.to_string().contains("换到这里"));
        // 重算校验必须在各自快照中执行，不能拿当前玩家校验之前的点差工具。
        let mut tampered = saved;
        let history = tampered["history"].as_array_mut().unwrap();
        let output = history
            .iter_mut()
            .find(|item| item["type"] == "function_call_output" && item["call_id"] == "new-review")
            .unwrap();
        output["output"] = json!(json!({"ok":true,"review":first.evidence()}).to_string());
        assert!(SessionArchive::from_json(&tampered.to_string()).is_err());
    }
}

#[test]
fn failed_context_change_survives_restart_without_polluting_accepted_history() {
    let (first, next) = contexts();
    let (endpoint, handle) = server(vec![
        (200, response(vec![message("第一问")]).to_string()),
        (503, "unavailable".into()),
        (200, response(vec![message("重试成功")]).to_string()),
    ]);
    let config = AgentConfig {
        endpoint: &endpoint,
        model: "test",
        api_key: None,
    };
    let mut session = AgentSession::with_context(&first, &config).unwrap();
    session.ask("第一处").unwrap();
    let accepted = session.history.clone();
    session.set_context(&next);
    assert!(session.ask("第二处失败").is_err());
    assert_eq!(session.history, accepted);
    let archive =
        SessionArchive::from_json(&serde_json::to_string(session.archive()).unwrap()).unwrap();
    let (question, context) = archive.failed_turn_context(1).unwrap();
    assert_eq!(question, "第二处失败");
    assert_eq!(context.evidence(), next.evidence());
    assert!(archive.failed_turn_context(0).is_none());
    let mut restored = AgentSession::from_archive(&archive, &config).unwrap();
    restored.ask(question).unwrap();
    assert_eq!(
        restored.history.iter().filter_map(context_evidence).count(),
        2
    );
    let requests = handle.join().unwrap();
    assert_eq!(requests[2]["input"], requests[1]["input"]);
}

#[test]
fn analysis_arriving_at_the_same_event_creates_a_new_snapshot() {
    let (context, _) = contexts();
    let mut analysed = AgentContext {
        evidence: context.evidence().clone(),
    };
    analysed.evidence["analysis_status"] = json!("available");
    let config = AgentConfig {
        endpoint: "http://localhost/responses",
        model: "test",
        api_key: None,
    };
    let mut session = AgentSession::with_context(&context, &config).unwrap();
    let (_, history) = answer(context.evidence(), &[], false, "分析前", |_, _| {
        Ok(response(vec![message("尚未分析")]))
    })
    .unwrap();
    session.history = history;
    session.has_evidence = true;
    session.set_context(&analysed);
    assert!(!session.has_evidence);
    let (_, history) = answer(
        session.evidence(),
        &session.history,
        session.has_evidence,
        "分析后",
        |input, _| {
            assert_eq!(
                context_evidence(&input[input.len() - 2]).unwrap()["analysis_status"],
                "available"
            );
            Ok(response(vec![message("已更新快照")]))
        },
    )
    .unwrap();
    assert_eq!(history.iter().filter_map(context_evidence).count(), 2);
}

#[test]
fn historical_facts_must_match_their_own_snapshot_and_availability() {
    let (context, _) = contexts();
    let mut current = review_evidence(&review());
    current["past"] = json!([context.evidence()]);
    let fact = |source: &str, path: &str, value: Value| {
        json!({"sections":[{
            "source":source,"text":"核对这份证据。","facts":[{"path":path,"value":value}]
        }]})
        .to_string()
    };
    assert!(
        output::render(
            &fact(
                "position",
                "/past/0/position/concealed",
                context.evidence()["position"]["concealed"].clone()
            ),
            &current
        )
        .is_ok()
    );
    assert!(
        output::render(
            &fact(
                "position",
                "/past/0/position/concealed",
                json!(["made-up-tile"])
            ),
            &current
        )
        .is_err()
    );
    assert!(
        output::render(
            &fact("position", "/past/1/position/concealed", json!([])),
            &current
        )
        .is_err()
    );
    assert!(
        output::render(
            &fact("mortal", "/past/0/mortal/decision/recommended", json!({})),
            &current
        )
        .is_err()
    );
    assert!(
        output::render(
            &fact("calculation", "/past/0/discards/0/shanten", json!(1)),
            &current
        )
        .is_err()
    );
}
