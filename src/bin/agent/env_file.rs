use std::{collections::BTreeMap, error::Error, fmt, fs::File, io, path::Path};

pub(super) struct EnvFile {
    values: BTreeMap<String, String>,
}

#[derive(Debug)]
pub(super) enum EnvFileError {
    Read(io::Error),
    InvalidSyntax,
}

impl fmt::Display for EnvFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(f, "cannot read .env: {error}"),
            Self::InvalidSyntax => write!(
                f,
                "invalid .env syntax; check key=value entries and quotes (contents omitted)"
            ),
        }
    }
}

impl Error for EnvFileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(error) => Some(error),
            Self::InvalidSyntax => None,
        }
    }
}

impl EnvFile {
    pub(super) fn load(path: &Path) -> Result<Self, EnvFileError> {
        match File::open(path) {
            Ok(file) => Self::parse(file),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self {
                values: BTreeMap::new(),
            }),
            Err(error) => Err(EnvFileError::Read(error)),
        }
    }

    fn parse(reader: impl io::Read) -> Result<Self, EnvFileError> {
        let mut values = BTreeMap::new();
        // 只解析到内存，不修改全局环境，也不向 Mortal 子进程注入文件中的密钥。
        for entry in dotenvy::from_read_iter(reader) {
            let (name, value) = entry.map_err(|error| match error {
                dotenvy::Error::Io(error) => EnvFileError::Read(error),
                // dotenvy 的解析错误包含原始行，不能放进 Display、Debug 或 source。
                _ => EnvFileError::InvalidSyntax,
            })?;
            values.entry(name).or_insert(value);
        }
        Ok(Self { values })
    }

    pub(super) fn get(&self, name: &str, inherited: Option<String>) -> Option<String> {
        inherited.or_else(|| self.values.get(name).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quotes_comments_crlf_and_preserves_environment_priority() {
        let file = EnvFile::parse(&b"# config\r\nexport OPENAI_MODEL=\"test model\" # note\r\nAGENT_API_KEY='test-$literal#key'\r\nOPENAI_MODEL=ignored\r\n"[..]).unwrap();
        assert_eq!(
            file.get("OPENAI_MODEL", None).as_deref(),
            Some("test model")
        );
        assert_eq!(
            file.get("AGENT_API_KEY", None).as_deref(),
            Some("test-$literal#key")
        );
        assert_eq!(
            file.get("OPENAI_MODEL", Some("shell-model".into()))
                .as_deref(),
            Some("shell-model")
        );
        assert_eq!(
            file.get("AGENT_API_KEY", Some(String::new())),
            Some(String::new())
        );
        assert_eq!(file.get("UNKNOWN", None), None);
    }

    #[test]
    fn parsing_errors_never_expose_the_secret_line() {
        let error = EnvFile::parse(&b"AGENT_API_KEY='test-secret-without-closing-quote"[..])
            .err()
            .unwrap();
        assert!(matches!(error, EnvFileError::InvalidSyntax));
        assert!(!format!("{error:?} {error}").contains("test-secret"));
        assert!(error.source().is_none());
    }

    #[test]
    fn file_settings_keep_custom_credentials_separate() {
        let file = EnvFile::parse(&b"KYOKU_OPENAI_ENDPOINT='http://localhost:8080/v1/responses'\nOPENAI_API_KEY='test-official-key'\n"[..]).unwrap();
        let (endpoint, key) = super::super::connection_settings(None, |name| file.get(name, None));
        assert_eq!(endpoint, "http://localhost:8080/v1/responses");
        assert_eq!(key, None);
        let (_, key) = super::super::connection_settings(None, |name| {
            file.get(
                name,
                (name == "AGENT_API_KEY").then(|| "test-custom-key".into()),
            )
        });
        assert_eq!(key.as_deref(), Some("test-custom-key"));
    }

    #[test]
    fn example_is_valid_and_contains_no_usable_key() {
        let file = EnvFile::parse(include_bytes!("../../../.env.example").as_slice()).unwrap();
        assert!(file.get("KYOKU_OPENAI_ENDPOINT", None).is_some());
        assert!(file.get("OPENAI_MODEL", None).is_some());
        assert_eq!(file.get("AGENT_API_KEY", None).as_deref(), Some(""));
        assert_eq!(file.get("OPENAI_API_KEY", None), None);
    }

    #[test]
    fn missing_file_is_optional_but_read_errors_are_reported() {
        let missing = std::env::temp_dir()
            .join(format!("kyoku-missing-env-{}", std::process::id()))
            .join(".env");
        assert!(EnvFile::load(&missing).unwrap().values.is_empty());
        struct Unreadable;
        impl io::Read for Unreadable {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            }
        }
        assert!(matches!(
            EnvFile::parse(Unreadable),
            Err(EnvFileError::Read(_))
        ));
    }
}
