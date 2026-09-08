use super::*;

fn parse(input: &str) -> Result<Option<Args>, Box<dyn Error>> {
    Args::parse(input.split_whitespace().map(str::to_owned))
}

#[test]
fn cli_requires_valid_position_and_resolves_stdin_ownership() {
    for input in [
        "",
        "--player 0 log",
        "--event 2 log",
        "--player 4 --event 2 log",
        "--player 0 --event -1 log",
        "--player 0 --event 2 a b",
        "--player 0 --event 2 --unknown log",
        "--player 0 --event 2 -",
        "--player 0 --event 2 --question why --interactive -",
    ] {
        assert!(parse(input).is_err(), "{input}");
    }
    let args = parse("--player 0 --event 2 log").unwrap().unwrap();
    assert!(args.interactive);
    assert!(args.question.is_none());
    let args = parse("--player 0 --event 2 --question why -")
        .unwrap()
        .unwrap();
    assert!(!args.interactive);
    assert_eq!(args.input, "-");
    assert!(parse("--help").unwrap().is_none());
}

#[test]
fn cli_keeps_mortal_and_llm_configuration_separate() {
    let args = parse("--player 2 --event 12 --model weights.pth --llm-model model-name --llm-config options.json --endpoint https://example.com/v1/responses --question why --interactive log").unwrap().unwrap();
    assert_eq!(args.model, PathBuf::from("weights.pth"));
    assert_eq!(args.llm_model.as_deref(), Some("model-name"));
    assert_eq!(args.llm_config, Some(PathBuf::from("options.json")));
    assert_eq!(args.question.as_deref(), Some("why"));
    assert!(args.interactive);
    assert_eq!(args.player.get_id(), 2);
    assert_eq!(args.event, Some(12));
}

#[test]
fn browse_requires_a_file_and_keeps_single_position_options_exclusive() {
    let args = parse("--player 0 --browse log").unwrap().unwrap();
    assert!(args.browse);
    assert!(args.event.is_none());
    assert!(args.interactive);
    for input in [
        "--player 0 --browse -",
        "--player 0 --browse --event 2 log",
        "--player 0 --browse --question why log",
        "--player 0 --browse --without-mortal log",
    ] {
        assert!(parse(input).is_err(), "{input}");
    }
}

#[test]
fn unanalysed_single_position_does_not_require_mortal_paths() {
    let args = parse("--player 0 --event 2 --without-mortal --python missing --question why log")
        .unwrap()
        .unwrap();
    assert!(args.without_mortal);
    assert!(!args.browse);
    assert!(!args.interactive);
}

fn points() -> Vec<kyoku::review::DecisionPoint> {
    [2, 12]
        .into_iter()
        .map(|event_index| {
            let mut review = review();
            review.event_index = event_index;
            kyoku::review::DecisionPoint {
                review,
                turn: 1,
                actual: kyoku::review::RecordedAction::Passed,
            }
        })
        .collect()
}

#[test]
fn browsing_switches_cached_positions_without_llm_and_handles_invalid_commands() {
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();
    browse::conversation(&points(), None,
        &b"/prev\n/select 12\n/next\n/select 999\n/select 2 extra\n/select\n/nope\n/show\n/prev\n/list\n/quit\nignored\n"[..],
        &mut output, &mut diagnostics, false).unwrap();
    let output = String::from_utf8(output).unwrap();
    assert_eq!(output.matches("G002 after event").count(), 2);
    assert_eq!(output.matches("G012 after event").count(), 2);
    assert_eq!(output.matches("问答上下文已重置").count(), 2);
    assert!(!output.contains("ignored"));
    let diagnostics = String::from_utf8(diagnostics).unwrap();
    assert_eq!(diagnostics.matches("边界").count(), 2);
    assert_eq!(diagnostics.matches("当前局面保持不变").count(), 3);
    assert!(diagnostics.contains("未知命令"));
    let mut output = Vec::new();
    browse::conversation(&[], None, &b"question"[..], &mut output, Vec::new(), false).unwrap();
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("共 0 个行动机会")
    );
    browse::conversation(&points(), None, &b""[..], Vec::new(), Vec::new(), false).unwrap();
}

#[test]
fn switching_rebuilds_tool_evidence_without_llm_configuration() {
    let invalid = AgentConfig {
        endpoint: "invalid",
        model: "",
        api_key: None,
        options: Default::default(),
    };
    for config in [None, Some(&invalid)] {
        let mut output = Vec::new();
        let mut diagnostics = Vec::new();
        browse::conversation(
            &points(),
            config,
            &b"/evidence\n/next\n/evidence\n/select 2\n/evidence\n/quit\n"[..],
            &mut output,
            &mut diagnostics,
            false,
        )
        .unwrap();
        let output = String::from_utf8(output).unwrap();
        assert_eq!(output.matches("\"event_index\": 2,").count(), 2);
        assert_eq!(output.matches("\"event_index\": 12,").count(), 1);
        assert!(diagnostics.is_empty());
    }
}

#[test]
fn browse_questions_reset_history_on_switch_but_keep_it_on_invalid_selection() {
    use serde_json::{Value, json};
    use std::{
        io::BufReader,
        net::TcpListener,
        thread,
        time::{Duration, Instant},
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let answer = json!({"status":"completed","output":[{"type":"message","status":"completed","role":"assistant","content":[{"type":"output_text","text":json!({"sections":[{"source":"limitation","text":"当前证据不足。","facts":[]}]}).to_string()}]}]}).to_string();
        let mut requests = Vec::<Value>::new();
        // 首问和追问均可直接回答，切换时只保留新局面证据。
        for body in [&answer; 4] {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "等待浏览问答请求超时");
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
            let mut length = None;
            loop {
                let mut line = String::new();
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
            let mut bytes = vec![0; length.unwrap()];
            reader.read_exact(&mut bytes).unwrap();
            requests.push(serde_json::from_slice(&bytes).unwrap());
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
        requests
    });
    let config = AgentConfig {
        endpoint: &endpoint,
        model: "test",
        api_key: None,
        options: Default::default(),
    };
    browse::conversation(&points(), Some(&config),
        &b"first-position-question\n/select 999\n/select 2\nfollow-up\n/next\nsecond-position-question\n/prev\nreturn-question\n/quit\n"[..],
        Vec::new(), Vec::new(), false).unwrap();
    let requests = handle.join().unwrap();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[1]["tool_choice"], "auto");
    assert!(requests[1].to_string().contains("first-position-question"));
    for (first, event_index) in [(0, 2), (2, 12), (3, 2)] {
        assert_eq!(requests[first]["input"].as_array().unwrap().len(), 2);
        assert_eq!(requests[first]["tool_choice"], "auto");
        assert!(requests[first]["tools"].as_array().unwrap().len() > 1);
        let content = requests[first]["input"][0]["content"]
            .as_str()
            .unwrap()
            .split_once('：')
            .unwrap();
        let evidence: Value = serde_json::from_str(content.1).unwrap();
        assert_eq!(evidence["event_index"], event_index);
        assert!(evidence.get("actual").is_none());
        assert!(
            !requests[first]
                .to_string()
                .contains("second-position-question")
                || event_index == 12
        );
    }
}

fn review() -> kyoku::review::Review {
    use kyoku::{
        mahjong::{
            player::RiichiState,
            round::{RoundId, RoundPhase, Wind},
        },
        mortal::ModelInfo,
        review::{PublicPlayer, Review, VisiblePosition},
    };

    Review {
        event_index: 2,
        player: PlayerIndex::new(0).unwrap(),
        position: VisiblePosition {
            history: None,
            round: RoundId::new(Wind::East, 1).unwrap(),
            honba: 0,
            riichi_sticks: 0,
            remaining_draws: 70,
            phase: RoundPhase::Initial,
            dora_indicators: vec![],
            concealed: vec![],
            players: std::array::from_fn(|_| PublicPlayer {
                score: 25_000,
                riichi: RiichiState::NotDeclared,
                discards: vec![],
                melds: vec![],
            }),
        },
        model: ModelInfo {
            version: 4,
            tag: "test".into(),
            sha256: "0".repeat(64),
        },
        decision: None,
        discards: vec![],
    }
}

#[test]
fn interactive_evidence_quit_and_eof_need_no_llm_configuration() {
    let review = review();
    let invalid = AgentConfig {
        endpoint: "invalid",
        model: "",
        api_key: None,
        options: Default::default(),
    };
    for config in [None, Some(&invalid)] {
        let mut session = None;
        let mut output = Vec::new();
        let mut diagnostics = Vec::new();
        conversation(
            &AgentContext::from(&review),
            config,
            &mut session,
            &b"\n/evidence\n/quit\nignored\n"[..],
            &mut output,
            &mut diagnostics,
            false,
        )
        .unwrap();
        let evidence: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(evidence, review_evidence(&review));
        assert!(diagnostics.is_empty());
        assert!(session.is_none());
        output.clear();
        conversation(
            &AgentContext::from(&review),
            config,
            &mut session,
            &b""[..],
            &mut output,
            &mut diagnostics,
            false,
        )
        .unwrap();
        assert!(output.is_empty());
    }
}

#[test]
fn missing_llm_configuration_blocks_questions_but_not_subsequent_evidence() {
    let mut session = None;
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();
    conversation(
        &AgentContext::from(&review()),
        None,
        &mut session,
        &b"why\n/evidence\n/quit\n"[..],
        &mut output,
        &mut diagnostics,
        false,
    )
    .unwrap();
    assert!(session.is_none());
    assert!(
        String::from_utf8(diagnostics)
            .unwrap()
            .contains("OPENAI_MODEL")
    );
    let evidence: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(evidence["event_index"], 2);
}

#[test]
fn default_endpoint_reads_only_openai_credentials() {
    let (endpoint, key) = connection_settings(None, |name| match name {
        "KYOKU_OPENAI_ENDPOINT" => None,
        "OPENAI_API_KEY" => Some("test-openai-key".into()),
        _ => panic!("默认地址不应读取 {name}"),
    });
    assert_eq!(endpoint, "https://api.openai.com/v1/responses");
    assert_eq!(key.as_deref(), Some("test-openai-key"));
}

#[test]
fn endpoint_overrides_never_read_or_fall_back_to_openai_credentials() {
    for endpoint in [
        "https://example.com/v1/responses",
        "http://localhost:8080/v1/responses",
        "https://api.openai.com/v1/responses",
    ] {
        for from_cli in [false, true] {
            for agent_key in [None, Some("test-custom-key"), Some("")] {
                let (actual_endpoint, key) =
                    connection_settings(from_cli.then_some(endpoint), |name| match name {
                        "KYOKU_OPENAI_ENDPOINT" if !from_cli => Some(endpoint.into()),
                        "AGENT_API_KEY" => agent_key.map(str::to_owned),
                        _ => panic!("覆盖地址不应读取 {name}"),
                    });
                assert_eq!(actual_endpoint, endpoint);
                assert_eq!(key.as_deref(), agent_key);
            }
        }
    }
}

#[test]
fn custom_http_requests_send_only_explicit_custom_credentials() {
    use std::{
        io::BufReader,
        net::TcpListener,
        thread,
        time::{Duration, Instant},
    };

    for from_cli in [false, true] {
        for agent_key in [None, Some("test-custom-key")] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let custom_endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
            let server = thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
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
                let mut headers = String::new();
                let mut length = None;
                let mut authorization = None;
                loop {
                    let mut line = String::new();
                    assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                    headers.push_str(&line);
                    if line == "\r\n" {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':') {
                        if name.eq_ignore_ascii_case("content-length") {
                            length = Some(value.trim().parse::<usize>().unwrap());
                        } else if name.eq_ignore_ascii_case("authorization") {
                            authorization = Some(value.trim().to_owned());
                        }
                    }
                }
                let mut body = vec![0; length.unwrap()];
                reader.read_exact(&mut body).unwrap();
                assert!(!headers.contains("test-openai-key"));
                assert!(!String::from_utf8(body).unwrap().contains("test-openai-key"));
                stream.write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                authorization
            });
            let (endpoint, key) = connection_settings(
                from_cli.then_some(custom_endpoint.as_str()),
                |name| match name {
                    "KYOKU_OPENAI_ENDPOINT" => Some(custom_endpoint.clone()),
                    "OPENAI_API_KEY" => Some("test-openai-key".into()),
                    "AGENT_API_KEY" => agent_key.map(str::to_owned),
                    _ => None,
                },
            );
            let mut session = AgentSession::new(
                &review(),
                &AgentConfig {
                    endpoint: &endpoint,
                    model: "test",
                    api_key: key.as_deref(),
                    options: Default::default(),
                },
            )
            .unwrap();
            assert!(matches!(
                session.ask("分析"),
                Err(kyoku::agent::AgentError::Http {
                    status: 401,
                    error: None
                })
            ));
            assert_eq!(
                server.join().unwrap(),
                agent_key.map(|key| format!("Bearer {key}"))
            );
        }
    }
}
