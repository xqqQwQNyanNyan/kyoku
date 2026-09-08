//! 手动问答实验使用指定的可见快照，不读取完整牌谱。
use super::*;
use std::path::PathBuf;

#[test]
#[ignore = "三个可见局面的完整问答；使用当前配置，可能产生调用费用"]
fn replay_verified_questions() {
    use super::super::{AgentContext, AgentSession};
    use std::io::Write;

    let path = PathBuf::from(std::env::var("KYOKU_VERIFICATION_PLAN").expect("需要实验计划路径"));
    let plan: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let cases = plan["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3, "只运行三道问题，不自动重试");
    let preview = std::env::var("KYOKU_VERIFICATION_PREVIEW").as_deref() == Ok("1");
    let directory = path.parent().unwrap();
    let api_key = std::env::var("KYOKU_VERIFICATION_KEY").ok();
    let config = AgentConfig {
        endpoint: plan["endpoint"].as_str().unwrap(),
        model: plan["model"].as_str().unwrap(),
        api_key: if preview {
            Some("preview-only")
        } else {
            api_key.as_deref()
        },
        options: serde_json::from_value(plan["options"].clone()).unwrap(),
    };
    if !preview {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("started"))
            .expect("已有实验记录，禁止重复发请求");
    }
    for (index, case) in cases.iter().enumerate() {
        let context = AgentContext {
            evidence: case["evidence"].clone(),
        };
        let question = case["question"].as_str().unwrap();
        let mut session = AgentSession::with_context(&context, &config).unwrap();
        if preview {
            let mut requests = 0;
            let result = super::super::answer_controlled(
                &context.evidence,
                &[],
                false,
                question,
                |input, mode| {
                    requests += 1;
                    let request = session.client.request(input, mode)?;
                    std::fs::write(
                        directory.join(format!("initial-request-{index}.json")),
                        serde_json::to_vec_pretty(&request).unwrap(),
                    )
                    .unwrap();
                    Err(invalid("offline preview stops before HTTP"))
                },
                &mut Vec::new(),
                &QuestionControl::default(),
            );
            assert!(result.is_err());
            assert_eq!(requests, 1);
            continue;
        }
        let start = Instant::now();
        let progress_path = directory.join(format!("progress-{index}.jsonl"));
        let control = QuestionControl::new(move |stage| {
            let entry = json!({"elapsed_ms":start.elapsed().as_millis(),"stage":stage});
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&progress_path)
                .unwrap();
            writeln!(file, "{entry}").unwrap();
            eprintln!("问题{}：{entry}", index + 1);
        });
        // 直接走正式会话入口，由模型自行选择工具并生成草稿、核查终稿。
        let result = session.ask_with_control(question, &control);
        let (answer, error) = match result {
            Ok(answer) => (Some(answer), None),
            Err(error) => (None, Some(error.to_string())),
        };
        std::fs::write(
            directory.join(format!("result-{index}.json")),
            serde_json::to_vec_pretty(&json!({"case":case["id"],"question":question,
                "elapsed_ms":start.elapsed().as_millis(),"answer":answer,"error":error,
                "archive":session.archive()}))
            .unwrap(),
        )
        .unwrap();
        eprintln!(
            "问题{}完成，耗时{}毫秒，成功={}",
            index + 1,
            start.elapsed().as_millis(),
            error.is_none()
        );
    }
}

#[test]
#[ignore = "读取指定的公开证据与草稿；实际运行两条或六条核查请求，可能产生费用"]
fn replay_verification_pair() {
    let path = PathBuf::from(std::env::var("KYOKU_VERIFICATION_PLAN").expect("需要实验计划路径"));
    let plan: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let cases = plan["cases"].as_array().unwrap();
    assert!(
        matches!(cases.len(), 2 | 6),
        "实验只允许两条或六条，不自动重试"
    );
    let preview = std::env::var("KYOKU_VERIFICATION_PREVIEW").as_deref() == Ok("1");
    let directory = path.parent().unwrap();
    let api_key = std::env::var("KYOKU_VERIFICATION_KEY").ok();
    let client = Client::new(&AgentConfig {
        endpoint: plan["endpoint"].as_str().unwrap(),
        model: plan["model"].as_str().unwrap(),
        api_key: if preview {
            Some("preview-only")
        } else {
            api_key.as_deref()
        },
        options: serde_json::from_value(plan["options"].clone()).unwrap(),
    })
    .unwrap();
    if !preview {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("started"))
            .expect("已有实验记录，禁止自动重复发请求");
    }
    for (index, case) in cases.iter().enumerate() {
        client.reset_usage();
        let evidence = case.get("evidence").unwrap_or(&plan["evidence"]);
        let default_tools =
            json!([{"name":"compare_discards","args":{"first":"6s","second":"7m","draw":null}}]);
        let calls = case
            .get("tools")
            .unwrap_or(&default_tools)
            .as_array()
            .unwrap();
        let draft = case["draft"].as_str().unwrap();
        let message = json!({"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":draft}]});
        let mut trace = Vec::new();
        for call in calls {
            let name = call["name"].as_str().unwrap();
            let arguments = call["args"].to_string();
            let (result, _) = super::super::execute_tool(evidence, name, &arguments);
            assert_eq!(result["ok"], true, "{result}");
            trace.push(json!({"kind":"tool","name":name,"arguments":arguments,"result":result}));
        }
        trace.push(
            json!({"kind":"response","output":{"status":"completed","output":[message.clone()]}}),
        );
        let start = Instant::now();
        let result = super::super::verification::finish(
            evidence,
            case["question"].as_str().unwrap(),
            draft.into(),
            vec![message],
            &mut trace,
            |input, mode| {
                assert_eq!(mode, RequestMode::Verification);
                let request = client.request(input, mode)?;
                std::fs::write(
                    directory.join(format!("request-{index}.json")),
                    serde_json::to_vec_pretty(&request).unwrap(),
                )
                .unwrap();
                if preview {
                    Ok(
                        json!({"status":"completed","output":[{"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"预览未联网"}]}]}),
                    )
                } else {
                    client.respond(input, mode)
                }
            },
            &QuestionControl::default(),
        );
        if !preview {
            let (answer, error) = match result {
                Ok((text, _)) => (Some(text), None),
                Err(error) => (None, Some(error.to_string())),
            };
            std::fs::write(directory.join(format!("result-{index}.json")),serde_json::to_vec_pretty(&json!({"case":case["id"],"elapsed_ms":start.elapsed().as_millis(),"answer":answer,"error":error,"usage":client.usage(),"trace":trace})).unwrap()).unwrap();
        }
    }
}
