mod account;
pub(crate) use account::{Account, Credentials, Paths};

use url::Url;

use crate::UiError;

pub(crate) fn is_host(host: &str) -> bool {
    matches!(
        host,
        "game.maj-soul.com"
            | "game.maj-soul.net"
            | "game.majsoul.com"
            | "game.mahjongsoul.com"
            | "mahjongsoul.game.yo-star.com"
            | "mahjongsoul.game.yo-star.net"
    )
}

fn paipu_id(url: &Url) -> Result<String, UiError> {
    let mut ids = url.query_pairs().filter(|(key, _)| key == "paipu");
    let id = ids
        .next()
        .filter(|(_, value)| valid_id(value))
        .ok_or_else(|| UiError::new("invalid_link", "雀魂链接缺少有效的 paipu 牌谱编号"))?;
    if ids.next().is_some() {
        return Err(UiError::new(
            "invalid_link",
            "链接中只能有一个 paipu 牌谱编号",
        ));
    }
    Ok(id.1.into_owned())
}

fn valid_id(id: &str) -> bool {
    if id.len() > 96 || !id.is_ascii() {
        return false;
    }
    let parts: Vec<_> = id.split('_').collect();
    if parts.len() > 3 || parts.len() == 3 && parts[2] != "2" {
        return false;
    }
    if let Some(player) = parts.get(1) {
        let player = player.strip_prefix('a').unwrap_or(player);
        if player.is_empty() || player.len() > 12 || !player.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
    }
    let anonymous = parts.len() == 3;
    let decoded: String = parts[0]
        .bytes()
        .enumerate()
        .map(|(i, b)| {
            if anonymous && b.is_ascii_alphanumeric() && !b.is_ascii_uppercase() {
                let value = if b.is_ascii_digit() {
                    b - b'0'
                } else {
                    b - b'a' + 10
                };
                let value = (usize::from(value) + 91 - i % 36) % 36;
                if value < 10 {
                    (b'0' + value as u8) as char
                } else {
                    (b'a' + value as u8 - 10) as char
                }
            } else {
                b as char
            }
        })
        .collect();
    let groups: Vec<_> = decoded.split('-').collect();
    let uuid = if groups.len() == 6 {
        if groups[0].len() != 6 || !groups[0].bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        &groups[1..]
    } else {
        &groups[..]
    };
    uuid.len() == 5
        && uuid.iter().zip([8, 4, 4, 4, 12]).all(|(part, len)| {
            part.len() == len
                && part
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "200515-cfbe0120-c92c-44ad-bdfc-ebfef3a33a10_a89702544";

    #[test]
    fn validates_normal_anonymous_and_legacy_ids() {
        for id in [
            ID,
            "jijpmr-0415suwv-971c-67ei-ilom-qottvksmnvnn_a89702544_2",
            "cfbe0120-c92c-44ad-bdfc-ebfef3a33a10",
            "200515-cfbe0120-c92c-44ad-bdfc-ebfef3a33a10_2",
        ] {
            assert!(valid_id(id), "{id}");
        }
        for id in [
            "",
            "hello",
            "../../secret",
            "200515-cfbe0120-c92c-44ad-bdfc-ebfef3a33a10_a",
            "200515-cfbe0120-c92c-44ad-bdfc-ebfef3a33a10_a1_3",
            "jijpmr-0415suwv-971c-67ei-ilom-qottvksmnvnn_a89702544",
            "雀魂",
        ] {
            assert!(!valid_id(id), "{id}");
        }
        let url = Url::parse(&format!(
            "https://game.maj-soul.com/1/?paipu={ID}&paipu={ID}"
        ))
        .unwrap();
        assert!(paipu_id(&url).is_err());
    }
}
