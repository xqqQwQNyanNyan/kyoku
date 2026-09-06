use std::{
    env,
    error::Error,
    fs,
    io::{self, BufRead, IsTerminal, Read, Write},
    path::PathBuf,
};

use convlog::{tenhou::Log, tenhou_to_mjai};
use kyoku::{
    agent::{AgentConfig, AgentContext, AgentError, AgentSession, review_evidence},
    mahjong::player_index::PlayerIndex,
    mortal::MortalConfig,
    review::{review_at, review_game},
};

#[path = "agent/browse.rs"]
mod browse;
#[path = "common/decision_output.rs"]
mod decision_output;
#[path = "agent/env_file.rs"]
mod env_file;
#[path = "common/mortal_output.rs"]
mod mortal_output;
#[path = "common/review_output.rs"]
mod review_output;

const USAGE: &str =
    "Usage: cargo run --bin agent -- --player <0..3> (--event N | --browse) [OPTIONS] <tenhou-json|->

Ask questions about one fixed position after zero-based event N.
Without --question, start an interactive conversation. Commands: /evidence, /quit.

Browse a game with --browse: /list, /select N, /next, /prev, /show, /evidence, /quit.
N is the global event index from /list. Switching positions clears the conversation.
Browsing and /evidence need no LLM configuration; asking questions requires it.

Options:
  --without-mortal   use visible position only; analysis is marked unavailable
  --browse           cache every decision and interactively browse a file
  --question TEXT    answer one question and exit
  --interactive      continue asking after --question (requires a file input)
  --llm-model NAME   LLM model (otherwise OPENAI_MODEL; required for questions)
  --endpoint URL     full Responses or Chat Completions URL (otherwise KYOKU_OPENAI_ENDPOINT,
                     default: https://api.openai.com/v1/responses)
  --python PATH      Mortal Python (default: mortal/.venv/bin/python)
  --runtime PATH     Mortal checkout (default: mortal/runtime)
  --model PATH       Mortal weights (default: mortal/models/mortal_582500.pth)
  -h, --help         show help

The default endpoint uses OPENAI_API_KEY. Endpoint overrides use only AGENT_API_KEY.
Local services may omit AGENT_API_KEY. Never pass keys as command-line options.
Reads .env from the working directory; existing environment variables take precedence.
The configured LLM service receives questions and visible review evidence.
Input '-' reads Tenhou JSON from stdin and requires --question without --interactive.
";

fn main() {
    if let Err(error) = run() {
        eprintln!("agent: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let Some(args) = Args::parse(env::args().skip(1))? else {
        print!("{USAGE}");
        return Ok(());
    };
    let env_file = env_file::EnvFile::load(std::path::Path::new(".env"))?;
    let read_config = |name: &str| env_file.get(name, env::var(name).ok());
    let model = args
        .llm_model
        .clone()
        .or_else(|| read_config("OPENAI_MODEL"))
        .filter(|value| !value.trim().is_empty());
    let (endpoint, key) = connection_settings(args.endpoint.as_deref(), read_config);
    let json = if args.input == "-" {
        let mut json = String::new();
        io::stdin().read_to_string(&mut json)?;
        json
    } else {
        fs::read_to_string(&args.input)?
    };
    let events = tenhou_to_mjai(&Log::from_json_str(&json)?)?;
    let mortal_config = MortalConfig {
        python: &args.python,
        runtime: &args.runtime,
        checkpoint: &args.model,
    };
    let agent_config = model.as_deref().map(|model| AgentConfig {
        endpoint: &endpoint,
        model,
        api_key: key.as_deref(),
    });
    if args.browse {
        eprintln!(
            "正在推理 P{} 的整场牌谱，模型只加载一次…",
            args.player.get_id()
        );
        let game = review_game(&events, args.player, &mortal_config)?;
        return Ok(browse::conversation(
            game.decisions(),
            agent_config.as_ref(),
            io::stdin().lock(),
            io::stdout().lock(),
            io::stderr().lock(),
            io::stdin().is_terminal(),
        )?);
    }
    if args.question.is_some() && agent_config.is_none() {
        return Err("set --llm-model or OPENAI_MODEL".into());
    }
    let event = args.event.ok_or("missing --event")?;
    eprintln!(
        "正在重建 G{event:03}、P{} 的局面{}…",
        args.player.get_id(),
        if args.without_mortal {
            "（不运行 Mortal）"
        } else {
            "并运行 Mortal"
        }
    );
    let context = if args.without_mortal {
        AgentContext::from_events(&events, args.player, event)?
    } else {
        AgentContext::from(&review_at(&events, args.player, event, &mortal_config)?)
    };
    let mut session = None;
    if let Some(question) = &args.question {
        eprintln!("正在请求复盘解释…");
        println!(
            "{}",
            ask_question(&mut session, &context, agent_config.as_ref(), question)?
        );
    }
    if args.interactive {
        eprintln!(
            "局面已固定。输入问题继续复盘；/evidence 查看工具证据，/quit 退出。回答中的计算和模型判断可与证据核对。"
        );
        conversation(
            &context,
            agent_config.as_ref(),
            &mut session,
            io::stdin().lock(),
            io::stdout().lock(),
            io::stderr().lock(),
            io::stdin().is_terminal(),
        )?;
    }
    Ok(())
}

fn connection_settings(
    endpoint: Option<&str>,
    mut read_env: impl FnMut(&str) -> Option<String>,
) -> (String, Option<String>) {
    let custom = endpoint
        .map(str::to_owned)
        .or_else(|| read_env("KYOKU_OPENAI_ENDPOINT"));
    match custom {
        // 显式覆盖地址时不读取官方凭据，即使覆盖值恰好是官方地址。
        Some(endpoint) => (endpoint, read_env("AGENT_API_KEY")),
        None => (
            "https://api.openai.com/v1/responses".into(),
            read_env("OPENAI_API_KEY"),
        ),
    }
}

fn conversation(
    context: &AgentContext,
    config: Option<&AgentConfig<'_>>,
    session: &mut Option<AgentSession>,
    mut input: impl BufRead,
    mut output: impl Write,
    mut diagnostics: impl Write,
    show_prompt: bool,
) -> io::Result<()> {
    loop {
        if show_prompt {
            write!(output, "\n你> ")?;
            output.flush()?;
        }
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(());
        }
        match line.trim() {
            "" => continue,
            "/quit" => return Ok(()),
            "/evidence" => writeln!(output, "{:#}", context.evidence())?,
            question => {
                writeln!(diagnostics, "正在请求复盘解释…")?;
                match ask_question(session, context, config, question) {
                    Ok(answer) => writeln!(output, "{answer}")?,
                    Err(error) => {
                        writeln!(diagnostics, "agent: {error}；本轮未写入会话，可重试。")?
                    }
                }
            }
        }
    }
}

fn ask_question(
    session: &mut Option<AgentSession>,
    context: &AgentContext,
    config: Option<&AgentConfig<'_>>,
    question: &str,
) -> Result<String, AgentError> {
    if session.is_none() {
        let config = config.ok_or(AgentError::InvalidConfig {
            field: "model",
            reason: "set --llm-model or OPENAI_MODEL to ask questions",
        })?;
        *session = Some(AgentSession::with_context(context, config)?);
    }
    session.as_mut().expect("会话已成功创建").ask(question)
}

struct Args {
    player: PlayerIndex,
    event: Option<usize>,
    browse: bool,
    without_mortal: bool,
    python: PathBuf,
    runtime: PathBuf,
    model: PathBuf,
    llm_model: Option<String>,
    endpoint: Option<String>,
    question: Option<String>,
    interactive: bool,
    input: String,
}

impl Args {
    fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Option<Self>, Box<dyn Error>> {
        let mut player = None;
        let mut event = None;
        let mut browse = false;
        let mut without_mortal = false;
        let mut python = PathBuf::from("mortal/.venv/bin/python");
        let mut runtime = PathBuf::from("mortal/runtime");
        let mut model = PathBuf::from("mortal/models/mortal_582500.pth");
        let mut llm_model = None;
        let mut endpoint = None;
        let mut question = None;
        let mut interactive = false;
        let mut input = None;
        let mut arguments = arguments.into_iter();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "-h" | "--help" => return Ok(None),
                "--interactive" => interactive = true,
                "--browse" => browse = true,
                "--without-mortal" => without_mortal = true,
                "--player" | "--event" | "--python" | "--runtime" | "--model" | "--llm-model"
                | "--endpoint" | "--question" => {
                    let value = arguments
                        .next()
                        .ok_or_else(|| format!("missing value for {argument}"))?;
                    if value.trim().is_empty() {
                        return Err(format!("empty value for {argument}").into());
                    }
                    match argument.as_str() {
                        "--player" => player = Some(PlayerIndex::try_from(value.parse::<u8>()?)?),
                        "--event" => event = Some(value.parse::<usize>()?),
                        "--python" => python = value.into(),
                        "--runtime" => runtime = value.into(),
                        "--model" => model = value.into(),
                        "--llm-model" => llm_model = Some(value),
                        "--endpoint" => endpoint = Some(value),
                        "--question" => question = Some(value),
                        _ => unreachable!(),
                    }
                }
                value if value.starts_with('-') && value != "-" => {
                    return Err(format!("unknown option {value}").into());
                }
                _ if input.is_none() => input = Some(argument),
                _ => return Err("expected only one input".into()),
            }
        }
        let input = input.ok_or("missing Tenhou JSON input")?;
        if browse && without_mortal {
            return Err("--without-mortal requires --event; decision browsing needs Mortal".into());
        }
        if browse && (event.is_some() || question.is_some()) {
            return Err("--browse cannot be combined with --event or --question".into());
        }
        if !browse && event.is_none() {
            return Err("missing --event or --browse".into());
        }
        interactive |= question.is_none();
        if input == "-" && interactive {
            return Err("stdin log input requires --question without --interactive".into());
        }
        Ok(Some(Self {
            player: player.ok_or("missing --player (0..3)")?,
            event,
            browse,
            without_mortal,
            python,
            runtime,
            model,
            llm_model,
            endpoint,
            question,
            interactive,
            input,
        }))
    }
}

#[cfg(test)]
#[path = "agent/tests.rs"]
mod tests;
