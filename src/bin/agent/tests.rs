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
    let args = parse("--player 2 --event 12 --model weights.pth --llm-model model-name --endpoint https://example.com/v1/responses --question why --interactive log").unwrap().unwrap();
    assert_eq!(args.model, PathBuf::from("weights.pth"));
    assert_eq!(args.llm_model.as_deref(), Some("model-name"));
    assert_eq!(args.question.as_deref(), Some("why"));
    assert!(args.interactive);
    assert_eq!(args.player.get_id(), 2);
    assert_eq!(args.event, 12);
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
fn interactive_evidence_quit_and_eof_need_no_http_request() {
    // 无服务监听；只查看证据和退出不应尝试联网。
    let mut session = AgentSession::new(
        &review(),
        &AgentConfig {
            endpoint: "http://127.0.0.1:1/v1/responses",
            model: "test",
            api_key: None,
        },
    )
    .unwrap();
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();
    conversation(
        &mut session,
        &b"\n/evidence\n/quit\nignored\n"[..],
        &mut output,
        &mut diagnostics,
        false,
    )
    .unwrap();
    let evidence: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(evidence["event_index"], 2);
    assert!(diagnostics.is_empty());
    output.clear();
    conversation(&mut session, &b""[..], &mut output, &mut diagnostics, false).unwrap();
    assert!(output.is_empty());
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
                },
            )
            .unwrap();
            assert!(matches!(
                session.ask("分析"),
                Err(kyoku::agent::AgentError::Http { status: 401 })
            ));
            assert_eq!(
                server.join().unwrap(),
                agent_key.map(|key| format!("Bearer {key}"))
            );
        }
    }
}
