use super::*;
use std::num::NonZeroU64;
use std::sync::{Arc, Mutex};

fn configured(endpoint: &str, budget: u64) -> AgentConfig<'_> {
    AgentConfig {
        endpoint,
        model: "arbitrary-model",
        api_key: None,
        options: ModelOptions {
            token_budget: NonZeroU64::new(budget),
            prices: Some(TokenPrices {
                currency: "USD".into(),
                input: 2.0,
                output: 8.0,
                cached_input: None,
            }),
            ..Default::default()
        },
    }
}

#[test]
fn budget_and_context_preflight_do_not_send_any_request() {
    let mut config = configured("http://127.0.0.1:1/responses", 1);
    let mut session = AgentSession::new(&review(), &config).unwrap();
    assert!(matches!(
        session.ask_draft("提问"),
        Err(AgentError::TokenBudget)
    ));
    let saved = serde_json::to_value(session.archive()).unwrap();
    assert!(saved["turns"][0]["usage"].is_null());
    config.options.token_budget = None;
    config.options.context_tokens = NonZeroU64::new(config.options.max_output_tokens.get() + 1);
    let mut session = AgentSession::new(&review(), &config).unwrap();
    assert!(matches!(
        session.ask_draft("提问"),
        Err(AgentError::ContextLimit)
    ));
}

#[test]
fn budget_stops_tool_execution_and_preserves_the_billed_call() {
    let mut body = response(vec![call("review", "get_review", "{}")]);
    body["usage"] = json!({"input_tokens":999900,"output_tokens":100});
    let (endpoint, handle) = server(vec![(200, body.to_string())]);
    let config = configured(&endpoint, 1_000_000);
    let mut session = AgentSession::new(&review(), &config).unwrap();
    assert!(matches!(
        session.ask_draft("提问"),
        Err(AgentError::TokenBudget)
    ));
    assert_eq!(handle.join().unwrap().len(), 1);
    let saved = serde_json::to_value(session.archive()).unwrap();
    let turn = &saved["turns"][0];
    assert_eq!(turn["usage"][0]["input_tokens"], 999900);
    assert_eq!(turn["options"]["token_budget"], 1_000_000);
    assert!(
        !turn["trace"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["kind"] == "tool")
    );
    assert!(saved["history"].as_array().unwrap().is_empty());
}

#[test]
fn cumulative_budget_blocks_followup_calls_and_resets_on_explicit_retry() {
    let mut tool = response(vec![call("review", "get_review", "{}")]);
    tool["usage"] = json!({"input_tokens":999890,"output_tokens":100});
    let mut answer = response(vec![raw_message("完成")]);
    answer["usage"] = json!({"input_tokens":500,"output_tokens":10});
    let (endpoint, handle) = server(vec![(200, tool.to_string()), (200, answer.to_string())]);
    let config = configured(&endpoint, 1_000_000);
    let mut session = AgentSession::new(&review(), &config).unwrap();
    assert!(matches!(
        session.ask_draft("提问"),
        Err(AgentError::TokenBudget)
    ));
    assert_eq!(session.ask_draft("重试").unwrap(), "完成");
    assert_eq!(handle.join().unwrap().len(), 2);
    let saved = serde_json::to_value(session.archive()).unwrap();
    assert_eq!(saved["turns"][0]["usage"].as_array().unwrap().len(), 1);
    assert_eq!(saved["turns"][1]["usage"][0]["input_tokens"], 500);
}

#[test]
fn missing_usage_stops_a_budgeted_task_without_inventing_zero_usage() {
    let (endpoint, handle) = server(vec![(
        200,
        response(vec![call("r", "get_review", "{}")]).to_string(),
    )]);
    let config = configured(&endpoint, 1_000_000);
    let mut session = AgentSession::new(&review(), &config).unwrap();
    assert!(matches!(
        session.ask_draft("提问"),
        Err(AgentError::UnknownUsage)
    ));
    assert_eq!(handle.join().unwrap().len(), 1);
    let saved = serde_json::to_value(session.archive()).unwrap();
    assert!(saved["turns"][0]["usage"][0]["input_tokens"].is_null());
    assert!(saved["turns"][0]["usage"][0]["cost"].is_null());
}

#[test]
fn truncated_and_refused_chat_responses_still_record_exact_usage() {
    for reason in ["length", "content_filter"] {
        let body = json!({"choices":[{"finish_reason":reason,"message":{"role":"assistant","content":"未完成"}}],
            "usage":{"prompt_tokens":1000,"completion_tokens":200}});
        let (endpoint, handle) = server_at("/chat/completions", vec![(200, body.to_string())]);
        let config = configured(&endpoint, 1_000_000);
        let mut session = AgentSession::new(&review(), &config).unwrap();
        assert!(session.ask_draft("提问").is_err());
        handle.join().unwrap();
        let saved = serde_json::to_value(session.archive()).unwrap();
        assert_eq!(saved["turns"][0]["usage"][0]["output_tokens"], 200);
        assert_eq!(saved["turns"][0]["usage"][0]["cost"], 0.0036);
        SessionArchive::from_json(&saved.to_string()).unwrap();
    }
}

#[test]
fn usage_is_reported_before_completion_and_historical_prices_survive_restore() {
    let mut body = response(vec![raw_message("完成")]);
    body["usage"] = json!({"input_tokens":1000,"output_tokens":200});
    let (endpoint, handle) = server(vec![(200, body.to_string()); 2]);
    let mut config = configured(&endpoint, 1_000_000);
    let mut session = AgentSession::new(&review(), &config).unwrap();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let events = observed.clone();
    let control = QuestionControl::new(move |progress| events.lock().unwrap().push(progress));
    assert_eq!(
        session.ask_draft_with_control("提问", &control).unwrap(),
        "完成"
    );
    assert!(observed.lock().unwrap().iter().any(|p| matches!(p, QuestionProgress::Usage { requests, .. } if requests.len() == 1 && requests[0].input_tokens == Some(1000))));
    let archive =
        SessionArchive::from_json(&serde_json::to_string(session.archive()).unwrap()).unwrap();
    config.options.prices.as_mut().unwrap().input = 20.0;
    let mut restored = AgentSession::from_archive(&archive, &config).unwrap();
    restored.ask_draft("追问").unwrap();
    handle.join().unwrap();
    let saved = serde_json::to_value(restored.archive()).unwrap();
    assert_eq!(saved["turns"][0]["usage"][0]["cost"], 0.0036);
    assert_eq!(saved["turns"][1]["usage"][0]["cost"], 0.0216);
}
