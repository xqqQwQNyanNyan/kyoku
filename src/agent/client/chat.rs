use serde_json::{Value, json};

use super::super::{AgentError, INSTRUCTIONS, RequestMode, invalid, required_string};
use super::available_tools;

pub(super) fn request(
    model: &str,
    input: &[Value],
    mode: RequestMode,
) -> Result<Value, AgentError> {
    let mut messages = vec![json!({"role": "system", "content": INSTRUCTIONS})];
    let mut items = input.iter();
    while let Some(item) = items.next() {
        if let Some(message) = item.get("_chat_message") {
            messages.push(message.clone());
            // 同一条 Chat 消息可能拆成多个工具调用和正文；发回时只还原一次。
            let count = item["_chat_output_count"]
                .as_u64()
                .ok_or(invalid("invalid chat history"))?;
            for _ in 1..count {
                items.next().ok_or(invalid("incomplete chat history"))?;
            }
        } else if item["type"] == "function_call_output" {
            messages.push(json!({
                "role": "tool", "tool_call_id": required_string(item, "call_id")?,
                "content": required_string(item, "output")?,
            }));
        } else if matches!(item["role"].as_str(), Some("user" | "developer")) {
            messages.push(json!({
                "role": if item["role"] == "developer" { "system" } else { "user" },
                "content": required_string(item, "content")?,
            }));
        } else {
            return Err(invalid("unsupported chat history item"));
        }
    }
    let mut tools = Vec::new();
    for mut function in available_tools(mode) {
        function
            .as_object_mut()
            .ok_or(invalid("invalid tool definition"))?
            .remove("type");
        tools.push(json!({"type": "function", "function": function}));
    }
    Ok(json!({
        "model": model, "messages": messages,
        "tools": tools,
        "tool_choice": "auto",
        "parallel_tool_calls": mode == RequestMode::Analysis, "store": false, "max_completion_tokens": 4096,
    }))
}

pub(super) fn response(response: Value) -> Result<Value, AgentError> {
    let choices = response["choices"]
        .as_array()
        .ok_or(invalid("missing chat choices"))?;
    if choices.len() != 1 {
        return Err(invalid("expected one chat choice"));
    }
    let choice = &choices[0];
    let has_calls = match choice["finish_reason"].as_str() {
        Some("stop") => false,
        Some("tool_calls") => true,
        Some("length") => return Err(AgentError::OutputLimit),
        Some("content_filter") => return Err(AgentError::Refused),
        _ => return Err(invalid("unsupported chat finish reason")),
    };
    let message = &choice["message"];
    if message["role"] != "assistant" {
        return Err(invalid("expected chat assistant message"));
    }
    if !message["refusal"].is_null() {
        match message["refusal"].as_str() {
            Some("") => {}
            Some(_) => return Err(AgentError::Refused),
            None => return Err(invalid("invalid chat refusal")),
        }
    }
    if !message["function_call"].is_null() {
        return Err(invalid("legacy function_call is not supported; use tools"));
    }
    let calls = match &message["tool_calls"] {
        Value::Null => &[][..],
        Value::Array(calls) => calls.as_slice(),
        _ => return Err(invalid("invalid chat tool calls")),
    };
    if has_calls == calls.is_empty() || calls.len() > 8 {
        return Err(invalid("chat finish reason and tool calls do not match"));
    }
    let mut output = Vec::new();
    for call in calls {
        if call["type"] != "function" {
            return Err(invalid("unsupported chat tool type"));
        }
        output.push(json!({
            "type": "function_call", "status": "completed",
            "call_id": required_string(call, "id")?,
            "name": required_string(&call["function"], "name")?,
            "arguments": call["function"]["arguments"].as_str().ok_or(invalid("missing chat function arguments"))?,
        }));
    }
    match &message["content"] {
        Value::Null => {}
        Value::String(text) if text.trim().is_empty() => {}
        Value::String(text) => output.push(json!({
            "type": "message", "role": "assistant", "status": "completed",
            "content": [{"type": "output_text", "text": text}],
        })),
        _ => return Err(invalid("unsupported chat message content")),
    }
    // 只续传消息输入字段；供应商若返回 reasoning_content，则与该条消息一起保留。
    let mut original = json!({"role": "assistant", "content": message["content"]});
    if !calls.is_empty() {
        original["tool_calls"] = json!(calls);
    }
    if let Some(reasoning) = message.get("reasoning_content") {
        if !reasoning.is_null() && !reasoning.is_string() {
            return Err(invalid("invalid chat reasoning content"));
        }
        original["reasoning_content"] = reasoning.clone();
    }
    let count = output.len();
    let first = output
        .first_mut()
        .ok_or(invalid("no chat answer or tool call"))?;
    first["_chat_message"] = original;
    first["_chat_output_count"] = json!(count);
    let mut normalized = json!({"status": "completed", "output": output});
    if let Some(usage) = response.get("usage") {
        normalized["usage"] = usage.clone();
    }
    Ok(normalized)
}
