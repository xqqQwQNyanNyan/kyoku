//! 仅压缩发给模型的数据表示；本地证据、工具结果和会话校验仍使用原始结构。

use serde_json::{Map, Value, json};

pub(super) fn input(items: &[Value]) -> Vec<Value> {
    items
        .iter()
        .map(|item| {
            let mut item = item.clone();
            if let Some(evidence) = super::super::context_evidence(&item) {
                item = super::super::initial_evidence(&value(evidence));
            } else if item["type"] == "function_call_output"
                && let Some(output) = item["output"].as_str()
                && let Ok(result) = serde_json::from_str(output)
            {
                item["output"] = Value::String(value(result).to_string());
            }
            item
        })
        .collect()
}

fn value(value: Value) -> Value {
    if let Some(table) = table(&value)
        && table.to_string().len() < value.to_string().len()
    {
        return table;
    }
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(self::value).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key, self::value(value)))
                .collect(),
        ),
        scalar => scalar,
    }
}

fn table(value: &Value) -> Option<Value> {
    let (rows, keys): (Vec<_>, Option<Vec<_>>) = match value {
        Value::Array(rows) => (rows.iter().collect(), None),
        Value::Object(rows) => (
            rows.values().collect(),
            Some(rows.keys().cloned().collect()),
        ),
        _ => return None,
    };
    if rows.len() < 2 {
        return None;
    }
    let first = rows[0].as_object().filter(|row| !row.is_empty())?;
    if !rows.iter().all(|row| {
        row.as_object()
            .is_some_and(|row| row.keys().eq(first.keys()))
    }) {
        return None;
    }
    // 字段完全一致才制表；缺失、null、false和空数组不能互相替代。
    let mut common = Map::new();
    let mut columns = Vec::new();
    for (key, field) in first {
        if rows.iter().all(|row| row.get(key) == Some(field)) {
            common.insert(key.clone(), self::value(field.clone()));
        } else {
            columns.push(key.clone());
        }
    }
    let values: Vec<Vec<_>> = rows
        .iter()
        .map(|row| {
            columns
                .iter()
                .map(|column| self::value(row[column].clone()))
                .collect()
        })
        .collect();
    let mut table = json!({"columns":columns,"rows":values});
    if !common.is_empty() {
        table["common"] = Value::Object(common);
    }
    if let Some(keys) = keys {
        table["row_keys"] = json!(keys);
    }
    Some(table)
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    pub(in crate::agent) fn expand(value: Value) -> Value {
        match value {
            Value::Object(mut object)
                if object.get("columns").is_some_and(Value::is_array)
                    && object.get("rows").is_some_and(Value::is_array) =>
            {
                let columns: Vec<String> =
                    serde_json::from_value(object.remove("columns").unwrap()).unwrap();
                let rows: Vec<Vec<Value>> =
                    serde_json::from_value(object.remove("rows").unwrap()).unwrap();
                let common: Map<String, Value> = object
                    .remove("common")
                    .map(|value| serde_json::from_value(value).unwrap())
                    .unwrap_or_default();
                let values: Vec<_> = rows
                    .into_iter()
                    .map(|row| {
                        assert_eq!(columns.len(), row.len());
                        let mut fields = common.clone();
                        fields.extend(columns.iter().cloned().zip(row));
                        Value::Object(fields.into_iter().map(|(k, v)| (k, expand(v))).collect())
                    })
                    .collect();
                if let Some(keys) = object.remove("row_keys") {
                    let keys: Vec<String> = serde_json::from_value(keys).unwrap();
                    assert_eq!(keys.len(), values.len());
                    Value::Object(keys.into_iter().zip(values).collect())
                } else {
                    Value::Array(values)
                }
            }
            Value::Object(fields) => {
                Value::Object(fields.into_iter().map(|(k, v)| (k, expand(v))).collect())
            }
            Value::Array(items) => Value::Array(items.into_iter().map(expand).collect()),
            scalar => scalar,
        }
    }

    #[test]
    fn tables_preserve_every_row_key_value_and_unknown_state() {
        let original = json!({
            "by_player": {
                "1":{"known_safe_against_ron":true,"unseen_copies":0,"passed_after_riichi":null,"tiles":["P","P"]},
                "2":{"known_safe_against_ron":false,"unseen_copies":1,"passed_after_riichi":null,"tiles":[]},
                "3":{"known_safe_against_ron":false,"unseen_copies":2,"passed_after_riichi":null,"tiles":["5mr"]}
            },
            "ordered": [
                {"long_shared_field":"same","optional":null,"value":1},
                {"long_shared_field":"same","optional":null,"value":1},
                {"long_shared_field":"same","optional":false,"value":2}
            ],
            "different_shapes": [{"present":null},{}],
            "not_records": [null,false,0,[]]
        });
        let compacted = value(original.clone());
        assert!(compacted.to_string().len() < original.to_string().len());
        assert_eq!(expand(compacted), original);
    }

    #[test]
    fn only_local_evidence_and_tool_json_are_compacted() {
        let rows = json!([
            {"unseen_is_not_wall_count":true,"discard":"1m"},
            {"unseen_is_not_wall_count":true,"discard":"2m"},
            {"unseen_is_not_wall_count":true,"discard":"3m"}
        ]);
        let original = vec![
            super::super::super::initial_evidence(&json!({"records":rows})),
            json!({"role":"user","content":rows.to_string()}),
            json!({"type":"function_call_output","call_id":"same-id","output":json!({"ok":true,"records":rows}).to_string()}),
            json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":"原始回答"}]}),
        ];
        let packed = input(&original);
        assert_eq!(packed[1], original[1]);
        assert_eq!(packed[3], original[3]);
        assert_eq!(packed[2]["call_id"], "same-id");
        let evidence = super::super::super::context_evidence(&packed[0]).unwrap();
        assert_eq!(expand(evidence), json!({"records":rows}));
        let tool = serde_json::from_str(packed[2]["output"].as_str().unwrap()).unwrap();
        assert_eq!(expand(tool), json!({"ok":true,"records":rows}));
    }

    #[test]
    #[ignore = "只读本地既有实战记录，比较请求表示大小；不调用模型"]
    fn measure_saved_tool_results() {
        let files: Vec<String> = serde_json::from_str(
            &std::env::var("KYOKU_COMPACT_INPUTS").expect("指定既有JSON记录路径数组"),
        )
        .unwrap();
        for file in files {
            let saved: Value =
                serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
            let outputs: Vec<&Value> = if let Some(trace) = saved["trace"].as_array() {
                trace
                    .iter()
                    .filter(|step| step["kind"] == "tool")
                    .map(|step| &step["result"])
                    .collect()
            } else {
                saved
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|step| &step["result"])
                    .collect()
            };
            for result in outputs {
                let started = std::time::Instant::now();
                let packed = value(result.clone());
                let micros = started.elapsed().as_micros();
                assert_eq!(expand(packed.clone()), *result);
                println!(
                    "{}",
                    json!({"source":file,"original_bytes":result.to_string().len(),"compact_bytes":packed.to_string().len(),"elapsed_us":micros})
                );
            }
        }
    }
}
