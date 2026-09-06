use std::time::Duration;

use url::Url;

use crate::{UiError, replay::MAX_LOG_BYTES};

pub(crate) fn download(link: &str) -> Result<String, UiError> {
    let id = log_id(link)?;
    // 只使用校验后的编号构造官方地址，不访问用户提供的任意 URL。
    let url = format!("https://tenhou.net/5/mjlog2json.cgi?{id}");
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .max_redirects(0)
        .build()
        .new_agent();
    let mut response = agent
        .get(&url)
        .header("Referer", "https://tenhou.net/")
        .call()
        .map_err(download_error)?;
    response
        .body_mut()
        .with_config()
        .limit(MAX_LOG_BYTES as u64)
        .read_to_string()
        .map_err(download_error)
}

fn log_id(link: &str) -> Result<String, UiError> {
    let link = link.trim();
    if valid_id(link) {
        return Ok(link.to_owned());
    }
    let url = Url::parse(link)
        .map_err(|_| UiError::new("invalid_link", "请输入完整的天凤牌谱链接或 log ID"))?;
    if !matches!(url.scheme(), "http" | "https")
        || !matches!(url.host_str(), Some("tenhou.net" | "www.tenhou.net"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err(UiError::new(
            "invalid_link",
            "目前只支持 tenhou.net 的牌谱链接；雀魂链接暂不支持",
        ));
    }
    let mut ids = url.query_pairs().filter(|(key, _)| key == "log");
    let id = ids
        .next()
        .filter(|(_, value)| valid_id(value))
        .ok_or_else(|| UiError::new("invalid_link", "天凤链接缺少有效的 log 牌谱编号"))?;
    if ids.next().is_some() {
        return Err(UiError::new(
            "invalid_link",
            "链接中只能有一个 log 牌谱编号",
        ));
    }
    Ok(id.1.into_owned())
}

fn valid_id(value: &str) -> bool {
    let parts: Vec<_> = value.split('-').collect();
    value.is_ascii()
        && parts.len() == 4
        && parts[0].len() == 12
        && parts[0][..10].bytes().all(|byte| byte.is_ascii_digit())
        && &parts[0][10..] == "gm"
        && parts[1].len() == 4
        && parts[2].len() == 4
        && parts[3].len() == 8
        && parts[1..]
            .iter()
            .all(|part| part.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn download_error(error: ureq::Error) -> UiError {
    match error {
        ureq::Error::Timeout(_) => UiError::new("download_timeout", "下载天凤牌谱超时，请重试"),
        ureq::Error::BodyExceedsLimit(_) => {
            UiError::new("log_too_large", "牌谱文件不能超过 16 MiB")
        }
        ureq::Error::StatusCode(status) => UiError::new(
            "download_http",
            format!("天凤牌谱下载失败（HTTP {status}），请检查牌谱是否存在且可访问"),
        ),
        _ => UiError::new("download", "无法下载天凤牌谱，请检查网络或稍后重试"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "2023010100gm-00a9-0000-123456ab";

    #[test]
    fn accepts_links_and_ids_with_optional_perspective() {
        for link in [
            ID.to_owned(),
            format!("  https://tenhou.net/0/?log={ID}&tw=2  "),
            format!("http://www.tenhou.net/6/?tw=0&log={ID}#viewer"),
            format!("https://TENHOU.NET/0/?log={}", ID.replace('-', "%2D")),
        ] {
            assert_eq!(log_id(&link).unwrap(), ID);
        }
    }

    #[test]
    fn rejects_invalid_links_before_network_access() {
        for link in [
            String::new(),
            "fixtures/tenhou/ranked_game.json".to_owned(),
            format!("https://tenhou.net.evil.test/0/?log={ID}"),
            format!("https://tenhou.net@evil.test/0/?log={ID}"),
            format!("https://user:password@tenhou.net/0/?log={ID}"),
            format!("https://tenhou.net:8000/0/?log={ID}"),
            format!("ftp://tenhou.net/0/?log={ID}"),
            format!("https://game.maj-soul.com/1/?paipu={ID}"),
            format!("https://tenhou.net/0/#log={ID}"),
            format!("https://tenhou.net/0/?log={ID}&log={ID}"),
            "https://tenhou.net/0/?log=not-a-log".to_owned(),
            "https://tenhou.net/0/?log=雀魂牌谱".to_owned(),
        ] {
            assert_eq!(
                download(&link).err().unwrap().code,
                "invalid_link",
                "{link}"
            );
        }
    }

    #[test]
    fn reports_http_and_size_failures() {
        let error = download_error(ureq::Error::StatusCode(404));
        assert_eq!(error.code, "download_http");
        assert!(error.message.contains("404"));
        assert_eq!(
            download_error(ureq::Error::BodyExceedsLimit(MAX_LOG_BYTES as u64)).code,
            "log_too_large"
        );
    }
}
