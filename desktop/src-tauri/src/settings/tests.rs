use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

struct TestStore(SettingsStore);

impl TestStore {
    fn new() -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        Self(SettingsStore::new(
            std::env::temp_dir().join(format!(
                "kyoku-settings-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            )),
            None,
        ))
    }
}

impl Drop for TestStore {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0.directory);
    }
}

fn input(endpoint: &str, key: &str) -> SettingsInput {
    SettingsInput {
        endpoint: endpoint.into(),
        model: "test-model".into(),
        api_key: key.into(),
        clear_key: false,
    }
}

#[test]
fn saves_configuration_without_exposing_the_key_and_preserves_it_on_model_change() {
    let store = TestStore::new();
    let view = store
        .0
        .save(input(DEFAULT_ENDPOINT, "private-test-key"))
        .unwrap();
    assert!(view.has_api_key && view.saved);
    assert!(
        !serde_json::to_string(&view)
            .unwrap()
            .contains("private-test-key")
    );
    assert!(
        fs::read_to_string(store.0.directory.join("settings.json"))
            .unwrap()
            .contains("private-test-key")
    );
    let mut changed = input(DEFAULT_ENDPOINT, "");
    changed.model = "next-model".into();
    store.0.save(changed).unwrap();
    let saved = store.0.read().unwrap().unwrap();
    assert_eq!(saved.model, "next-model");
    assert_eq!(saved.api_key.as_deref(), Some("private-test-key"));
    let reopened = SettingsStore::new(store.0.directory.clone(), None);
    let config = reopened.load().unwrap();
    assert_eq!(config.key.as_deref(), Some("private-test-key"));
    assert_eq!(config.model, "next-model");
}

#[test]
fn changing_endpoint_never_reuses_the_previous_key() {
    let store = TestStore::new();
    store
        .0
        .save(input(DEFAULT_ENDPOINT, "official-test-key"))
        .unwrap();
    assert!(
        store
            .0
            .draft(input("https://other.example/v1/responses", ""))
            .is_err()
    );
    let local = store
        .0
        .draft(input("http://localhost:8000/v1/responses", ""))
        .unwrap();
    assert!(local.key.is_none());
    assert_eq!(store.0.view().unwrap().endpoint, DEFAULT_ENDPOINT);
}

#[test]
fn failed_file_write_preserves_previous_configuration() {
    let store = TestStore::new();
    store
        .0
        .save(input(DEFAULT_ENDPOINT, "original-key"))
        .unwrap();
    let original = fs::read(store.0.directory.join("settings.json")).unwrap();
    fs::create_dir(store.0.directory.join("settings.json.tmp")).unwrap();
    assert!(
        store
            .0
            .save(input(DEFAULT_ENDPOINT, "replacement-key"))
            .is_err()
    );
    assert_eq!(
        fs::read(store.0.directory.join("settings.json")).unwrap(),
        original
    );
    assert_eq!(store.0.load().unwrap().key.as_deref(), Some("original-key"));
}

#[test]
fn deleting_key_is_saved_without_disabling_local_replay() {
    let store = TestStore::new();
    store.0.save(input(DEFAULT_ENDPOINT, "test-key")).unwrap();
    let mut cleared = input(DEFAULT_ENDPOINT, "");
    cleared.clear_key = true;
    assert!(!store.0.save(cleared).unwrap().has_api_key);
    assert!(store.0.load().unwrap().borrowed().validate().is_err());
    assert!(store.0.load().unwrap().key.is_none());
    assert!(
        !fs::read_to_string(store.0.directory.join("settings.json"))
            .unwrap()
            .contains("test-key")
    );
}

#[test]
fn custom_environment_endpoint_does_not_fall_back_to_official_key() {
    let config = LlmConfig::from_values(|name| match name {
        "KYOKU_OPENAI_ENDPOINT" => Some("http://localhost:8080/v1/responses".into()),
        "OPENAI_API_KEY" => Some("official-secret".into()),
        "OPENAI_MODEL" => Some("test-model".into()),
        _ => None,
    });
    assert!(config.key.is_none());
}

#[test]
fn invalid_input_does_not_write_a_key_or_settings() {
    let store = TestStore::new();
    for endpoint in [
        "http://remote.example/v1/responses",
        "https://host/v1/responses?key=secret",
        "https://user:secret@host/v1/responses",
    ] {
        assert!(store.0.save(input(endpoint, "test-key")).is_err());
    }
    assert!(!store.0.directory.exists());
}

#[test]
fn chat_endpoint_can_be_saved_and_reloaded_with_its_own_key() {
    let store = TestStore::new();
    let endpoint = "https://service.example/v1/chat/completions";
    let view = store.0.save(input(endpoint, "chat-test-key")).unwrap();
    assert_eq!(view.endpoint, endpoint);
    assert!(view.saved && view.has_api_key);
    let config = store.0.draft(input(endpoint, "")).unwrap();
    assert_eq!(config.endpoint, endpoint);
    assert_eq!(config.key.as_deref(), Some("chat-test-key"));
    config.borrowed().validate().unwrap();
    assert!(store.0.draft(input(DEFAULT_ENDPOINT, "")).is_err());
}

#[test]
fn old_keychain_settings_keep_endpoint_and_model_but_require_a_new_key() {
    let store = TestStore::new();
    fs::create_dir_all(&store.0.directory).unwrap();
    fs::write(
        store.0.directory.join("settings.json"),
        serde_json::to_vec(&serde_json::json!({
            "endpoint": DEFAULT_ENDPOINT,
            "model": "old-model",
            "credential": "old-keychain-id"
        }))
        .unwrap(),
    )
    .unwrap();
    let view = store.0.view().unwrap();
    assert!(view.saved);
    assert!(!view.has_api_key);
    assert_eq!(view.endpoint, DEFAULT_ENDPOINT);
    assert_eq!(view.model, "old-model");
    assert!(store.0.load().unwrap().key.is_none());
    assert!(store.0.draft(input(DEFAULT_ENDPOINT, "")).is_err());
    store
        .0
        .save(input(DEFAULT_ENDPOINT, "replacement-key"))
        .unwrap();
    assert_eq!(
        store.0.load().unwrap().key.as_deref(),
        Some("replacement-key")
    );
    assert!(
        !fs::read_to_string(store.0.directory.join("settings.json"))
            .unwrap()
            .contains("credential")
    );
}

#[test]
fn stale_temporary_file_does_not_prevent_saving() {
    let store = TestStore::new();
    fs::create_dir_all(&store.0.directory).unwrap();
    fs::write(
        store.0.directory.join("settings.json.tmp"),
        "partial settings",
    )
    .unwrap();
    store.0.save(input(DEFAULT_ENDPOINT, "new-key")).unwrap();
    assert_eq!(store.0.load().unwrap().key.as_deref(), Some("new-key"));
    assert!(!store.0.directory.join("settings.json.tmp").exists());
}

#[cfg(unix)]
#[test]
fn settings_file_is_private_even_when_replacing_an_old_public_file() {
    use std::os::unix::fs::PermissionsExt;

    let store = TestStore::new();
    store.0.save(input(DEFAULT_ENDPOINT, "first-key")).unwrap();
    let path = store.0.directory.join("settings.json");
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    store.0.save(input(DEFAULT_ENDPOINT, "next-key")).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(store.0.load().unwrap().key.as_deref(), Some("next-key"));
}
