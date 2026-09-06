use super::*;
use crate::{
    analysis::{DiscardEfficiency, DrawCandidates, TileAvailability},
    mahjong::{
        meld::Meld,
        player::{Discard, RiichiState},
        player_index::PlayerIndex,
        round::{DrawSource, RoundId, RoundPhase, Wind},
        tile::{Tile, TileKind},
    },
    mortal::{Action, Candidate, Decision, KanCandidate, ModelInfo},
    review::{PublicPlayer, VisiblePosition},
};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};

fn review() -> Review {
    let tile = Tile::new(34).unwrap();
    Review {
        event_index: 12,
        player: PlayerIndex::new(0).unwrap(),
        position: VisiblePosition {
            round: RoundId::new(Wind::South, 2).unwrap(),
            honba: 1,
            riichi_sticks: 2,
            remaining_draws: 40,
            phase: RoundPhase::AfterDraw {
                player: PlayerIndex::new(0).unwrap(),
                source: DrawSource::Rinshan,
            },
            dora_indicators: vec![Tile::new(31).unwrap()],
            concealed: vec![tile, Tile::new(4).unwrap()],
            players: std::array::from_fn(|_| PublicPlayer {
                score: 25_000,
                riichi: RiichiState::Accepted,
                discards: vec![Discard::new(tile, true, true, true)],
                melds: vec![Meld::Pon {
                    tiles: [tile; 3],
                    called: tile,
                    from: PlayerIndex::new(3).unwrap(),
                }],
            }),
        },
        model: ModelInfo {
            version: 4,
            tag: "test-only".into(),
            sha256: "0".repeat(64),
        },
        decision: Some(Decision {
            recommended: convlog::Event::None,
            candidates: vec![
                Candidate {
                    action: Action::Win,
                    q_value: 0.9,
                },
                Candidate {
                    action: Action::Pass,
                    q_value: 0.1,
                },
                Candidate {
                    action: Action::Kan,
                    q_value: 0.2,
                },
            ],
            kan_candidates: vec![KanCandidate {
                tile: TileKind::new(4).unwrap(),
                q_value: -0.5,
            }],
            shanten: Some(0),
            at_furiten: Some(true),
        }),
        discards: vec![DiscardEfficiency {
            discard: tile,
            shanten: 0,
            candidates: DrawCandidates::Winning(vec![TileAvailability {
                kind: TileKind::new(27).unwrap(),
                unseen: 3,
            }]),
            total_unseen: 3,
        }],
    }
}

fn response(output: Vec<Value>) -> Value {
    json!({"status": "completed", "output": output})
}

fn call(id: &str, name: &str, arguments: &str) -> Value {
    json!({"type": "function_call", "id": format!("fc_{id}"), "status": "completed", "call_id": id, "name": name, "arguments": arguments})
}

fn message(text: &str) -> Value {
    json!({"type": "message", "id": "msg_test", "role": "assistant", "status": "completed", "content": [{"type": "output_text", "text": text, "annotations": []}]})
}

#[test]
fn evidence_preserves_sources_red_tiles_public_details_and_final_recommendation() {
    let value = review_evidence(&review());
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["position"]["concealed"], json!(["5mr", "5m"]));
    assert_eq!(value["position"]["dealer"], 1);
    assert_eq!(value["position"]["phase"]["source"], "rinshan");
    let player = &value["position"]["players"][3];
    assert!(player.get("concealed").is_none());
    assert_eq!(
        player["discards"][0],
        json!({"tile": "5mr", "tsumogiri": true, "riichi": true, "called": true})
    );
    assert_eq!(player["melds"][0]["from"], 3);
    assert_eq!(value["discards"][0]["draw_kind"], "winning_shape");
    assert_eq!(
        value["discards"][0]["draws"],
        json!([{"tile": "E", "unseen": 3}])
    );
    let decision = &value["mortal"]["decision"];
    assert_eq!(decision["recommended"], json!({"type": "none"}));
    assert_eq!(decision["candidates"][0]["action"]["kind"], "win");
    assert_ne!(
        decision["candidates"][2]["q_value"],
        decision["kan_candidates"][0]["q_value"]
    );
    assert_eq!(value["mortal"]["model"]["sha256"], "0".repeat(64));
    let mut none = review();
    none.decision = None;
    none.discards.clear();
    assert!(review_evidence(&none)["mortal"]["decision"].is_null());
}

#[test]
fn tool_rejects_unknown_names_and_nonempty_or_malformed_arguments() {
    let evidence = json!({"private_test_marker": true});
    for args in [
        "null",
        "[]",
        "",
        "{",
        "{\"event_index\":13}",
        "{\"path\":\"secret\"}",
    ] {
        let (result, success) = execute_tool(&evidence, "get_review", args);
        assert!(!success);
        assert_eq!(result["error"]["code"], "invalid_arguments");
        assert!(result.get("review").is_none());
    }
    assert_eq!(
        execute_tool(&evidence, "read_file", "{}").0["error"]["code"],
        "unknown_tool"
    );
    assert_eq!(
        execute_tool(&evidence, "get_review", " {} "),
        (json!({"ok": true, "review": evidence}), true)
    );
}

#[test]
fn first_answer_requires_evidence_and_followup_preserves_reasoning_and_history() {
    let evidence = review_evidence(&review());
    let reasoning =
        json!({"type": "reasoning", "id": "rs_test", "summary": [], "encrypted_content": "opaque"});
    let mut requests = 0;
    let (text, history) = answer(
        &evidence,
        &[],
        false,
        "切牌怎么比较？",
        |input, forced| {
            requests += 1;
            if requests == 1 {
                assert!(forced);
                assert_eq!(input.len(), 1);
                Ok(response(vec![
                    reasoning.clone(),
                    call("a", "get_review", "{}"),
                ]))
            } else {
                assert!(!forced);
                assert_eq!(input[1], reasoning);
                let tool: Value =
                    serde_json::from_str(input.last().unwrap()["output"].as_str().unwrap())
                        .unwrap();
                assert_eq!(tool["review"], evidence);
                Ok(response(vec![message(
                    "【计算】听牌，完成牌形的东风有 3 枚不可见。",
                )]))
            }
        },
    )
    .unwrap();
    assert_eq!(requests, 2);
    assert!(text.contains("3 枚"));
    let (_, next) = answer(
        &evidence,
        &history,
        true,
        "这就是剩余牌山吗？",
        |input, forced| {
            assert!(!forced);
            assert_eq!(&input[..history.len()], &history);
            Ok(response(vec![message("不是，不可见牌也可能在对手手中。")]))
        },
    )
    .unwrap();
    assert_eq!(next.len(), history.len() + 2);
}

#[test]
fn invalid_tool_arguments_can_be_corrected_without_gaining_evidence() {
    let mut request = 0;
    answer(&json!({}), &[], false, "分析", |input, forced| {
        request += 1;
        match request {
            1 => Ok(response(vec![call(
                "a",
                "get_review",
                "{\"event_index\":999}",
            )])),
            2 => {
                assert!(forced);
                assert!(
                    input.last().unwrap()["output"]
                        .as_str()
                        .unwrap()
                        .contains("invalid_arguments")
                );
                Ok(response(vec![call("b", "get_review", "{}")]))
            }
            _ => Ok(response(vec![message("完成")])),
        }
    })
    .unwrap();
    assert_eq!(request, 3);
}

#[test]
fn missing_evidence_incomplete_refusal_and_malformed_output_never_become_answers() {
    assert!(matches!(
        answer(&json!({}), &[], false, "分析", |_, _| Ok(response(vec![
            message("编造")
        ]))),
        Err(AgentError::MissingEvidence)
    ));
    for bad in [
        json!({"status": "completed"}),
        response(vec![]),
        response(vec![json!({"type": "web_search_call"})]),
        response(vec![
            call("a", "get_review", "{}"),
            call("a", "get_review", "{}"),
        ]),
        response(vec![
            json!({"type": "message", "role": "user", "content": []}),
        ]),
    ] {
        assert!(matches!(
            answer(&json!({}), &[], true, "分析", |_, _| Ok(bad.clone())),
            Err(AgentError::InvalidResponse { .. })
        ));
    }
    for status in ["failed", "incomplete", "in_progress", "cancelled"] {
        assert!(matches!(
            answer(&json!({}), &[], true, "分析", |_, _| Ok(
                json!({"status": status, "output": [message("截断")]})
            )),
            Err(AgentError::IncompleteResponse)
        ));
    }
    let refusal = json!({"type": "message", "role": "assistant", "status": "completed", "content": [{"type": "refusal", "refusal": "no"}]});
    assert!(matches!(
        answer(&json!({}), &[], true, "分析", |_, _| Ok(response(vec![
            refusal.clone()
        ]))),
        Err(AgentError::Refused)
    ));
}

#[test]
fn requests_questions_and_history_are_bounded() {
    let mut requests = 0;
    let result = answer(&json!({}), &[], false, "分析", |_, _| {
        requests += 1;
        Ok(response(vec![call(
            &requests.to_string(),
            "get_review",
            "{}",
        )]))
    });
    assert!(matches!(result, Err(AgentError::RequestLimit)));
    assert_eq!(requests, MAX_REQUESTS);
    for question in [" ".into(), "x".repeat(MAX_QUESTION_BYTES + 1)] {
        assert!(matches!(
            answer(&json!({}), &[], false, &question, |_, _| panic!(
                "不应请求网络"
            )),
            Err(AgentError::InvalidQuestion)
        ));
    }
    let history = vec![json!({"role": "user", "content": "x".repeat(MAX_HISTORY_BYTES)})];
    assert!(matches!(
        answer(&json!({}), &history, false, "分析", |_, _| panic!(
            "不应请求网络"
        )),
        Err(AgentError::HistoryLimit)
    ));
}

// 使用真实本地 HTTP 验证协议；不使用外部账号，不依赖模型生成固定句子。
fn server(responses: Vec<(u16, String)>) -> (String, thread::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        for (status, body) in responses {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "等待测试请求超时");
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(&mut stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(line.starts_with("POST /v1/responses HTTP/1.1"));
            let mut length = None;
            loop {
                line.clear();
                assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                if line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    length = Some(value.trim().parse::<usize>().unwrap());
                }
            }
            let mut data = vec![0; length.unwrap()];
            reader.read_exact(&mut data).unwrap();
            requests.push(serde_json::from_slice(&data).unwrap());
            // 客户端可以因响应过大而提前关闭连接。
            let _ = write!(
                stream,
                "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
        requests
    });
    (endpoint, handle)
}

#[test]
fn connection_test_requires_a_valid_tool_call_without_sending_review_data() {
    let (endpoint, handle) = server(vec![(
        200,
        response(vec![call("probe", "get_review", "{}")]).to_string(),
    )]);
    AgentConfig {
        endpoint: &endpoint,
        model: "test-model",
        api_key: None,
    }
    .test_connection()
    .unwrap();
    let requests = handle.join().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["store"], false);
    assert_eq!(requests[0]["tool_choice"]["name"], "get_review");
    assert_eq!(requests[0]["input"].as_array().unwrap().len(), 1);
    assert!(!requests[0].to_string().contains("concealed"));
}

#[test]
fn connection_test_rejects_successful_http_with_incompatible_output() {
    for body in [
        response(vec![message("OK")]),
        response(vec![call("probe", "get_review", "{\"wrong\":true}")]),
    ] {
        let (endpoint, handle) = server(vec![(200, body.to_string())]);
        assert!(matches!(
            AgentConfig {
                endpoint: &endpoint,
                model: "test",
                api_key: None
            }
            .test_connection(),
            Err(AgentError::InvalidResponse { .. })
        ));
        handle.join().unwrap();
    }
}

#[test]
fn http_session_handles_tool_roundtrip_followup_and_rolls_back_failed_turn() {
    let (endpoint, handle) = server(vec![
        (
            200,
            response(vec![call("a", "get_review", "{}")]).to_string(),
        ),
        (200, response(vec![message("首次解释")]).to_string()),
        (429, "do-not-print-remote-body".into()),
        (200, response(vec![message("追问解释")]).to_string()),
    ]);
    let mut session = AgentSession::new(
        &review(),
        &AgentConfig {
            endpoint: &endpoint,
            model: "test-model",
            api_key: None,
        },
    )
    .unwrap();
    assert_eq!(session.ask("第一问").unwrap(), "首次解释");
    let history = session.history.clone();
    let error = session.ask("失败的问题").unwrap_err();
    assert!(matches!(error, AgentError::Http { status: 429 }));
    assert!(!error.to_string().contains("do-not-print"));
    assert_eq!(session.history, history);
    assert_eq!(session.ask("第二问").unwrap(), "追问解释");
    let requests = handle.join().unwrap();
    assert_eq!(requests[0]["model"], "test-model");
    assert_eq!(requests[0]["store"], false);
    assert_eq!(
        requests[0]["tool_choice"],
        json!({"type": "function", "name": "get_review"})
    );
    assert_eq!(
        requests[0]["include"],
        json!(["reasoning.encrypted_content"])
    );
    assert_eq!(requests[1]["tool_choice"], "auto");
    assert_eq!(
        requests[3]["input"].as_array().unwrap().len(),
        history.len() + 1
    );
    assert!(!requests[3].to_string().contains("失败的问题"));
    assert!(!requests[0].to_string().contains("5mr"));
    assert!(requests[1].to_string().contains("5mr"));
}

#[test]
fn http_status_invalid_json_and_response_size_are_checked() {
    for (status, body, expected) in [
        (401, "secret".into(), "HTTP 401"),
        (302, "".into(), "HTTP 302"),
        (200, "not-json".into(), "not valid JSON"),
        (200, "x".repeat(2 * 1024 * 1024 + 1), "response_too_large"),
    ] {
        let (endpoint, handle) = server(vec![(status, body)]);
        let client = client::Client::new(&AgentConfig {
            endpoint: &endpoint,
            model: "test",
            api_key: None,
        })
        .unwrap();
        let error = client.respond(&[], true).unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
        assert!(!error.to_string().contains("secret"));
        handle.join().unwrap();
    }
}

#[test]
fn config_rejects_invalid_endpoints_and_credentials_without_echoing_them() {
    for endpoint in [
        "relative",
        "http://example.com/v1/responses",
        "https://user:secret@example.com/v1/responses",
        "https://example.com/v1/responses?key=secret",
        "https://example.com/#secret",
    ] {
        let result = client::Client::new(&AgentConfig {
            endpoint,
            model: "test",
            api_key: Some("secret"),
        });
        let error = result.err().unwrap();
        assert!(matches!(
            error,
            AgentError::InvalidConfig {
                field: "endpoint",
                ..
            }
        ));
        assert!(!format!("{error:?} {error}").contains("secret"));
    }
    for key in [None, Some(""), Some("\r\nsecret")] {
        assert!(matches!(
            client::Client::new(&AgentConfig {
                endpoint: "https://api.openai.com/v1/responses",
                model: "test",
                api_key: key
            }),
            Err(AgentError::InvalidConfig {
                field: "api_key",
                ..
            })
        ));
    }
}
