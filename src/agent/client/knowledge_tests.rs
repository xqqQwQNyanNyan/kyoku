//! 解释阶段的单次对照实验；仅手动运行，输入与输出保留在本地。

use super::*;
use std::path::Path;

#[test]
#[ignore = "需要本地实验计划；live 模式最多请求两次，可能产生费用"]
fn replay_knowledge_pair() {
    let plan_path = std::env::var("KYOKU_KNOWLEDGE_PLAN").unwrap();
    let plan: Value = serde_json::from_str(&std::fs::read_to_string(plan_path).unwrap()).unwrap();
    let directory = plan["output_directory"].as_str().unwrap();
    let directory = Path::new(directory);
    std::fs::create_dir_all(directory).unwrap();
    let endpoint = std::env::var("KYOKU_OPENAI_ENDPOINT").unwrap();
    let model = std::env::var("OPENAI_MODEL").unwrap();
    let key = std::env::var("AGENT_API_KEY").unwrap();
    let options: ModelOptions =
        serde_json::from_str(&std::env::var("KYOKU_AGENT_OPTIONS").unwrap()).unwrap();
    let client = Client::new(&AgentConfig {
        endpoint: &endpoint,
        model: &model,
        api_key: Some(&key),
        options,
    })
    .unwrap();
    let cases = plan["cases"].as_array().unwrap();
    assert!((1..=2).contains(&cases.len()));
    let live = std::env::var("KYOKU_KNOWLEDGE_MODE").unwrap() == "live";
    for (index, case) in cases.iter().enumerate() {
        let original = case["input"].as_array().unwrap();
        let input = if case["organize_context"] == true {
            super::super::request_context::prepare(original)
        } else {
            original.clone()
        };
        let request = client.request(&input, RequestMode::Analysis).unwrap();
        let preview = directory.join(format!("request-{index}.json"));
        if !live {
            std::fs::write(preview, serde_json::to_vec_pretty(&request).unwrap()).unwrap();
            continue;
        }
        let checked: Value = serde_json::from_slice(&std::fs::read(preview).unwrap()).unwrap();
        assert_eq!(request, checked, "实际请求必须与离线检查版本相同");
        // 请求前独占创建标记，失败或中断后也不能意外重发。
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(format!("attempt-{index}")))
            .unwrap();
        client.reset_usage();
        eprintln!("开始解释对照 {index}：{}", case["label"]);
        let started = Instant::now();
        let result = client.respond(&input, RequestMode::Analysis);
        let elapsed_ms = started.elapsed().as_millis();
        let record = match result {
            Ok(response) => json!({"response":response}),
            Err(error) => json!({"error":error.to_string()}),
        };
        let record = json!({"label":case["label"],"elapsed_ms":elapsed_ms,
            "usage":client.usage(),"result":record});
        std::fs::write(
            directory.join(format!("result-{index}.json")),
            serde_json::to_vec_pretty(&record).unwrap(),
        )
        .unwrap();
        eprintln!("解释对照 {index} 已结束：{elapsed_ms} ms");
    }
}
