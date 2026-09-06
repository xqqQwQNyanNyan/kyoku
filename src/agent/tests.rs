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
    raw_message(
        &json!({"sections": [{"source": "limitation", "text": text, "facts": []}]}).to_string(),
    )
}

fn raw_message(text: &str) -> Value {
    json!({"type": "message", "id": "msg_test", "role": "assistant", "status": "completed", "content": [{"type": "output_text", "text": text, "annotations": []}]})
}

#[test]
fn evidence_preserves_sources_red_tiles_public_details_and_final_recommendation() {
    let value = review_evidence(&review());
    assert_eq!(value["schema_version"], 2);
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
                    "听牌，完成牌形的东风有 3 枚不可见。",
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
    server_at("/v1/responses", responses)
}

fn server_at(
    path: &str,
    responses: Vec<(u16, String)>,
) -> (String, thread::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}{path}", listener.local_addr().unwrap());
    let path = path.to_owned();
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
            assert!(line.starts_with(&format!("POST {path} HTTP/1.1")));
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

#[path = "chat_tests.rs"]
mod chat;

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
    assert_eq!(session.ask("第一问").unwrap(), "【说明】首次解释");
    let history = session.history.clone();
    let error = session.ask("失败的问题").unwrap_err();
    assert!(matches!(error, AgentError::Http { status: 429 }));
    assert!(!error.to_string().contains("do-not-print"));
    assert_eq!(session.history, history);
    assert_eq!(session.ask("第二问").unwrap(), "【说明】追问解释");
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
    assert!(!requests[0]["input"].to_string().contains("5mr"));
    assert!(requests[1]["input"].to_string().contains("5mr"));
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

#[test]
fn output_validates_sources_values_and_formats_plain_paragraphs() {
    let evidence = review_evidence(&review());
    let valid = json!({"sections": [
        {"source": "calculation", "text": "切赤五万后为 0 向听。", "facts": [
            {"path": "/discards/0/shanten", "value": 0},
            {"path": "/discards/0/discard", "value": "5mr"}
        ]},
        {"source": "limitation", "text": "现有证据不能解释模型偏好的原因。", "facts": []}
    ]});
    assert_eq!(
        output::render(&valid.to_string(), &evidence).unwrap(),
        "【计算】切赤五万后为 0 向听。\n\n【说明】现有证据不能解释模型偏好的原因。"
    );
    for q in [
        0.24627554_f32,
        0.24628288,
        -0.28292805,
        0.08960321,
        0.9,
        0.1,
    ] {
        let mut evidence = evidence.clone();
        evidence["mortal"]["decision"]["candidates"][0]["q_value"] = json!(q);
        let reply = json!({"sections": [{"source": "mortal", "text": "Q 值只表示模型偏好。", "facts": [{
            "path": "/mortal/decision/candidates/0/q_value", "value": q
        }]}]});
        assert!(
            output::render(&reply.to_string(), &evidence).is_ok(),
            "{q}: {reply}"
        );
    }
    for text in [
        "",
        "**加粗**",
        "【计算】重复标签",
        "表|格",
        "<script>",
        "两行\n文字",
        "字面\\n换行",
        "\u{1b}[31m红色",
    ] {
        let mut bad = valid.clone();
        bad["sections"][0]["text"] = json!(text);
        assert!(
            output::render(&bad.to_string(), &evidence).is_err(),
            "{text:?}"
        );
    }
    for (path, value) in [
        ("/discards/0/shanten", json!(1)),
        ("/discards/99/shanten", json!(0)),
        ("/position/remaining_draws", json!(40)),
        ("/discards/99/shanten", Value::Null),
    ] {
        let mut bad = valid.clone();
        bad["sections"][0]["facts"] = json!([{"path": path, "value": value}]);
        assert!(output::render(&bad.to_string(), &evidence).is_err());
    }
    for bad in [
        "普通文本".into(),
        format!("```json\n{valid}\n```"),
        json!({"sections": []}).to_string(),
        json!({"sections": [{"source":"inference", "text":"可能更好", "facts":[]}]}).to_string(),
        json!({"sections": [], "extra": true}).to_string(),
    ] {
        assert!(output::render(&bad, &evidence).is_err());
    }
}

#[test]
fn output_displays_rounded_q_and_chinese_honors_without_changing_evidence() {
    let mut evidence = review_evidence(&review());
    let values = [-2.327948570251465, 0.11272299289703369, -0.00001];
    evidence["mortal"]["decision"]["candidates"] = json!([
        {"q_value": values[0]}, {"q_value": values[1]}
    ]);
    evidence["mortal"]["decision"]["kan_candidates"] = json!([{"q_value": values[2]}]);
    let original = evidence.clone();
    let reply = json!({"sections": [{
        "source": "mortal",
        "text": "Mortal：切 1m 的 Q 为 -2.327948570251465，切 W 为 0.11272299289703369，杠候选为 -0.00001。字牌 E、S、W、N、P、F、C；P0，5mr，其他数值 0.123456。",
        "facts": [
            {"path": "/mortal/decision/candidates/0/q_value", "value": values[0]},
            {"path": "/mortal/decision/candidates/1/q_value", "value": values[1]},
            {"path": "/mortal/decision/kan_candidates/0/q_value", "value": values[2]}
        ]
    }]});
    assert_eq!(
        output::render(&reply.to_string(), &evidence).unwrap(),
        "【Mortal】Mortal：切 1m 的 Q 为 -2.328，切 西 为 0.113，杠候选为 0.000。字牌 东、南、西、北、白、发、中；P0，5mr，其他数值 0.123456。"
    );
    assert_eq!(evidence, original);
    let mut rounded_fact = reply;
    rounded_fact["sections"][0]["facts"][0]["value"] = json!(-2.328);
    assert!(output::render(&rounded_fact.to_string(), &evidence).is_err());
}

#[test]
fn output_groups_effective_tiles_from_evidence() {
    let mut evidence = review_evidence(&review());
    let draws: Vec<_> = [
        ("2m", 4),
        ("3m", 3),
        ("4m", 3),
        ("5m", 4),
        ("6m", 4),
        ("7m", 3),
        ("8m", 4),
        ("9m", 4),
        ("1p", 3),
        ("2p", 4),
        ("3p", 4),
        ("4p", 4),
        ("5p", 3),
        ("6p", 3),
        ("7p", 4),
        ("2s", 4),
        ("3s", 3),
        ("4s", 3),
        ("5s", 4),
        ("6s", 2),
        ("W", 3),
        ("P", 3),
        ("F", 3),
        ("C", 3),
    ]
    .into_iter()
    .map(|(tile, unseen)| json!({"tile": tile, "unseen": unseen}))
    .collect();
    evidence["discards"] = json!([{
        "discard": "1m", "shanten": 5, "draw_kind": "effective", "draws": draws, "total_unseen": 82
    }]);
    let reply = json!({"sections": [{
        "source": "calculation", "text": "切 1m 后为 5 向听。", "draws_for": "1m",
        "facts": [{"path": "/discards/0/shanten", "value": 5}]
    }]});
    assert_eq!(
        output::render(&reply.to_string(), &evidence).unwrap(),
        concat!(
            "【计算】切 1m 后为 5 向听。\n\n切 1m 后的有效牌：\n\n",
            "- 万：2m、5m、6m、8m、9m（各4枚）；3m、4m、7m（各3枚）\n",
            "- 筒：2p、3p、4p、7p（各4枚）；1p、5p、6p（各3枚）\n",
            "- 索：2s、5s（各4枚）；3s、4s（各3枚）；6s（2枚）\n",
            "- 字牌：西、白、发、中（各3枚）\n\n",
            "共 24 种，合计 82 枚不可见牌（包含对手暗牌，并非牌山剩余枚数）。"
        )
    );
    for discard in ["9s", "西"] {
        let mut bad = reply.clone();
        bad["sections"][0]["draws_for"] = json!(discard);
        assert!(output::render(&bad.to_string(), &evidence).is_err());
    }
    let mut bad = reply.clone();
    bad["sections"][0]["source"] = json!("limitation");
    assert!(output::render(&bad.to_string(), &evidence).is_err());
    evidence["discards"][0]["total_unseen"] = json!(81);
    assert!(output::render(&reply.to_string(), &evidence).is_err());
}

#[test]
fn output_preserves_exhausted_winning_shape_tiles() {
    let mut evidence = review_evidence(&review());
    evidence["discards"] = json!([{
        "discard": "W", "draw_kind": "winning_shape",
        "draws": [{"tile": "C", "unseen": 0}], "total_unseen": 0
    }]);
    let reply = json!({"sections": [{
        "source": "calculation", "text": "完成牌形的牌已全部可见。", "draws_for": "W",
        "facts": [{"path": "/discards/0/discard", "value": "W"}]
    }]});
    assert_eq!(
        output::render(&reply.to_string(), &evidence).unwrap(),
        concat!(
            "【计算】完成牌形的牌已全部可见。\n\n切 西 后的完成牌形的牌（不代表可以合法和牌）：\n\n",
            "- 字牌：中（0枚）\n\n共 1 种，合计 0 枚不可见牌（包含对手暗牌，并非牌山剩余枚数）。"
        )
    );
}

#[test]
fn unanalysed_context_replays_only_requested_history_without_mortal() {
    let log =
        convlog::tenhou::Log::from_json_str(include_str!("../../fixtures/tenhou/ranked_game.json"))
            .unwrap();
    let events = convlog::tenhou_to_mjai(&log).unwrap();
    let player = PlayerIndex::new(0).unwrap();
    let context = AgentContext::from_events(&events, player, 2).unwrap();
    let evidence = context.evidence();
    assert_eq!(
        evidence["position"]["concealed"],
        json!([
            "3m", "7m", "8m", "1p", "1p", "2p", "4p", "5p", "7s", "8s", "9s", "E", "C", "C"
        ])
    );
    assert_eq!(evidence["mortal"]["status"], "not_analyzed");
    assert!(evidence["mortal"]["model"].is_null());
    assert_eq!(evidence["analysis_status"], "not_analyzed");
    for public in evidence["position"]["players"].as_array().unwrap() {
        assert!(public.get("concealed").is_none());
        assert_eq!(public["discards"], json!([]));
    }
    assert_eq!(
        evidence,
        AgentContext::from_events(&events[..3], player, 2)
            .unwrap()
            .evidence()
    );
    assert!(matches!(
        AgentContext::from_events(&events, player, events.len()),
        Err(ReviewError::EventOutOfRange { .. })
    ));
    assert!(matches!(
        AgentContext::from_events(&events, player, 0),
        Err(ReviewError::NoRound { .. })
    ));
    let text = json!({"sections":[{"source":"limitation","text":"切牌效率未分析。","facts":[]}]})
        .to_string();
    assert_eq!(
        output::render(&text, evidence).unwrap(),
        "【说明】切牌效率未分析。\n\n【Mortal】未分析。"
    );
    for source in ["mortal", "calculation"] {
        assert!(
            output::render(
                &json!({"sections":[{"source":source,"text":"编造推荐","facts":[]}]}).to_string(),
                evidence
            )
            .is_err()
        );
    }
    let mut review = review();
    review.decision = None;
    let rendered = output::render(&text, &review_evidence(&review)).unwrap();
    assert!(rendered.contains("没有该玩家的决策结果"));
    assert!(!rendered.contains("【Mortal】未分析"));
}

#[test]
fn invalid_answer_gets_bounded_correction_and_failed_turn_rolls_back() {
    let evidence = review_evidence(&review());
    let mut requests = 0;
    let (text, history) = answer(&evidence, &[], true, "解释", |input, _| {
        requests += 1;
        if requests == 1 {
            Ok(response(vec![raw_message("```json\n坏格式\n```")]))
        } else {
            assert_eq!(input.last().unwrap()["role"], "developer");
            Ok(response(vec![message("现有证据不足。")]))
        }
    })
    .unwrap();
    assert_eq!(requests, 2);
    assert_eq!(text, "【说明】现有证据不足。");
    assert!(!json!(history).to_string().contains("坏格式"));
    assert!(!json!(history).to_string().contains("回答校验失败"));
    let bad = response(vec![raw_message("不是 JSON")]).to_string();
    let (endpoint, handle) = server(vec![(200, bad); 3]);
    let mut session = AgentSession::new(
        &review(),
        &AgentConfig {
            endpoint: &endpoint,
            model: "test",
            api_key: None,
        },
    )
    .unwrap();
    session.has_evidence = true;
    let history = session.history.clone();
    assert!(matches!(
        session.ask("解释"),
        Err(AgentError::InvalidResponse { .. })
    ));
    assert_eq!(session.history, history);
    assert_eq!(handle.join().unwrap().len(), 3);
}

/// 手动复测真实模型时，只打印校验原因和通过校验的答案，不输出配置或原始响应。
#[test]
#[ignore = "使用本机 LLM 配置发送公开 fixture，可能产生调用费用"]
fn live_issue9_two_turn_explanation() {
    let mut file_config = std::collections::HashMap::new();
    if let Ok(entries) = dotenvy::from_path_iter(".env") {
        for entry in entries {
            let (key, value) = entry.unwrap_or_else(|_| panic!("无法解析本机 .env"));
            file_config.entry(key).or_insert(value);
        }
    }
    let read = |key: &str| {
        std::env::var(key)
            .ok()
            .or_else(|| file_config.get(key).cloned())
    };
    let endpoint = read("KYOKU_OPENAI_ENDPOINT");
    let key = read(if endpoint.is_some() {
        "AGENT_API_KEY"
    } else {
        "OPENAI_API_KEY"
    });
    let model = read("OPENAI_MODEL").expect("需要配置 OPENAI_MODEL");
    let client = client::Client::new(&AgentConfig {
        endpoint: endpoint
            .as_deref()
            .unwrap_or("https://api.openai.com/v1/responses"),
        model: &model,
        api_key: key.as_deref(),
    })
    .unwrap();
    let log =
        convlog::tenhou::Log::from_json_str(include_str!("../../fixtures/tenhou/ranked_game.json"))
            .unwrap();
    let events = convlog::tenhou_to_mjai(&log).unwrap();
    let review = crate::review::review_at(
        &events,
        PlayerIndex::new(0).unwrap(),
        2,
        &crate::mortal::MortalConfig {
            python: std::path::Path::new("mortal/.venv/bin/python"),
            runtime: std::path::Path::new("mortal/runtime"),
            checkpoint: std::path::Path::new("mortal/models/mortal_582500.pth"),
        },
    )
    .unwrap();
    let evidence = review_evidence(&review);
    let mut history = Vec::new();
    for question in [
        "现在我的手牌和牌河是啥样的？",
        "为什么这里不切 3m 呢？反正是个浮牌。或者东风也行？",
    ] {
        let (text, next) = answer(
            &evidence,
            &history,
            !history.is_empty(),
            question,
            |input, forced| {
                let response = client.respond(input, forced)?;
                for item in response["output"].as_array().into_iter().flatten() {
                    if item["type"] != "message" {
                        continue;
                    }
                    for part in item["content"].as_array().into_iter().flatten() {
                        if let Some(text) = part["text"].as_str()
                            && let Err(reason) = output::render(text, &evidence)
                        {
                            eprintln!("在线回答校验：{reason}");
                        }
                    }
                }
                Ok(response)
            },
        )
        .unwrap();
        println!("问题：{question}\n{text}\n");
        history = next;
    }
}
