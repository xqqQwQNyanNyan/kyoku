use super::*;
use crate::{library::ReplayOrigin, sessions::SessionGame};
use kyoku::{
    agent::{AgentConfig, AgentContext, AgentSession},
    mahjong::player_index::PlayerIndex,
};
use serde_json::json;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(unique("kyoku-storage-test"));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn child(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(&path).unwrap();
        path
    }
    fn storage(&self) -> Storage {
        Storage::new(self.child("config"), self.child("old")).unwrap()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn seed(storage: &Storage) -> (String, String) {
    let (events, _) =
        crate::replay::parse(include_str!("../../../../fixtures/tenhou/ranked_game.json")).unwrap();
    let context = AgentContext::from_events(&events, PlayerIndex::new(0).unwrap(), 2).unwrap();
    let game = SessionGame {
        key: SessionGame::key(&events).unwrap(),
        events,
        round_details: Vec::new(),
    };
    let archive = AgentSession::with_context(
        &context,
        &AgentConfig {
            endpoint: "http://localhost/responses",
            model: "test",
            api_key: None,
            options: Default::default(),
        },
    )
    .unwrap()
    .archive()
    .clone();
    let stores = storage.read().unwrap();
    let view = stores
        .sessions
        .import(
            &json!({
                "version":3,"id":"sample","title":"已有对话","context_label":"我的牌谱",
                "created_at":1,"updated_at":1,"archive":archive,"pending_question":"待重试的问题",
                "game":game,"position":{"player":0,"event_index":2}
            })
            .to_string(),
        )
        .unwrap();
    stores
        .library
        .save(&game, "我的牌谱", ReplayOrigin::File)
        .unwrap();
    (view.document.id, game.key)
}

#[test]
fn migration_preserves_sessions_replay_links_and_originals_and_survives_restart() {
    let directory = Directory::new();
    let storage = directory.storage();
    let (id, key) = seed(&storage);
    let old = storage.read().unwrap().directory.clone();
    fs::write(old.join("settings.json"), "private-test-key").unwrap();
    let target = directory.child("中文 新位置");
    fs::write(target.join("keep.txt"), "unrelated").unwrap();
    let old_session = fs::read(old.join("sessions").join(format!("{id}.json"))).unwrap();
    let result = storage.migrate(&target, "move", |_| {}).unwrap();
    assert_eq!(
        Path::new(&result.directory),
        fs::canonicalize(&target).unwrap()
    );
    assert_eq!(
        fs::read(target.join("sessions").join(format!("{id}.json"))).unwrap(),
        old_session
    );
    assert_eq!(
        fs::read(old.join("sessions").join(format!("{id}.json"))).unwrap(),
        old_session
    );
    assert!(!target.join("settings.json").exists());
    assert_eq!(
        fs::read_to_string(target.join("keep.txt")).unwrap(),
        "unrelated"
    );
    let stores = storage.read().unwrap();
    assert_eq!(
        stores.sessions.open_game(&id).unwrap().game.unwrap().key,
        key
    );
    stores.sessions.rename(&id, "迁移后改名").unwrap();
    assert_eq!(
        stores.sessions.get(&id).unwrap().document.title,
        "迁移后改名"
    );
    assert_eq!(
        fs::read(old.join("sessions").join(format!("{id}.json"))).unwrap(),
        old_session
    );
    drop(stores);
    drop(storage);
    let restarted = directory.storage();
    assert_eq!(
        restarted
            .read()
            .unwrap()
            .sessions
            .get(&id)
            .unwrap()
            .document
            .title,
        "迁移后改名"
    );
    assert_eq!(
        restarted
            .read()
            .unwrap()
            .sessions
            .open_game(&id)
            .unwrap()
            .game
            .unwrap()
            .key,
        key
    );
    // 第二次切换也必须能替换已有配置文件（包括 Windows）。
    restarted
        .migrate(&directory.child("第三位置"), "again", |_| {})
        .unwrap();
    assert_eq!(
        directory
            .storage()
            .read()
            .unwrap()
            .sessions
            .get(&id)
            .unwrap()
            .document
            .title,
        "迁移后改名"
    );
}

#[test]
fn active_storage_operations_prevent_migration_and_migration_excludes_readers() {
    let directory = Directory::new();
    let storage = directory.storage();
    let target = directory.child("new");
    let reading = storage.read().unwrap();
    assert_eq!(
        storage.migrate(&target, "busy", |_| {}).err().unwrap().code,
        "storage_busy"
    );
    drop(reading);
    storage
        .migrate(&target, "move", |_| {
            assert_eq!(storage.read().err().unwrap().code, "storage_busy");
        })
        .unwrap();
}

#[test]
fn cancellation_and_config_failure_keep_original_location_and_allow_retry() {
    let directory = Directory::new();
    let storage = directory.storage();
    let (id, _) = seed(&storage);
    let old = storage.view().unwrap().directory;
    let target = directory.child("new");
    let result = storage.migrate(&target, "cancel", |progress| {
        if progress.copied_files > 0 {
            storage.cancel("cancel").unwrap();
        }
    });
    assert_eq!(result.err().unwrap().code, "storage_cancelled");
    assert_eq!(storage.view().unwrap().directory, old);
    assert!(!target.join("sessions").exists());
    assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
    assert!(storage.read().unwrap().sessions.get(&id).is_ok());
    fs::create_dir(storage.config_directory.join("storage.json")).unwrap();
    assert!(storage.migrate(&target, "fail", |_| {}).is_err());
    assert_eq!(storage.view().unwrap().directory, old);
    assert!(!target.join("replays").exists());
    fs::remove_dir(storage.config_directory.join("storage.json")).unwrap();
    storage
        .migrate(&target, "retry", |_| {
            storage.cancel("cancel").unwrap(); // 迟到的取消不能影响新迁移。
        })
        .unwrap();
}

#[test]
fn nested_paths_and_existing_data_are_never_overwritten() {
    let directory = Directory::new();
    let storage = directory.storage();
    let old = storage.view().unwrap().directory;
    let child = Path::new(&old).join("nested");
    fs::create_dir(&child).unwrap();
    for path in [child, directory.0.clone()] {
        assert_eq!(
            storage.migrate(&path, "nested", |_| {}).err().unwrap().code,
            "storage_path"
        );
    }
    let target = directory.child("occupied");
    fs::write(target.join("sessions"), "existing").unwrap();
    assert_eq!(
        storage
            .migrate(&target, "occupied", |_| {})
            .err()
            .unwrap()
            .code,
        "storage_conflict"
    );
    assert_eq!(
        fs::read_to_string(target.join("sessions")).unwrap(),
        "existing"
    );
    assert_eq!(storage.view().unwrap().directory, old);
    assert!(
        storage
            .migrate(Path::new("relative"), "invalid", |_| {})
            .is_err()
    );
    assert!(storage.migrate(Path::new(&old), "same", |_| {}).is_ok());
}

#[test]
fn disconnected_custom_directory_does_not_create_an_empty_library() {
    let directory = Directory::new();
    let storage = directory.storage();
    let target = directory.child("external");
    storage.migrate(&target, "move", |_| {}).unwrap();
    drop(storage);
    let detached = directory.0.join("detached");
    fs::rename(&target, &detached).unwrap();
    let restarted = directory.storage();
    assert!(!restarted.view().unwrap().available);
    assert_eq!(restarted.read().err().unwrap().code, "storage_unavailable");
    assert!(!target.exists());
    fs::rename(detached, &target).unwrap();
    assert!(restarted.read().is_ok());
}

#[test]
fn deleted_example_markers_and_unreadable_records_are_preserved() {
    let directory = Directory::new();
    let storage = directory.storage();
    let (_, key) = seed(&storage);
    let stores = storage.read().unwrap();
    stores.library.delete(&key).unwrap();
    fs::write(
        stores.directory.join("sessions/broken.json"),
        "damaged data",
    )
    .unwrap();
    drop(stores);
    let target = directory.child("new");
    storage.migrate(&target, "move", |_| {}).unwrap();
    drop(storage);
    let restarted = directory.storage();
    assert_eq!(
        restarted
            .read()
            .unwrap()
            .library
            .get(&key)
            .err()
            .unwrap()
            .code,
        "replay_missing"
    );
    assert_eq!(
        fs::read_to_string(target.join("sessions/broken.json")).unwrap(),
        "damaged data"
    );
}

#[cfg(unix)]
#[test]
fn symlinks_are_rejected_without_copying_files_outside_the_data_directory() {
    use std::os::unix::fs::symlink;
    let directory = Directory::new();
    let storage = directory.storage();
    let outside = directory.0.join("private.txt");
    fs::write(&outside, "private").unwrap();
    let source = storage.read().unwrap().directory.clone();
    symlink(&outside, source.join("sessions/link.json")).unwrap();
    let target = directory.child("new");
    assert_eq!(
        storage.migrate(&target, "move", |_| {}).err().unwrap().code,
        "storage_path"
    );
    assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
    assert_eq!(fs::read_to_string(outside).unwrap(), "private");
}
