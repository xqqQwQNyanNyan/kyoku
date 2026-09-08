use super::*;
use std::sync::{Arc, Mutex};

fn context() -> AgentContext {
    let log =
        convlog::tenhou::Log::from_json_str(include_str!("../../fixtures/tenhou/ranked_game.json"))
            .unwrap();
    let events = convlog::tenhou_to_mjai(&log).unwrap();
    AgentContext::from_events(&events, PlayerIndex::new(0).unwrap(), 2).unwrap()
}

fn text_response(chat: bool, text: &str) -> String {
    if chat {
        json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":text,"reasoning_content":"private-reasoning"}}],
            "usage":{"prompt_tokens":100,"completion_tokens":20}}).to_string()
    } else {
        let mut result = response(vec![
            json!({"type":"reasoning","encrypted_content":"private-reasoning"}),
            raw_message(text),
        ]);
        result["usage"] = json!({"input_tokens":100,"output_tokens":20});
        result.to_string()
    }
}

#[test]
fn verification_is_a_fresh_request_and_only_its_answer_enters_history_in_both_protocols() {
    for chat in [false, true] {
        let (endpoint, handle) = server_at(
            if chat {
                "/chat/completions"
            } else {
                "/responses"
            },
            vec![
                (200, text_response(chat, "错误草稿一")),
                (200, text_response(chat, "修订终稿一")),
                (200, text_response(chat, "本题草稿二")),
                (200, text_response(chat, "修订终稿二")),
            ],
        );
        let mut session = AgentSession::with_context(
            &context(),
            &AgentConfig {
                endpoint: &endpoint,
                model: "configured-terra",
                api_key: None,
                options: Default::default(),
            },
        )
        .unwrap();
        let stages = Arc::new(Mutex::new(Vec::new()));
        let collected = stages.clone();
        let control = QuestionControl::new(move |stage| {
            collected
                .lock()
                .unwrap()
                .push(serde_json::to_value(stage).unwrap())
        });
        assert_eq!(
            session.ask_with_control("第一问", &control).unwrap(),
            "修订终稿一"
        );
        assert!(
            stages
                .lock()
                .unwrap()
                .contains(&json!({"phase":"verifying","request":2}))
        );
        assert_eq!(session.ask("第二问").unwrap(), "修订终稿二");
        let requests = handle.join().unwrap();
        assert_eq!(requests.len(), 4);
        for index in [1, 3] {
            let request = &requests[index];
            assert_eq!(request["model"], "configured-terra");
            assert!(request.get("tools").is_none());
            assert!(request.get("tool_choice").is_none());
            let input = &request[if chat { "messages" } else { "input" }];
            assert!(!input.to_string().contains("private-reasoning"));
            assert!(!input.to_string().contains("修订终稿一"));
            assert!(input.to_string().contains("核查证据"));
            assert!(
                input
                    .to_string()
                    .contains(if index == 1 { "第一问" } else { "第二问" })
            );
            assert_eq!(
                if chat {
                    &request["messages"][0]["content"]
                } else {
                    &request["instructions"]
                },
                super::super::verification::INSTRUCTIONS
            );
        }
        assert!(!requests[3].to_string().contains("第一问"));
        let archive = serde_json::to_value(session.archive()).unwrap();
        assert!(!archive["history"].to_string().contains("错误草稿一"));
        assert!(!archive["history"].to_string().contains("本题草稿二"));
        assert!(archive["history"].to_string().contains("修订终稿二"));
        assert!(
            archive["turns"][0]["trace"]
                .to_string()
                .contains("错误草稿一")
        );
        assert_eq!(archive["turns"][0]["usage"].as_array().unwrap().len(), 2);
        assert_eq!(archive["turns"][0]["usage"][1]["input_tokens"], 100);
        let restored = SessionArchive::from_json(&archive.to_string()).unwrap();
        AgentSession::from_archive(
            &restored,
            &AgentConfig {
                endpoint: &endpoint,
                model: "configured-terra",
                api_key: None,
                options: Default::default(),
            },
        )
        .unwrap();
    }
}

#[test]
fn verifier_receives_current_tool_results_but_not_author_reasoning() {
    let (endpoint, handle) = server(vec![
        (
            200,
            response(vec![call("d", "analyze_defense", "{}")]).to_string(),
        ),
        (200, text_response(false, "草稿")),
        (200, text_response(false, "终稿")),
    ]);
    let mut session = AgentSession::with_context(
        &context(),
        &AgentConfig {
            endpoint: &endpoint,
            model: "test",
            api_key: None,
            options: Default::default(),
        },
    )
    .unwrap();
    assert_eq!(session.ask("安全牌有哪些？").unwrap(), "终稿");
    let requests = handle.join().unwrap();
    assert_eq!(requests.len(), 3);
    let source = requests[2]["input"][0]["content"]
        .as_str()
        .unwrap()
        .split_once('：')
        .unwrap()
        .1;
    let evidence: Value = serde_json::from_str(source).unwrap();
    assert_eq!(evidence["tools"].as_array().unwrap().len(), 1);
    assert_eq!(evidence["tools"][0]["name"], "analyze_defense");
    assert_eq!(evidence["tools"][0]["result"]["ok"], true);
    SessionArchive::from_json(&serde_json::to_string(session.archive()).unwrap()).unwrap();
}

#[test]
fn failed_incomplete_empty_or_tool_calling_verifier_never_publishes_draft_or_retries() {
    for (status, body) in [
        (429,"{}".into()),
        (200,json!({"status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},"output":[]}).to_string()),
        (200,response(vec![]).to_string()),
        (200,response(vec![call("bad","get_review","{}"),raw_message("未经核查")]).to_string()),
    ] {
        let (endpoint,handle) = server(vec![(200,text_response(false,"错误草稿")),(status,body)]);
        let mut session = AgentSession::with_context(&context(),&AgentConfig{endpoint:&endpoint,model:"test",api_key:None,options:Default::default()}).unwrap();
        let error = session.ask("问题").unwrap_err();
        assert!(matches!(error,AgentError::VerificationFailed{..}));
        assert!(error.to_string().contains("答案核查未完成"));
        assert_eq!(handle.join().unwrap().len(),2);
        let archive = serde_json::to_value(session.archive()).unwrap();
        assert_eq!(archive["history"],json!([]));
        assert_eq!(archive["turns"][0]["answer"],Value::Null);
        assert!(archive["turns"][0]["trace"].to_string().contains("错误草稿"));
        assert_eq!(archive["turns"][0]["usage"].as_array().unwrap().len(),2);
        SessionArchive::from_json(&archive.to_string()).unwrap();
    }
}

#[test]
fn verifier_budget_failure_keeps_usage_and_does_not_send_a_request() {
    let mut draft: Value =
        serde_json::from_str(&text_response(false, "不能直接交付的草稿")).unwrap();
    draft["usage"] = json!({"input_tokens":999900,"output_tokens":100});
    let (endpoint, handle) = server(vec![(200, draft.to_string())]);
    let config = AgentConfig {
        endpoint: &endpoint,
        model: "test",
        api_key: None,
        options: ModelOptions {
            token_budget: std::num::NonZeroU64::new(1_000_000),
            ..Default::default()
        },
    };
    let mut session = AgentSession::with_context(&context(), &config).unwrap();
    let error = session.ask("问题").unwrap_err();
    assert!(
        matches!(error,AgentError::VerificationFailed{source} if matches!(*source,AgentError::TokenBudget))
    );
    assert_eq!(handle.join().unwrap().len(), 1);
    let archive = serde_json::to_value(session.archive()).unwrap();
    assert_eq!(archive["turns"][0]["answer"], Value::Null);
    assert_eq!(archive["turns"][0]["usage"].as_array().unwrap().len(), 1);
}

#[test]
fn stopping_at_verification_does_not_publish_the_draft() {
    let (endpoint, handle) = server(vec![(200, text_response(false, "草稿"))]);
    let holder = Arc::new(Mutex::new(None::<QuestionControl>));
    let handle_control = holder.clone();
    let control = QuestionControl::new(move |stage| {
        if matches!(stage, QuestionProgress::Verifying { .. }) {
            handle_control.lock().unwrap().as_ref().unwrap().cancel();
        }
    });
    *holder.lock().unwrap() = Some(control.clone());
    let mut session = AgentSession::with_context(
        &context(),
        &AgentConfig {
            endpoint: &endpoint,
            model: "test",
            api_key: None,
            options: Default::default(),
        },
    )
    .unwrap();
    assert!(matches!(
        session.ask_with_control("问题", &control),
        Err(AgentError::Cancelled)
    ));
    assert_eq!(handle.join().unwrap().len(), 1);
    assert!(session.history.is_empty());
    *holder.lock().unwrap() = None;
}
