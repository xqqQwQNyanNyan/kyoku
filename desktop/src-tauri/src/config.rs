use crate::UiError;
use kyoku::mortal::MortalConfig;
use std::path::{Path, PathBuf};
use tauri::Manager;

pub(crate) struct RuntimePaths {
    pub python: PathBuf,
    pub runtime: PathBuf,
    pub checkpoint: PathBuf,
    pub bundled: bool,
}

impl RuntimePaths {
    pub fn resolve(app: &tauri::AppHandle) -> Result<Self, UiError> {
        let resources = app
            .path()
            .resource_dir()
            .map_err(|_| UiError::new("runtime", "无法定位应用资源目录"))?;
        Ok(Self::from_roots(
            &resources,
            development_home().as_deref(),
            cfg!(target_os = "windows"),
        ))
    }

    fn from_roots(resources: &Path, development: Option<&Path>, windows: bool) -> Self {
        let root = resources.join("inference");
        if !root.exists()
            && let Some(home) = development
        {
            return Self {
                python: home.join(if windows {
                    "mortal/.venv/Scripts/python.exe"
                } else {
                    "mortal/.venv/bin/python"
                }),
                runtime: home.join("mortal/runtime"),
                checkpoint: home.join("mortal/models/mortal_582500.pth"),
                bundled: false,
            };
        }
        Self {
            python: root.join(if windows {
                "python/python.exe"
            } else {
                "python/bin/python3"
            }),
            runtime: root.join("runtime"),
            checkpoint: root.join("models/mortal_582500.pth"),
            bundled: true,
        }
    }

    pub fn borrowed(&self) -> MortalConfig<'_> {
        MortalConfig {
            python: &self.python,
            runtime: &self.runtime,
            checkpoint: &self.checkpoint,
        }
    }
}

pub(crate) fn development_home() -> Option<PathBuf> {
    // 发行版不读取编译机器的路径或开发 .env。
    if cfg!(debug_assertions) {
        Some(
            std::env::var_os("KYOKU_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")),
        )
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_resources_are_relative_to_the_installed_app() {
        let resources = Path::new("/Applications/复盘 工具.app/Contents/Resources");
        let paths = RuntimePaths::from_roots(resources, None, false);
        assert_eq!(paths.python, resources.join("inference/python/bin/python3"));
        assert!(paths.bundled);
    }

    #[test]
    fn development_can_use_the_existing_environment() {
        let paths = RuntimePaths::from_roots(
            Path::new("/missing-resources"),
            Some(Path::new("/work")),
            false,
        );
        assert_eq!(paths.python, Path::new("/work/mortal/.venv/bin/python"));
        assert!(!paths.bundled);
    }

    #[test]
    fn windows_uses_executable_paths() {
        let resources = Path::new("C:/Program Files/Kyoku/resources");
        let bundled = RuntimePaths::from_roots(resources, None, true);
        assert_eq!(
            bundled.python,
            resources.join("inference/python/python.exe")
        );

        let development =
            RuntimePaths::from_roots(Path::new("C:/missing"), Some(Path::new("C:/work")), true);
        assert_eq!(
            development.python,
            Path::new("C:/work/mortal/.venv/Scripts/python.exe")
        );
    }
}
