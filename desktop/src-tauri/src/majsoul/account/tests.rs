use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

const ID: &str = "200515-cfbe0120-c92c-44ad-bdfc-ebfef3a33a10_a89702544";
const SAMPLE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../services/majsoul/test/fixtures/ranked-round.tenhou.json"
));

fn credentials() -> Credentials {
    Credentials {
        username: "test-user".into(),
        password: "pipe-only-secret".into(),
        accept_risk: true,
    }
}

struct Fixture {
    paths: Paths,
    directory: PathBuf,
}
impl Fixture {
    fn new(program: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "kyoku-majsoul-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir(&directory).unwrap();
        let script = directory.join("worker.cjs");
        std::fs::write(&script, program).unwrap();
        Self {
            paths: Paths {
                node: "node".into(),
                script,
            },
            directory,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn credentials_require_consent_before_starting_any_process() {
    let paths = Paths {
        node: "/missing".into(),
        script: "/missing".into(),
    };
    let mut account = Account::default();
    let mut input = credentials();
    input.accept_risk = false;
    assert_eq!(
        account.login(&paths, input).unwrap_err().code,
        "majsoul_risk"
    );
    let mut input = credentials();
    input.password.clear();
    assert_eq!(
        account.login(&paths, input).unwrap_err().code,
        "majsoul_credentials"
    );
    assert!(account.next_login.is_none());
    assert!(!account.logged_in());
}

#[test]
fn release_uses_bundled_node_and_development_uses_local_dependencies() {
    let resources = Path::new("/Applications/Kyoku.app/Contents/Resources");
    let paths = Paths::from_roots(resources, None);
    assert_eq!(paths.node, resources.join("majsoul/node/bin/node"));
    assert_eq!(paths.script, resources.join("majsoul/service/desktop.cjs"));
    let paths = Paths::from_roots(Path::new("/missing"), Some(Path::new("/work")));
    assert_eq!(paths.node, Path::new("node"));
    assert_eq!(
        paths.script,
        Path::new("/work/services/majsoul/desktop.cjs")
    );
}

#[test]
fn private_pipe_login_download_replay_logout_and_login_cooldown() {
    let program = format!(
        r#"
const readline = require('node:readline');
const log = {SAMPLE};
let authenticated = false;
readline.createInterface({{input:process.stdin}}).on('line', line => {{
  const input = JSON.parse(line);
  if (input.action === 'login') {{
    if (process.argv.some(arg => arg.includes('pipe-only-secret')) ||
        Object.values(process.env).some(value => value.includes('pipe-only-secret')) ||
        process.env.NODE_OPTIONS || process.env.MJS_PASSWORD ||
        input.username !== 'test-user' || input.password !== 'pipe-only-secret' || !input.accept_risk) process.exit(1);
    authenticated = true;
    console.log(JSON.stringify({{logged_in:true}}));
  }} else if (authenticated && input.id === '{ID}') {{
    console.log(JSON.stringify({{log}}));
  }} else process.exit(1);
}});
"#
    );
    let fixture = Fixture::new(&program);
    let mut account = Account::default();
    let url = Url::parse(&format!("https://game.maj-soul.com/1/?paipu={ID}")).unwrap();
    assert_eq!(
        account.download(&url).unwrap_err().code,
        "majsoul_login_required"
    );
    account.login(&fixture.paths, credentials()).unwrap();
    assert!(account.logged_in());
    let pid = account.session.as_ref().unwrap().child.id();
    for _ in 0..2 {
        let json = account.download(&url).unwrap();
        let (_, replay) = crate::replay::parse(&json).unwrap();
        let frame = serde_json::to_value(replay.frames.last().unwrap()).unwrap();
        let scores: Vec<_> = frame["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|player| player["score"].as_i64().unwrap())
            .collect();
        assert_eq!(scores, [13000, 25000, 37000, 25000]);
        assert_eq!(account.session.as_ref().unwrap().child.id(), pid);
    }
    let account = std::sync::Mutex::new(account);
    let json = crate::log_link::download(&format!("雀魂牌谱:{url}"), &account).unwrap();
    assert!(crate::replay::parse(&json).is_ok());
    let mut account = account.into_inner().unwrap();
    account.logout();
    assert!(!account.logged_in());
    assert_eq!(
        account.download(&url).unwrap_err().code,
        "majsoul_login_required"
    );
    assert_eq!(
        account
            .login(&fixture.paths, credentials())
            .unwrap_err()
            .code,
        "majsoul_login_limited"
    );
}

#[test]
fn login_errors_are_sanitized_and_do_not_keep_a_session() {
    let fixture = Fixture::new(
        r#"
require('node:readline').createInterface({input:process.stdin}).on('line', () => {
  console.log(JSON.stringify({error:'pipe-only-secret: upstream token'}));
});
"#,
    );
    let mut account = Account::default();
    let error = account.login(&fixture.paths, credentials()).unwrap_err();
    assert!(!error.message.contains("secret"));
    assert!(!error.message.contains("token"));
    assert!(!account.logged_in());
    assert_eq!(
        account
            .login(&fixture.paths, credentials())
            .unwrap_err()
            .code,
        "majsoul_login_limited"
    );
}

#[test]
fn broken_or_excessive_output_ends_the_account_session() {
    for reply in [
        "process.stdout.write('invalid-secret-json\\n')",
        "process.stdout.write('x'.repeat(16 * 1024 * 1024 + 1025))",
        "process.exit(1)",
    ] {
        let fixture = Fixture::new(&format!(
            r#"
require('node:readline').createInterface({{input:process.stdin}}).on('line', line => {{
  if (JSON.parse(line).action === 'login') console.log('{{"logged_in":true}}');
  else {{ {reply}; }}
}});
"#
        ));
        let mut account = Account::default();
        account.login(&fixture.paths, credentials()).unwrap();
        let url = Url::parse(&format!("https://game.maj-soul.com/1/?paipu={ID}")).unwrap();
        let error = account.download(&url).unwrap_err();
        assert!(!error.message.contains("secret"));
        assert!(!account.logged_in());
    }
}

#[test]
fn stalled_process_has_a_bounded_response_wait() {
    let mut command = Command::new("node");
    command.args(["-e", "setTimeout(() => {}, 60000)"]);
    let session = Session::spawn(command).unwrap();
    let start = Instant::now();
    assert_eq!(
        session
            .read_response(Duration::from_millis(20))
            .err()
            .unwrap()
            .code,
        "majsoul_timeout"
    );
    drop(session);
    assert!(start.elapsed() < Duration::from_secs(5));
}
