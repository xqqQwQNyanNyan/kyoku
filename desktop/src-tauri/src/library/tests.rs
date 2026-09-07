use super::*;
use std::sync::Arc;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "kyoku-library-{}-{}",
            std::process::id(),
            FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn game() -> SessionGame {
    let (events, _) =
        replay::parse(include_str!("../../../../fixtures/tenhou/ranked_game.json")).unwrap();
    SessionGame {
        key: SessionGame::key(&events).unwrap(),
        events,
    }
}

#[test]
fn saved_replays_deduplicate_across_sources_and_survive_restart() {
    let directory = Directory::new();
    let library = Arc::new(ReplayLibrary::new(directory.0.clone()));
    let game = game();
    library
        .save(&game, "对局.json", ReplayOrigin::File)
        .unwrap();
    let second = library.clone();
    let copy = game.clone();
    std::thread::spawn(move || second.save(&copy, "来自链接", ReplayOrigin::Link))
        .join()
        .unwrap()
        .unwrap();
    library
        .save(&game, "会话中同一份牌谱", ReplayOrigin::Session)
        .unwrap();
    let list = library.list().unwrap();
    assert_eq!(list.replays.len(), 1);
    assert_eq!(list.replays[0].name, "对局");
    let restarted = ReplayLibrary::new(directory.0.clone());
    assert_eq!(restarted.get(&game.key).unwrap().game.events, game.events);
    let path = library.path(&game.key, false).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(parse_input(&text).unwrap().0, game.events);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn examples_are_installed_once_with_license_and_share_content_keys() {
    let directory = Directory::new();
    let library = ReplayLibrary::new(directory.0.clone());
    library.initialize().unwrap();
    let first = library.list().unwrap();
    let unique: HashSet<_> = EXAMPLES
        .iter()
        .map(|(_, json)| SessionGame::key(&replay::parse(json).unwrap().0).unwrap())
        .collect();
    assert_eq!(first.replays.len(), unique.len());
    assert!(
        first
            .replays
            .iter()
            .all(|record| record.origin == ReplayOrigin::Example)
    );
    assert!(directory.0.join("sessions").is_dir());
    assert!(library.directory(true).join("LICENSE.txt").is_file());
    library
        .save(&game(), "本地复制的示例", ReplayOrigin::File)
        .unwrap();
    library.initialize().unwrap();
    assert_eq!(library.list().unwrap().replays.len(), first.replays.len());
    assert_eq!(fs::read_dir(library.directory(false)).unwrap().count(), 0);
}

#[test]
fn rename_preserves_content_and_survives_restart_and_duplicate_imports() {
    let directory = Directory::new();
    let library = ReplayLibrary::new(directory.0.clone());
    let game = game();
    library
        .save(&game, "原名.json", ReplayOrigin::Example)
        .unwrap();
    let path = library.path(&game.key, true).unwrap();
    let before: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    let renamed = library.rename(&game.key, "  东一局的押引  ").unwrap();
    assert_eq!(renamed.name, "东一局的押引");
    assert_eq!(renamed.key, game.key);
    let mut after: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    after["name"] = before["name"].clone();
    assert_eq!(before, after);
    let reopened = ReplayLibrary::new(directory.0.clone());
    assert_eq!(reopened.get(&game.key).unwrap().name, "东一局的押引");
    assert_eq!(
        reopened
            .save(&game, "原名.json", ReplayOrigin::File)
            .unwrap(),
        "东一局的押引"
    );
    reopened.initialize().unwrap();
    assert_eq!(reopened.get(&game.key).unwrap().name, "东一局的押引");
    assert_eq!(
        reopened
            .rename(&game.key, &"🀄".repeat(80))
            .unwrap()
            .name
            .chars()
            .count(),
        80
    );
    let unchanged = fs::read(&path).unwrap();
    for name in ["".into(), "  ".into(), "一\n二".into(), "🀄".repeat(81)] {
        assert!(reopened.rename(&game.key, &name).is_err());
        assert_eq!(fs::read(&path).unwrap(), unchanged);
    }
    fs::write(&path, "broken").unwrap();
    assert!(reopened.rename(&game.key, "不能覆盖损坏文件").is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "broken");
}

#[test]
fn invalid_missing_and_corrupt_files_do_not_replace_existing_records() {
    let directory = Directory::new();
    let library = ReplayLibrary::new(directory.0.clone());
    let game = game();
    assert_eq!(
        library.get("../../outside").err().unwrap().code,
        "replay_key"
    );
    assert_eq!(library.get(&game.key).err().unwrap().code, "replay_missing");
    library.save(&game, "原牌谱", ReplayOrigin::File).unwrap();
    let mut tampered = game.clone();
    tampered.events.pop();
    assert!(
        library
            .save(&tampered, "伪造内容", ReplayOrigin::File)
            .is_err()
    );
    assert_eq!(library.get(&game.key).unwrap().game.events, game.events);
    let path = library.path(&game.key, false).unwrap();
    fs::write(&path, "broken").unwrap();
    assert!(
        library
            .save(&game, "不覆盖坏文件", ReplayOrigin::Link)
            .is_err()
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "broken");
    let list = library.list().unwrap();
    assert_eq!(list.replays.len(), 0);
    assert_eq!(list.warnings.len(), 1);
    library.initialize().unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "broken");
    assert_eq!(library.list().unwrap().warnings.len(), 1);
}
