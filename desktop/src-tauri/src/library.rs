//! 本地牌谱库；按转换后的事件内容去重，session 只保存牌谱标识。

use crate::{UiError, lock, replay, sessions::SessionGame};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_NAME_CHARS: usize = 80;
static FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
include!(concat!(env!("OUT_DIR"), "/replay_examples.rs"));

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReplayOrigin {
    Example,
    File,
    Link,
    Session,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedReplay {
    format: String,
    version: u32,
    pub name: String,
    origin: ReplayOrigin,
    saved_at: u64,
    pub game: SessionGame,
}

#[derive(Serialize)]
pub(crate) struct ReplaySummary {
    key: String,
    name: String,
    origin: ReplayOrigin,
    saved_at: u64,
}

#[derive(Serialize)]
pub(crate) struct ReplayList {
    replays: Vec<ReplaySummary>,
    pub warnings: Vec<String>,
    directory: String,
}

pub(crate) struct ReplayLibrary {
    data_directory: PathBuf,
    gate: Mutex<()>,
}

fn io_error() -> UiError {
    UiError::new("replay_io", "无法读写牌谱库，请检查数据目录权限和存储空间")
}

fn name_for_file(name: &str) -> String {
    let name = name.trim();
    let name = name
        .strip_suffix(".json")
        .or_else(|| name.strip_suffix(".JSON"))
        .unwrap_or(name);
    let title: String = name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_NAME_CHARS)
        .collect();
    if title.is_empty() {
        "未命名牌谱".into()
    } else {
        title
    }
}

impl SavedReplay {
    fn parse(text: &str) -> Result<Self, UiError> {
        if text.len() as u64 > MAX_FILE_BYTES {
            return Err(UiError::new("replay_size", "保存的牌谱不能超过 32 MiB"));
        }
        let record: Self = serde_json::from_str(text)
            .map_err(|_| UiError::new("replay_format", "不是有效的 Kyoku 牌谱文件"))?;
        if record.format != "kyoku-replay" || record.version != 1 || record.name.len() > 2048 {
            return Err(UiError::new("replay_format", "牌谱版本或名称无效"));
        }
        record.game.validate()?;
        Ok(record)
    }
}

impl ReplayLibrary {
    pub fn new(data_directory: PathBuf) -> Self {
        Self {
            data_directory,
            gate: Mutex::new(()),
        }
    }

    fn directory(&self, example: bool) -> PathBuf {
        self.data_directory
            .join("replays")
            .join(if example { "examples" } else { "imported" })
    }

    fn path(&self, key: &str, example: bool) -> Result<PathBuf, UiError> {
        if !SessionGame::valid_key(key) {
            return Err(UiError::new("replay_key", "牌谱标识无效"));
        }
        Ok(self.directory(example).join(format!("{key}.json")))
    }

    pub fn initialize(&self) -> Result<(), UiError> {
        fs::create_dir_all(self.data_directory.join("sessions")).map_err(|_| io_error())?;
        fs::create_dir_all(self.directory(false)).map_err(|_| io_error())?;
        fs::create_dir_all(self.directory(true)).map_err(|_| io_error())?;
        for (name, json) in EXAMPLES {
            let (events, _) = replay::parse(json)?;
            let game = SessionGame {
                key: SessionGame::key(&events)?,
                events,
            };
            // 已有文件由列表报告损坏，不覆盖用户文件，也不阻止应用启动。
            if self
                .path(&game.key, true)?
                .try_exists()
                .map_err(|_| io_error())?
                || self
                    .path(&game.key, false)?
                    .try_exists()
                    .map_err(|_| io_error())?
            {
                continue;
            }
            self.save(&game, name, ReplayOrigin::Example)?;
        }
        let license = self.directory(true).join("LICENSE.txt");
        if !license.try_exists().map_err(|_| io_error())? {
            write_new(&license, include_bytes!("../../../fixtures/tenhou/LICENSE"))?;
        }
        Ok(())
    }

    /// 保存完成才返回；已有内容不覆盖，名称不参与文件路径。
    pub fn save(
        &self,
        game: &SessionGame,
        name: &str,
        origin: ReplayOrigin,
    ) -> Result<String, UiError> {
        if name.len() > 2048 {
            return Err(UiError::new("replay_name", "牌谱名称不能超过 2 KiB"));
        }
        game.validate()?;
        let _guard = lock(&self.gate)?;
        for example in [true, false] {
            let path = self.path(&game.key, example)?;
            if path.try_exists().map_err(|_| io_error())? {
                let existing = read(&path)?;
                if existing.game.key != game.key || existing.game.events != game.events {
                    return Err(UiError::new(
                        "replay_format",
                        "已有牌谱内容不匹配，原文件已保留",
                    ));
                }
                return Ok(existing.name);
            }
        }
        // 来自旧 session 的事件也必须能重建牌桌，才能替换其内嵌牌谱。
        replay::replay(&game.events)?;
        let record = SavedReplay {
            format: "kyoku-replay".into(),
            version: 1,
            name: name_for_file(name),
            origin,
            saved_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            game: game.clone(),
        };
        let path = self.path(&game.key, origin == ReplayOrigin::Example)?;
        write_record(&path, &record)?;
        Ok(record.name)
    }

    /// 只修改显示名称，保留内容标识、文件位置和所有 session 引用。
    pub fn rename(&self, key: &str, name: &str) -> Result<ReplaySummary, UiError> {
        let name = name.trim();
        if name.is_empty()
            || name.chars().count() > MAX_NAME_CHARS
            || name.chars().any(char::is_control)
        {
            return Err(UiError::new(
                "replay_name",
                "牌谱名称需为 1–80 个字符，且不能换行",
            ));
        }
        let _guard = lock(&self.gate)?;
        let mut record = self.get(key)?;
        let example_path = self.path(key, true)?;
        let path = if example_path.try_exists().map_err(|_| io_error())? {
            example_path
        } else {
            self.path(key, false)?
        };
        record.name = name_for_file(name);
        write_record(&path, &record)?;
        Ok(ReplaySummary {
            key: record.game.key,
            name: record.name,
            origin: record.origin,
            saved_at: record.saved_at,
        })
    }

    pub fn get(&self, key: &str) -> Result<SavedReplay, UiError> {
        for example in [true, false] {
            let path = self.path(key, example)?;
            if path.try_exists().map_err(|_| io_error())? {
                let record = read(&path)?;
                if record.game.key != key {
                    return Err(UiError::new("replay_format", "牌谱文件与标识不匹配"));
                }
                return Ok(record);
            }
        }
        Err(UiError::new(
            "replay_missing",
            "关联的牌谱文件不存在，请重新导入原牌谱；已有会话记录仍可查看",
        ))
    }

    pub fn list(&self) -> Result<ReplayList, UiError> {
        let mut result = ReplayList {
            replays: vec![],
            warnings: vec![],
            directory: self.data_directory.to_string_lossy().into_owned(),
        };
        let mut seen = HashSet::new();
        for example in [true, false] {
            let entries = match fs::read_dir(self.directory(example)) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => return Err(io_error()),
            };
            for entry in entries {
                let path = entry.map_err(|_| io_error())?.path();
                if path.extension().is_none_or(|ext| ext != "json") {
                    continue;
                }
                match read(&path) {
                    Ok(record)
                        if path.file_stem().and_then(|stem| stem.to_str())
                            == Some(&record.game.key) =>
                    {
                        if seen.insert(record.game.key.clone()) {
                            result.replays.push(ReplaySummary {
                                key: record.game.key,
                                name: name_for_file(&record.name),
                                origin: record.origin,
                                saved_at: record.saved_at,
                            });
                        }
                    }
                    _ => result.warnings.push(format!(
                        "无法读取牌谱 {}，原文件已保留",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    )),
                }
            }
        }
        result.replays.sort_by(|a, b| {
            b.saved_at
                .cmp(&a.saved_at)
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(result)
    }

    pub fn open_directory(&self) -> Result<(), UiError> {
        let program = if cfg!(target_os = "macos") {
            "open"
        } else if cfg!(target_os = "windows") {
            "explorer.exe"
        } else {
            "xdg-open"
        };
        fs::create_dir_all(&self.data_directory).map_err(|_| io_error())?;
        let status = std::process::Command::new(program)
            .arg(&self.data_directory)
            .status()
            .map_err(|_| UiError::new("open_directory", "无法打开数据文件夹"))?;
        if !status.success() {
            return Err(UiError::new("open_directory", "无法打开数据文件夹"));
        }
        Ok(())
    }
}

fn write_record(path: &Path, record: &SavedReplay) -> Result<(), UiError> {
    let bytes = serde_json::to_vec(record).map_err(|_| io_error())?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(UiError::new("replay_size", "保存的牌谱不能超过 32 MiB"));
    }
    let directory = path.parent().ok_or_else(io_error)?;
    fs::create_dir_all(directory).map_err(|_| io_error())?;
    let temporary = directory.join(format!(
        ".{}-{}.tmp",
        std::process::id(),
        FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    write_new(&temporary, &bytes)?;
    if fs::rename(&temporary, path).is_err() {
        let _ = fs::remove_file(temporary);
        return Err(io_error());
    }
    Ok(())
}

fn read(path: &Path) -> Result<SavedReplay, UiError> {
    if fs::metadata(path).map_err(|_| io_error())?.len() > MAX_FILE_BYTES {
        return Err(UiError::new("replay_size", "保存的牌谱不能超过 32 MiB"));
    }
    SavedReplay::parse(&fs::read_to_string(path).map_err(|_| io_error())?)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), UiError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| io_error())?;
    if file.write_all(bytes).and_then(|_| file.sync_all()).is_err() {
        let _ = fs::remove_file(path);
        return Err(io_error());
    }
    Ok(())
}

/// 数据文件夹里的 Kyoku 牌谱也可以经由本地文件入口重新导入。
pub(crate) fn parse_input(
    json: &str,
) -> Result<(Vec<convlog::Event>, replay::ReplayData), UiError> {
    if json.len() as u64 > MAX_FILE_BYTES {
        return Err(UiError::new("replay_size", "保存的牌谱不能超过 32 MiB"));
    }
    if serde_json::from_str::<serde_json::Value>(json).is_ok_and(|v| v["format"] == "kyoku-replay")
    {
        let record = SavedReplay::parse(json)?;
        let data = replay::replay(&record.game.events)?;
        Ok((record.game.events, data))
    } else {
        replay::parse(json)
    }
}

#[cfg(test)]
mod tests;
