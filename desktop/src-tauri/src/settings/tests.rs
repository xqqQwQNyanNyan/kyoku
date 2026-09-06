use super::*;
use std::cell::{Cell, RefCell};

#[derive(Default)]
struct MemorySecrets {
    values: RefCell<BTreeMap<String, String>>,
    fail: Cell<bool>,
}

impl Secrets for MemorySecrets {
    fn get(&self, id: &str) -> Result<String, UiError> {
        self.values
            .borrow()
            .get(id)
            .cloned()
            .ok_or_else(|| UiError::new("keychain", "missing test key"))
    }
    fn set(&self, id: &str, key: &str) -> Result<(), UiError> {
        if self.fail.get() {
            return Err(UiError::new("keychain", "test failure"));
        }
        self.values.borrow_mut().insert(id.into(), key.into());
        Ok(())
    }
    fn delete(&self, id: &str) -> Result<(), UiError> {
        self.values.borrow_mut().remove(id);
        Ok(())
    }
}

struct TestStore(SettingsStore);

impl TestStore {
    fn new() -> Self {
        Self(SettingsStore::new(
            std::env::temp_dir().join(format!("kyoku-settings-{}", credential_id().unwrap())),
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
    let secrets = MemorySecrets::default();
    let view = store
        .0
        .save_with(input(DEFAULT_ENDPOINT, "private-test-key"), &secrets)
        .unwrap();
    assert!(view.has_api_key && view.saved);
    assert!(
        !serde_json::to_string(&view)
            .unwrap()
            .contains("private-test-key")
    );
    assert!(
        !fs::read_to_string(store.0.directory.join("settings.json"))
            .unwrap()
            .contains("private-test-key")
    );
    let mut changed = input(DEFAULT_ENDPOINT, "");
    changed.model = "next-model".into();
    store.0.save_with(changed, &secrets).unwrap();
    let saved = store.0.read().unwrap().unwrap();
    assert_eq!(saved.model, "next-model");
    assert_eq!(
        secrets.get(saved.credential.as_deref().unwrap()).unwrap(),
        "private-test-key"
    );
    assert_eq!(secrets.values.borrow().len(), 1);
}

#[test]
fn changing_endpoint_never_reuses_the_previous_key() {
    let store = TestStore::new();
    let secrets = MemorySecrets::default();
    store
        .0
        .save_with(input(DEFAULT_ENDPOINT, "official-test-key"), &secrets)
        .unwrap();
    assert!(
        store
            .0
            .draft(input("https://other.example/v1/responses", ""), &secrets)
            .is_err()
    );
    let local = store
        .0
        .draft(input("http://localhost:8000/v1/responses", ""), &secrets)
        .unwrap();
    assert!(local.key.is_none());
    assert_eq!(store.0.view().unwrap().endpoint, DEFAULT_ENDPOINT);
}

#[test]
fn failed_keychain_or_file_write_preserves_previous_configuration() {
    let store = TestStore::new();
    let secrets = MemorySecrets::default();
    store
        .0
        .save_with(input(DEFAULT_ENDPOINT, "original-key"), &secrets)
        .unwrap();
    let original = fs::read(store.0.directory.join("settings.json")).unwrap();
    secrets.fail.set(true);
    assert!(
        store
            .0
            .save_with(input(DEFAULT_ENDPOINT, "replacement-key"), &secrets)
            .is_err()
    );
    secrets.fail.set(false);
    fs::create_dir(store.0.directory.join("settings.json.tmp")).unwrap();
    assert!(
        store
            .0
            .save_with(input(DEFAULT_ENDPOINT, "replacement-key"), &secrets)
            .is_err()
    );
    assert_eq!(
        fs::read(store.0.directory.join("settings.json")).unwrap(),
        original
    );
    assert_eq!(secrets.values.borrow().len(), 1);
    assert_eq!(
        secrets.values.borrow().values().next().unwrap(),
        "original-key"
    );
}

#[test]
fn deleting_key_is_saved_without_disabling_local_replay() {
    let store = TestStore::new();
    let secrets = MemorySecrets::default();
    store
        .0
        .save_with(input(DEFAULT_ENDPOINT, "test-key"), &secrets)
        .unwrap();
    let mut cleared = input(DEFAULT_ENDPOINT, "");
    cleared.clear_key = true;
    assert!(!store.0.save_with(cleared, &secrets).unwrap().has_api_key);
    assert!(secrets.values.borrow().is_empty());
    assert!(store.0.load().unwrap().borrowed().validate().is_err());
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
    let secrets = MemorySecrets::default();
    for endpoint in [
        "http://remote.example/v1/responses",
        "https://host/v1/responses?key=secret",
        "https://user:secret@host/v1/responses",
    ] {
        assert!(
            store
                .0
                .save_with(input(endpoint, "test-key"), &secrets)
                .is_err()
        );
    }
    assert!(secrets.values.borrow().is_empty());
    assert!(!store.0.directory.exists());
}
