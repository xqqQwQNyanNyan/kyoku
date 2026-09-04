use std::collections::HashSet;
use std::env;
use std::error::Error;
use std::fs;
use std::io::{self, Read};
use std::str::FromStr;

use convlog::tenhou::Log;
use convlog::{Event, tenhou_to_mjai};
use kyoku::replay::inspector::ReplayInspector;
use serde_json::Value;

const USAGE: &str = "Usage: cargo run --bin replay -- [OPTIONS] <tenhou-json|tenhou-url|log-id|->\n\nRead a tenhou.net/6 JSON log from a file/stdin, or download a Tenhou URL/log ID, then print replay deltas. Event indices are zero-based and ranges are inclusive.\n\nOptions:\n  -f, --full-state       print the final full RoundState snapshot\n      --state-at N       print the RoundState snapshot after global event N\n      --event N          show only global event N\n      --from N           show events starting at global event N\n      --to N             show events through global event N\n      --kyoku ROUND      show a round such as E1, S3, or E1.2 (honba)\n      --only TYPES       comma-separated event types (hora,kan,dora,ryukyoku,...)\n  -h, --help             show this help\n\nWith --only, StartKyoku/EndKyoku/EndGame summaries remain visible.\n";

fn main() {
    if let Err(error) = run() {
        eprintln!("replay: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let Some(args) = Args::parse(env::args().skip(1))? else {
        print!("{USAGE}");
        return Ok(());
    };

    let json = read_input(&args.input)?;
    let ryukyoku_reasons = extract_ryukyoku_reasons(&json);
    let log = Log::from_json_str(&json)?;
    let events = tenhou_to_mjai(&log)?;
    if let Some(index) = args.state_at
        && index >= events.len()
    {
        return Err(format!("--state-at {index} is outside the event log").into());
    }
    let mut inspector = ReplayInspector::new();
    let mut round = None;
    let mut kyoku_ordinal = None;
    let mut state_at_snapshot = None;

    for (global_index, event) in events.iter().enumerate() {
        if let Event::StartKyoku {
            bakaze,
            kyoku,
            honba,
            ..
        } = event
        {
            round = Some(RoundKey {
                wind: wind_letter(bakaze.as_u8()),
                number: *kyoku,
                honba: *honba,
            });
            kyoku_ordinal = Some(kyoku_ordinal.map_or(0, |index| index + 1));
        }

        let reason = if matches!(event, Event::Ryukyoku { .. }) {
            kyoku_ordinal
                .and_then(|index| ryukyoku_reasons.get(index))
                .and_then(Option::as_deref)
        } else {
            None
        };
        let visible = args.filter.matches(global_index, event, round);
        inspector
            .apply_with_context(event, visible, reason)
            .map_err(|error| *error)?;
        if args.state_at == Some(global_index) {
            state_at_snapshot = Some(inspector.full_state());
        }
    }

    print!("{}", inspector.output());
    if let Some(index) = args.state_at {
        let snapshot = state_at_snapshot.expect("validated event index must produce a snapshot");
        println!("\n========== STATE AFTER G{index:03} ==========");
        print!("{snapshot}");
    }
    if args.full_state {
        println!("\n========== FINAL STATE ==========");
        print!("{}", inspector.full_state());
    }
    Ok(())
}

#[derive(Debug)]
struct Args {
    full_state: bool,
    state_at: Option<usize>,
    input: String,
    filter: ReplayFilter,
}

impl Args {
    fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Option<Self>, Box<dyn Error>> {
        let mut full_state = false;
        let mut state_at = None;
        let mut input = None;
        let mut event = None;
        let mut from = None;
        let mut to = None;
        let mut kyoku = None;
        let mut only = None;
        let mut arguments = arguments.into_iter();

        while let Some(argument) = arguments.next() {
            let (option, inline_value) = argument
                .split_once('=')
                .map_or((argument.as_str(), None), |(name, value)| {
                    (name, Some(value))
                });
            match option {
                "-f" | "--full-state" if inline_value.is_none() => full_state = true,
                "-h" | "--help" if inline_value.is_none() => return Ok(None),
                "--state-at" => {
                    state_at = Some(parse_usize_option(
                        "--state-at",
                        &option_value(inline_value, &mut arguments, "--state-at")?,
                    )?);
                }
                "--event" => {
                    event = Some(parse_usize_option(
                        "--event",
                        &option_value(inline_value, &mut arguments, "--event")?,
                    )?);
                }
                "--from" => {
                    from = Some(parse_usize_option(
                        "--from",
                        &option_value(inline_value, &mut arguments, "--from")?,
                    )?);
                }
                "--to" => {
                    to = Some(parse_usize_option(
                        "--to",
                        &option_value(inline_value, &mut arguments, "--to")?,
                    )?);
                }
                "--kyoku" => {
                    kyoku = Some(RoundSelector::from_str(&option_value(
                        inline_value,
                        &mut arguments,
                        "--kyoku",
                    )?)?);
                }
                "--only" => {
                    only = Some(parse_event_types(&option_value(
                        inline_value,
                        &mut arguments,
                        "--only",
                    )?)?);
                }
                value if value.starts_with('-') => {
                    return Err(format!("unknown option {value}\n\n{USAGE}").into());
                }
                value if input.is_none() && inline_value.is_none() => {
                    input = Some(value.to_owned())
                }
                _ => return Err(format!("expected one input\n\n{USAGE}").into()),
            }
        }

        if event.is_some() && (from.is_some() || to.is_some()) {
            return Err("--event cannot be combined with --from or --to".into());
        }
        if let (Some(from), Some(to)) = (from, to)
            && from > to
        {
            return Err(format!("--from {from} is greater than --to {to}").into());
        }
        let input = input.ok_or_else(|| format!("missing input\n\n{USAGE}"))?;
        Ok(Some(Self {
            full_state,
            state_at,
            input,
            filter: ReplayFilter {
                event,
                from,
                to,
                kyoku,
                only,
            },
        }))
    }
}

fn option_value(
    inline: Option<&str>,
    arguments: &mut impl Iterator<Item = String>,
    option: &str,
) -> Result<String, Box<dyn Error>> {
    if let Some(value) = inline {
        if value.is_empty() {
            return Err(format!("missing value for {option}").into());
        }
        return Ok(value.to_owned());
    }
    arguments
        .next()
        .ok_or_else(|| format!("missing value for {option}").into())
}

fn parse_usize_option(option: &str, value: &str) -> Result<usize, Box<dyn Error>> {
    value.parse().map_err(|_| {
        format!("invalid {option} value {value:?}: expected a non-negative integer").into()
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RoundKey {
    wind: Option<char>,
    number: u8,
    honba: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RoundSelector {
    wind: char,
    number: u8,
    honba: Option<u8>,
}

impl FromStr for RoundSelector {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut chars = value.chars();
        let wind = chars
            .next()
            .map(|wind| wind.to_ascii_uppercase())
            .ok_or_else(|| "empty --kyoku value".to_owned())?;
        if !matches!(wind, 'E' | 'S' | 'W' | 'N') {
            return Err(format!(
                "invalid --kyoku value {value:?}: wind must be E, S, W, or N"
            ));
        }
        let rest = chars.as_str();
        let (number, honba) = rest
            .split_once('.')
            .map_or((rest, None), |(number, honba)| (number, Some(honba)));
        let number: u8 = number
            .parse()
            .map_err(|_| format!("invalid --kyoku value {value:?}"))?;
        if !(1..=4).contains(&number) {
            return Err(format!(
                "invalid --kyoku value {value:?}: round number must be 1 through 4"
            ));
        }
        let honba = honba
            .map(|honba| {
                honba
                    .parse()
                    .map_err(|_| format!("invalid --kyoku honba in {value:?}"))
            })
            .transpose()?;
        Ok(Self {
            wind,
            number,
            honba,
        })
    }
}

impl RoundSelector {
    fn matches(self, round: RoundKey) -> bool {
        round.wind == Some(self.wind)
            && round.number == self.number
            && self.honba.is_none_or(|honba| honba == round.honba)
    }
}

#[derive(Debug, Default)]
struct ReplayFilter {
    event: Option<usize>,
    from: Option<usize>,
    to: Option<usize>,
    kyoku: Option<RoundSelector>,
    only: Option<HashSet<String>>,
}

impl ReplayFilter {
    fn matches(&self, index: usize, event: &Event, round: Option<RoundKey>) -> bool {
        if self.event.is_some_and(|selected| index != selected)
            || self.from.is_some_and(|from| index < from)
            || self.to.is_some_and(|to| index > to)
        {
            return false;
        }
        if let Some(selector) = self.kyoku {
            if matches!(event, Event::StartGame { .. } | Event::EndGame) {
                return false;
            }
            if !round.is_some_and(|round| selector.matches(round)) {
                return false;
            }
        }
        let Some(only) = &self.only else {
            return true;
        };
        is_structural(event)
            || event_type_names(event)
                .iter()
                .any(|name| only.contains(*name))
    }
}

fn parse_event_types(value: &str) -> Result<HashSet<String>, Box<dyn Error>> {
    let valid = [
        "none",
        "start_game",
        "start_kyoku",
        "tsumo",
        "dahai",
        "chi",
        "pon",
        "kan",
        "daiminkan",
        "ankan",
        "kakan",
        "dora",
        "reach",
        "reach_accepted",
        "hora",
        "ryukyoku",
        "end_kyoku",
        "end_game",
    ];
    let mut selected = HashSet::new();
    for name in value.split(',') {
        let name = name.trim().to_ascii_lowercase();
        if name.is_empty() || !valid.contains(&name.as_str()) {
            return Err(format!(
                "unknown event type {name:?}; expected one of {}",
                valid.join(",")
            )
            .into());
        }
        selected.insert(name);
    }
    Ok(selected)
}

fn event_type_names(event: &Event) -> &'static [&'static str] {
    match event {
        Event::None => &["none"],
        Event::StartGame { .. } => &["start_game"],
        Event::StartKyoku { .. } => &["start_kyoku"],
        Event::Tsumo { .. } => &["tsumo"],
        Event::Dahai { .. } => &["dahai"],
        Event::Chi { .. } => &["chi"],
        Event::Pon { .. } => &["pon"],
        Event::Daiminkan { .. } => &["daiminkan", "kan"],
        Event::Ankan { .. } => &["ankan", "kan"],
        Event::Kakan { .. } => &["kakan", "kan"],
        Event::Dora { .. } => &["dora"],
        Event::Reach { .. } => &["reach"],
        Event::ReachAccepted { .. } => &["reach_accepted"],
        Event::Hora { .. } => &["hora"],
        Event::Ryukyoku { .. } => &["ryukyoku"],
        Event::EndKyoku => &["end_kyoku"],
        Event::EndGame => &["end_game"],
    }
}

fn is_structural(event: &Event) -> bool {
    matches!(
        event,
        Event::StartKyoku { .. } | Event::EndKyoku | Event::EndGame
    )
}

fn read_input(input: &str) -> Result<String, Box<dyn Error>> {
    match classify_input(input)? {
        InputSource::Stdin => {
            let mut input = String::new();
            io::stdin().read_to_string(&mut input)?;
            Ok(input)
        }
        InputSource::File(path) => Ok(fs::read_to_string(path)?),
        InputSource::Tenhou(log_id) => download_tenhou_log(&log_id),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum InputSource<'a> {
    Stdin,
    File(&'a str),
    Tenhou(String),
}

fn classify_input(input: &str) -> Result<InputSource<'_>, Box<dyn Error>> {
    if input == "-" {
        return Ok(InputSource::Stdin);
    }
    if input.starts_with("http://") || input.starts_with("https://") {
        return Ok(InputSource::Tenhou(log_id_from_url(input)?));
    }
    if is_tenhou_log_id(input) {
        return Ok(InputSource::Tenhou(input.to_owned()));
    }
    Ok(InputSource::File(input))
}

fn log_id_from_url(url: &str) -> Result<String, Box<dyn Error>> {
    let after_scheme = url
        .split_once("://")
        .map(|(_, rest)| rest)
        .ok_or_else(|| format!("invalid Tenhou URL {url:?}"))?;
    let host = after_scheme.split(['/', '?']).next().unwrap_or_default();
    if !matches!(
        host.to_ascii_lowercase().as_str(),
        "tenhou.net" | "www.tenhou.net"
    ) {
        return Err(format!("not a tenhou.net URL: {url}").into());
    }
    let query = url
        .split_once('?')
        .map(|(_, query)| query)
        .ok_or_else(|| format!("Tenhou URL has no log query parameter: {url}"))?;
    let log_id = query
        .split(['&', '#'])
        .find_map(|field| field.strip_prefix("log="))
        .ok_or_else(|| format!("Tenhou URL has no log query parameter: {url}"))?;
    if !is_tenhou_log_id(log_id) {
        return Err(format!("invalid Tenhou log ID in URL: {log_id:?}").into());
    }
    Ok(log_id.to_owned())
}

fn is_tenhou_log_id(value: &str) -> bool {
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

fn download_tenhou_log(log_id: &str) -> Result<String, Box<dyn Error>> {
    let url = format!("https://tenhou.net/5/mjlog2json.cgi?{log_id}");
    let response = ureq::get(&url)
        .header("Referer", "https://tenhou.net/")
        .call()?;
    Ok(response.into_body().read_to_string()?)
}

fn extract_ryukyoku_reasons(json: &str) -> Vec<Option<String>> {
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    value
        .get("log")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|kyoku| {
            let status = kyoku.as_array()?.last()?.as_array()?.first()?.as_str()?;
            (status != "和了").then(|| status.to_owned())
        })
        .collect()
}

fn wind_letter(tile: u8) -> Option<char> {
    match tile {
        27 => Some('E'),
        28 => Some('S'),
        29 => Some('W'),
        30 => Some('N'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn parses_event_range_kyoku_and_type_filters() {
        let args = Args::parse(strings(&[
            "--from",
            "10",
            "--to=20",
            "--state-at",
            "15",
            "--kyoku",
            "e2.1",
            "--only",
            "hora,kan,dora,ryukyoku",
            "game.json",
        ]))
        .unwrap()
        .unwrap();

        assert_eq!(args.filter.from, Some(10));
        assert_eq!(args.filter.to, Some(20));
        assert_eq!(args.state_at, Some(15));
        assert_eq!(
            args.filter.kyoku,
            Some(RoundSelector {
                wind: 'E',
                number: 2,
                honba: Some(1),
            })
        );
        assert!(args.filter.only.unwrap().contains("kan"));
    }

    #[test]
    fn rejects_conflicting_and_invalid_filters() {
        assert!(Args::parse(strings(&["--event", "1", "--from", "1", "game.json"])).is_err());
        assert!(Args::parse(strings(&["--from", "3", "--to", "2", "game.json"])).is_err());
        assert!(Args::parse(strings(&["--only", "hora,nope", "game.json"])).is_err());
    }

    #[test]
    fn only_filter_groups_kans_and_keeps_structural_summaries() {
        let only = parse_event_types("hora,kan").unwrap();
        let filter = ReplayFilter {
            only: Some(only),
            ..ReplayFilter::default()
        };
        let tile = convlog::Tile::try_from(0_u8).unwrap();

        assert!(filter.matches(
            1,
            &Event::Ankan {
                actor: 0,
                consumed: [tile; 4],
            },
            None,
        ));
        assert!(!filter.matches(
            2,
            &Event::Tsumo {
                actor: 0,
                pai: tile
            },
            None
        ));
        assert!(filter.matches(3, &Event::EndKyoku, None));
    }

    #[test]
    fn event_range_and_kyoku_filters_are_combined() {
        let filter = ReplayFilter {
            from: Some(10),
            to: Some(20),
            kyoku: Some("E2".parse().unwrap()),
            ..ReplayFilter::default()
        };
        let event = Event::Dora {
            dora_marker: convlog::Tile::try_from(0_u8).unwrap(),
        };
        let east_two = Some(RoundKey {
            wind: Some('E'),
            number: 2,
            honba: 3,
        });

        assert!(filter.matches(10, &event, east_two));
        assert!(filter.matches(20, &event, east_two));
        assert!(!filter.matches(9, &event, east_two));
        assert!(!filter.matches(
            15,
            &event,
            Some(RoundKey {
                wind: Some('S'),
                number: 2,
                honba: 3,
            }),
        ));
    }

    #[test]
    fn recognizes_tenhou_urls_ids_and_local_files() {
        let id = "2019050417gm-0029-0000-4f2a8622";
        assert_eq!(
            classify_input(id).unwrap(),
            InputSource::Tenhou(id.to_owned())
        );
        assert_eq!(
            classify_input(&format!("https://tenhou.net/0/?log={id}&tw=2")).unwrap(),
            InputSource::Tenhou(id.to_owned())
        );
        assert_eq!(
            classify_input("game.json").unwrap(),
            InputSource::File("game.json")
        );
    }

    #[test]
    fn extracts_original_tenhou_ryukyoku_types() {
        let json = r#"{"log":[[[0,0,0],[],[],[],[],[],[],[],[],[],[],[],[],[],[],["四家立直",[0,0,0,0]]],[[1,0,0],[],[],[],[],[],[],[],[],[],[],[],[],[],[],["和了"]]]}"#;
        assert_eq!(
            extract_ryukyoku_reasons(json),
            vec![Some("四家立直".to_owned()), None]
        );
    }
}
