//! 真实牌谱的手动验收。输入和完整执行记录留在本地，不提交牌谱或服务配置。

use super::*;
use std::path::Path;

#[test]
#[ignore = "需指定实战局面的可见快照及工具请求；不调用 LLM"]
fn replay_selected_facts() {
    let plan = std::env::var("KYOKU_FACT_CASES").expect("需要 KYOKU_FACT_CASES");
    let output_dir = std::env::var("KYOKU_LIVE_OUTPUT").expect("需要 KYOKU_LIVE_OUTPUT");
    std::fs::create_dir_all(&output_dir).unwrap();
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(plan).unwrap()).unwrap();
    for (index, case) in cases.iter().enumerate() {
        let saved: Value = serde_json::from_str(
            &std::fs::read_to_string(case["record"].as_str().unwrap()).unwrap(),
        )
        .unwrap();
        let mut output = Vec::new();
        for request in case["requests"].as_array().unwrap() {
            let result = strategy::execute(
                request["name"].as_str().unwrap(),
                &saved["evidence"],
                &request["args"].to_string(),
            );
            assert_eq!(result["ok"], true, "{request}: {result}");
            output.push(json!({"request":request,"result":result}));
        }
        std::fs::write(
            Path::new(&output_dir).join(format!("selected-{index}.json")),
            serde_json::to_vec_pretty(&output).unwrap(),
        )
        .unwrap();
    }
}

#[test]
#[ignore = "需指定本地公开牌谱目录；不调用 LLM 或 Mortal"]
fn replay_fact_sweep() {
    let directory = std::env::var("KYOKU_REPLAY_DIR").expect("需要 KYOKU_REPLAY_DIR");
    let output_dir = std::env::var("KYOKU_LIVE_OUTPUT").expect("需要 KYOKU_LIVE_OUTPUT");
    std::fs::create_dir_all(&output_dir).unwrap();
    let mut paths: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    let mut records = Vec::new();
    let mut failures = Vec::new();
    for (game_index, path) in paths.iter().enumerate() {
        let log =
            convlog::tenhou::Log::from_json_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let events = convlog::tenhou_to_mjai(&log).unwrap();
        let player = PlayerIndex::new((game_index % 4) as u8).unwrap();
        let mut replay = Replayer::new();
        let mut eligible = Vec::new();
        for (event_index, event) in events.iter().enumerate() {
            replay.apply(event).unwrap();
            if let Some(state) = replay.state()
                && matches!(state.phase(),RoundPhase::AfterDraw{player:active,..} if active==player)
                && state.player(player).riichi() == RiichiState::NotDeclared
                && state.remaining_draws() > 0
            {
                eligible.push(event_index);
            }
        }
        if eligible.is_empty() {
            continue;
        }
        let mut selected = vec![
            eligible[0],
            eligible[eligible.len() / 2],
            *eligible.last().unwrap(),
        ];
        selected.dedup();
        for event in selected {
            let mut evidence = AgentContext::from_events(&events, player, event)
                .unwrap()
                .evidence()
                .clone();
            let snapshot = position::Snapshot::read(&evidence).unwrap();
            let mut choices = snapshot.hand.concealed().to_vec();
            choices.dedup();
            if choices.len() < 2 {
                continue;
            }
            // 自由摸切阶段的真实手牌均可作候选；这里只扫纯计算，不伪造模型推荐。
            evidence["discards"] = json!(
                choices
                    .iter()
                    .copied()
                    .map(|tile| json!({"discard":crate::replay::inspector::format_tile(tile)}))
                    .collect::<Vec<_>>()
            );
            let first = crate::replay::inspector::format_tile(choices[0]);
            let second = crate::replay::inspector::format_tile(*choices.last().unwrap());
            let draw = snapshot
                .unseen
                .iter()
                .position(|&copies| copies > 0)
                .map(|kind| crate::replay::inspector::format_tile(Tile::new(kind as u8).unwrap()));
            for draw in [None, draw] {
                let args = json!({"first":first,"second":second,"draw":draw});
                let start = Instant::now();
                let result =
                    strategy::execute("compare_discard_facts", &evidence, &args.to_string());
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                if result["ok"] != true {
                    failures.push(json!({"replay":path,"event":event,"result":result}));
                }
                let size = serde_json::to_vec(&result).unwrap().len();
                records.push(
                    json!({"replay":path,"player":player.get_id(),"event":event,"args":args,
                    "elapsed_ms":elapsed,"bytes":size,"ok":result["ok"],"error":result["error"]}),
                );
            }
        }
        eprintln!(
            "已扫描 {}：累计{}次完整比较",
            path.file_name().unwrap().to_string_lossy(),
            records.len()
        );
    }
    assert!(!records.is_empty());
    std::fs::write(
        Path::new(&output_dir).join("sweep.json"),
        serde_json::to_vec_pretty(&records).unwrap(),
    )
    .unwrap();
    assert!(failures.is_empty(), "{}", json!(failures));
}

#[test]
#[ignore = "需指定本地公开牌谱用例；使用已配置 LLM，可能产生调用费用"]
fn live_replay_fact_comparisons() {
    let request_limit = std::env::var("KYOKU_LIVE_REQUEST_LIMIT")
        .map(|value| value.parse::<usize>().expect("请求上限必须是正整数"))
        .unwrap_or(MAX_REQUESTS);
    assert!((1..=MAX_REQUESTS).contains(&request_limit));
    let plan_path = std::env::var("KYOKU_LIVE_CASES").expect("需要 KYOKU_LIVE_CASES");
    let output_dir = std::env::var("KYOKU_LIVE_OUTPUT").expect("需要 KYOKU_LIVE_OUTPUT");
    let cases: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(plan_path).unwrap()).unwrap();
    std::fs::create_dir_all(&output_dir).unwrap();
    let client = live_client();
    let config = crate::mortal::MortalConfig {
        python: Path::new("mortal/.venv/bin/python"),
        runtime: Path::new("mortal/runtime"),
        checkpoint: Path::new("mortal/models/mortal_582500.pth"),
    };
    let mut failures = Vec::new();
    'cases: for (case_index, case) in cases.iter().enumerate() {
        let label = case["label"].as_str().unwrap();
        let evidence = if let Some(record) = case["record"].as_str() {
            // 复测已发生的问答时保留当时的快照与Mortal结果，工具仍使用当前实现。
            let saved: Value =
                serde_json::from_str(&std::fs::read_to_string(record).unwrap()).unwrap();
            saved["evidence"].clone()
        } else {
            let log = convlog::tenhou::Log::from_json_str(
                &std::fs::read_to_string(case["replay"].as_str().unwrap()).unwrap(),
            )
            .unwrap();
            let events = convlog::tenhou_to_mjai(&log).unwrap();
            let player = PlayerIndex::new(case["player"].as_u64().unwrap() as u8).unwrap();
            let event = case["event"].as_u64().unwrap() as usize;
            let review = crate::review::review_at(&events, player, event, &config).unwrap();
            review_evidence(&review)
        };
        let mut history = Vec::new();
        for (question_index, question) in case["questions"].as_array().unwrap().iter().enumerate() {
            let question = question.as_str().unwrap();
            eprintln!("开始 {label} 问题{}：{question}", question_index + 1);
            let start = Instant::now();
            let mut trace = Vec::new();
            let mut requests = 0;
            let result = answer_traced(
                &evidence,
                &history,
                !history.is_empty(),
                question,
                |input, mode| {
                    if requests >= request_limit {
                        return Err(invalid("manual test request budget exhausted"));
                    }
                    requests += 1;
                    eprintln!("请求{requests}开始");
                    let result = client.respond(input, mode);
                    if let Ok(response) = &result {
                        eprintln!("请求{requests}完成 {}", response["_metrics"]);
                        for item in response["output"].as_array().into_iter().flatten() {
                            if item["type"] == "function_call" {
                                eprintln!("计划调用 {} {}", item["name"], item["arguments"]);
                            }
                        }
                    }
                    result
                },
                &mut trace,
            );
            let stop = matches!(
                &result,
                Err(AgentError::Http {
                    status: 401..=403,
                    ..
                })
            );
            let (text, error) = match result {
                Ok((text, next)) => {
                    history = next;
                    (Some(text), None)
                }
                Err(error) => {
                    failures.push(format!("{label} 问题{}：{error}", question_index + 1));
                    (None, Some(error.to_string()))
                }
            };
            let failed = error.is_some();
            for step in &trace {
                if step["kind"] == "tool" {
                    eprintln!(
                        "工具 {} {} 成功={}",
                        step["name"], step["arguments"], step["result"]["ok"]
                    );
                } else if step["kind"] == "validation" {
                    eprintln!("校验：{}", step["error"]);
                }
            }
            eprintln!(
                "完成 {label} 问题{}：{:.2}s\n{}",
                question_index + 1,
                start.elapsed().as_secs_f64(),
                text.as_deref().or(error.as_deref()).unwrap()
            );
            let record = json!({"case":case,"question_index":question_index,"question":question,
                "elapsed_ms":start.elapsed().as_millis(),"answer":text,"error":error,"evidence":evidence,"trace":trace});
            std::fs::write(
                Path::new(&output_dir).join(format!("{case_index}-{question_index}.json")),
                serde_json::to_vec_pretty(&record).unwrap(),
            )
            .unwrap();
            if stop {
                break 'cases;
            }
            // 失败回答没有进入会话历史；不能继续问依赖“刚才两边”的追问。
            if failed {
                break;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
