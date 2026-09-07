use serde_json::Value;

use super::super::ProviderError;

pub(super) fn parse(body: &str, secret: Option<&str>) -> Option<ProviderError> {
    let value: Value = serde_json::from_str(body).ok()?;
    let error = value.get("error")?.as_object()?;
    let field = |name: &str, limit| {
        let text = error.get(name)?.as_str()?;
        let text = sanitize(text, secret, limit);
        (!text.is_empty()).then_some(text)
    };
    let error = ProviderError {
        code: field("code", 128).or_else(|| field("type", 128)),
        parameter: field("param", 128),
        message: field("message", 1024),
    };
    (error.code.is_some() || error.parameter.is_some() || error.message.is_some()).then_some(error)
}

fn sanitize(text: &str, secret: Option<&str>, limit: usize) -> String {
    // 必须先脱敏后截断，避免长密钥被截成无法匹配的前缀。
    let text = match secret.filter(|secret| !secret.is_empty()) {
        Some(secret) => text.replace(secret, "[REDACTED]"),
        None => text.to_owned(),
    };
    let mut words = Vec::new();
    let mut after_bearer = false;
    for word in text.split_whitespace() {
        if after_bearer {
            words.push("[REDACTED]".to_owned());
            after_bearer = false;
            continue;
        }
        after_bearer = word
            .trim_matches(|c: char| !c.is_ascii_alphabetic())
            .eq_ignore_ascii_case("bearer");
        words.push(
            word.chars()
                .filter(|c| {
                    !c.is_control()
                        && !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
                })
                .collect(),
        );
    }
    let text = words.join(" ");
    let mut short: String = text.chars().take(limit).collect();
    if text.chars().count() > limit {
        short.push('…');
    }
    short
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn structured_provider_error_is_preserved_without_credentials_or_extra_fields() {
        let body = json!({"error":{
            "code":"invalid_request_error",
            "param":"tool_choice",
            "message":"Thinking mode does not support this tool_choice; key=private-test-key Bearer another-key\ntry auto",
            "request":{"private":"do-not-copy"}
        }}).to_string();
        let error = parse(&body, Some("private-test-key")).unwrap();
        assert_eq!(error.code.as_deref(), Some("invalid_request_error"));
        assert_eq!(error.parameter.as_deref(), Some("tool_choice"));
        let text = format!("{error:?}");
        assert!(text.contains("Thinking mode does not support this tool_choice"));
        for secret in ["private-test-key", "another-key", "do-not-copy"] {
            assert!(!text.contains(secret));
        }
    }

    #[test]
    fn malformed_errors_are_ignored_and_all_fields_are_sanitized_before_truncation() {
        for body in [
            "<html>private</html>",
            "{}",
            r#"{"error":"private"}"#,
            r#"{"error":{"message":null}}"#,
        ] {
            assert!(parse(body, None).is_none());
        }
        let secret = "private".repeat(300);
        let body = json!({"error":{"code":secret,"param":secret,"message":format!("{secret}{}", "文".repeat(2000))}}).to_string();
        let error = parse(&body, Some(&secret)).unwrap();
        assert_eq!(error.code.as_deref(), Some("[REDACTED]"));
        assert_eq!(error.parameter.as_deref(), Some("[REDACTED]"));
        assert_eq!(error.message.as_ref().unwrap().chars().count(), 1025);
        assert!(!format!("{error:?}").contains("private"));
    }
}
