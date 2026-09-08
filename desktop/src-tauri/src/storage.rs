//! 牌谱与会话共用数据根目录；迁移期间独占存储，配置落盘后才切换。

use crate::{UiError, library::ReplayLibrary, lock, sessions::SessionStore};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, RwLock, RwLockReadGuard,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
const TREES: [&str; 2] = ["replays", "sessions"];

pub(crate) struct Stores {
    directory: PathBuf,
    library: Arc<ReplayLibrary>,
    sessions: SessionStore,
}

impl Stores {
    fn new(directory: PathBuf) -> Self {
        let library = Arc::new(ReplayLibrary::new(directory.clone()));
        Self {
            sessions: SessionStore::new(directory.join("sessions"), library.clone()),
            library,
            directory,
        }
    }

    // 只借出当前库，调用方不能克隆 Arc 后绕过迁移锁写入旧目录。
    pub fn library(&self) -> &ReplayLibrary {
        &self.library
    }

    pub fn sessions(&self) -> &SessionStore {
        &self.sessions
    }

    fn available(&self) -> bool {
        TREES.iter().all(|name| self.directory.join(name).is_dir())
    }
}

#[derive(Serialize)]
pub(crate) struct StorageView {
    directory: String,
    available: bool,
}

#[derive(Clone, Serialize)]
pub(crate) struct MigrationProgress {
    copied_files: u64,
    total_files: u64,
    copied_bytes: u64,
    total_bytes: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Location {
    directory: PathBuf,
}

pub(crate) struct Storage {
    config_directory: PathBuf,
    stores: RwLock<Stores>,
    migration: Mutex<Option<(String, Arc<AtomicBool>)>>,
}

struct Migration<'a> {
    storage: &'a Storage,
    cancelled: Arc<AtomicBool>,
}

impl Migration<'_> {
    fn check(&self) -> Result<(), UiError> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err(UiError::new(
                "storage_cancelled",
                "迁移已取消，仍使用原数据目录",
            ));
        }
        Ok(())
    }
}

impl Drop for Migration<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.storage.migration.lock() {
            *active = None;
        }
    }
}

fn io_error(action: &str, path: &Path, error: std::io::Error) -> UiError {
    UiError::new(
        "storage_io",
        format!("{action}失败（{}）：{error}", path.display()),
    )
}

fn busy() -> UiError {
    UiError::new(
        "storage_busy",
        "数据正在使用或迁移，请等待导入、问答和保存完成后重试",
    )
}

fn unavailable() -> UiError {
    UiError::new(
        "storage_unavailable",
        "数据目录不可用，请重新连接存储设备或恢复原目录；不会自动改用空目录",
    )
}

fn unique(prefix: &str) -> String {
    format!(
        ".{prefix}-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

fn new_file(path: &Path) -> Result<File, UiError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map_err(|error| io_error("创建文件", path, error))
}

impl Storage {
    pub fn new(config_directory: PathBuf, default_directory: PathBuf) -> Result<Self, UiError> {
        let path = config_directory.join("storage.json");
        let location = match File::open(&path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(16 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|error| io_error("读取数据位置", &path, error))?;
                if bytes.len() > 16 * 1024 {
                    return Err(UiError::new("storage_config", "数据位置配置过大"));
                }
                let location: Location = serde_json::from_slice(&bytes).map_err(|_| {
                    UiError::new("storage_config", "数据位置配置无效，请检查 storage.json")
                })?;
                if !location.directory.is_absolute() {
                    return Err(UiError::new("storage_config", "数据位置必须是绝对路径"));
                }
                Some(location)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(io_error("读取数据位置", &path, error)),
        };
        let configured = location.is_some();
        let directory = location.map_or(default_directory, |value| value.directory);
        let stores = Stores::new(directory);
        // 自选磁盘断开或目录丢失时保留位置，允许界面报告问题，不创建一份空库。
        if !configured || stores.available() {
            stores.library.initialize()?;
        }
        Ok(Self {
            config_directory,
            stores: RwLock::new(stores),
            migration: Mutex::new(None),
        })
    }

    pub fn read(&self) -> Result<RwLockReadGuard<'_, Stores>, UiError> {
        let stores = self.stores.try_read().map_err(|_| busy())?;
        if !stores.available() {
            return Err(unavailable());
        }
        Ok(stores)
    }

    pub fn view(&self) -> Result<StorageView, UiError> {
        let stores = self.stores.try_read().map_err(|_| busy())?;
        Ok(StorageView {
            directory: stores.directory.to_string_lossy().into_owned(),
            available: stores.available(),
        })
    }

    pub fn cancel(&self, id: &str) -> Result<(), UiError> {
        if let Some((active_id, cancelled)) = lock(&self.migration)?.as_ref()
            && active_id == id
        {
            cancelled.store(true, Ordering::Relaxed);
        }
        Ok(())
    }

    pub fn migrate(
        &self,
        destination: &Path,
        id: &str,
        on_progress: impl Fn(MigrationProgress),
    ) -> Result<StorageView, UiError> {
        if id.is_empty() || id.len() > 128 || !destination.is_absolute() {
            return Err(UiError::new("storage_path", "请选择有效的绝对目录路径"));
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        {
            let mut active = lock(&self.migration)?;
            if active.is_some() {
                return Err(busy());
            }
            *active = Some((id.into(), cancelled.clone()));
        }
        let migration = Migration {
            storage: self,
            cancelled,
        };
        let mut stores = self.stores.try_write().map_err(|_| busy())?;
        if !stores.available() {
            return Err(unavailable());
        }
        let source = fs::canonicalize(&stores.directory)
            .map_err(|error| io_error("读取原目录", &stores.directory, error))?;
        let target = fs::canonicalize(destination)
            .map_err(|error| io_error("读取目标目录", destination, error))?;
        if !target.is_dir() || target.to_str().is_none() {
            return Err(UiError::new(
                "storage_path",
                "请选择有效的文件夹，路径需为 Unicode 文本",
            ));
        }
        if source == target {
            return Ok(StorageView {
                directory: target.to_string_lossy().into_owned(),
                available: true,
            });
        }
        if source.starts_with(&target) || target.starts_with(&source) {
            return Err(UiError::new(
                "storage_path",
                "新旧数据目录不能互相包含，请选择其他文件夹",
            ));
        }
        for name in TREES {
            if fs::symlink_metadata(target.join(name)).is_ok() {
                return Err(UiError::new(
                    "storage_conflict",
                    "目标目录已有 replays 或 sessions，请选择其他文件夹；不会覆盖已有数据",
                ));
            }
        }
        let mut progress = MigrationProgress {
            copied_files: 0,
            total_files: 0,
            copied_bytes: 0,
            total_bytes: 0,
        };
        on_progress(progress.clone());
        let mut directories = Vec::new();
        let mut files = Vec::new();
        let mut pending: Vec<PathBuf> = TREES.iter().map(PathBuf::from).collect();
        while let Some(relative) = pending.pop() {
            migration.check()?;
            let path = source.join(&relative);
            let metadata =
                fs::symlink_metadata(&path).map_err(|error| io_error("读取数据", &path, error))?;
            if metadata.is_dir() {
                for entry in
                    fs::read_dir(&path).map_err(|error| io_error("读取目录", &path, error))?
                {
                    let entry = entry.map_err(|error| io_error("读取目录", &path, error))?;
                    pending.push(relative.join(entry.file_name()));
                }
                directories.push(relative);
            } else if metadata.is_file() {
                progress.total_bytes = progress
                    .total_bytes
                    .checked_add(metadata.len())
                    .ok_or_else(|| UiError::new("storage_size", "数据总量超出支持范围"))?;
                files.push(relative);
            } else {
                return Err(UiError::new(
                    "storage_path",
                    format!("数据目录包含链接或特殊文件，无法迁移：{}", path.display()),
                ));
            }
        }
        progress.total_files = files.len() as u64;
        on_progress(progress.clone());
        let stage = target.join(unique("kyoku-migration"));
        fs::create_dir(&stage).map_err(|error| io_error("创建迁移目录", &stage, error))?;
        let mut installed = Vec::new();
        let result = (|| {
            for directory in directories {
                let path = stage.join(directory);
                fs::create_dir_all(&path)
                    .map_err(|error| io_error("创建数据目录", &path, error))?;
            }
            let mut buffer = [0u8; 64 * 1024];
            for relative in files {
                migration.check()?;
                let from = source.join(&relative);
                let to = stage.join(&relative);
                let mut input =
                    File::open(&from).map_err(|error| io_error("读取原文件", &from, error))?;
                let mut output = new_file(&to)?;
                let mut reported = progress.copied_bytes;
                loop {
                    migration.check()?;
                    let count = input
                        .read(&mut buffer)
                        .map_err(|error| io_error("读取原文件", &from, error))?;
                    if count == 0 {
                        break;
                    }
                    output
                        .write_all(&buffer[..count])
                        .map_err(|error| io_error("复制文件", &to, error))?;
                    progress.copied_bytes += count as u64;
                    if progress.copied_bytes - reported >= 1024 * 1024 {
                        on_progress(progress.clone());
                        reported = progress.copied_bytes;
                    }
                }
                output
                    .sync_all()
                    .map_err(|error| io_error("保存文件", &to, error))?;
                progress.copied_files += 1;
                on_progress(progress.clone());
            }
            migration.check()?;
            for name in TREES {
                let path = target.join(name);
                // 只接管本次创建的两个目录；其他文件不参与迁移。
                if path
                    .try_exists()
                    .map_err(|error| io_error("检查目标目录", &path, error))?
                {
                    return Err(UiError::new(
                        "storage_conflict",
                        "迁移期间目标目录发生变化，已停止迁移",
                    ));
                }
                fs::rename(stage.join(name), &path)
                    .map_err(|error| io_error("完成目录复制", &path, error))?;
                installed.push(path);
            }
            migration.check()?;
            self.save_location(&target)?;
            // 配置已提交后不再接受取消，确保本次运行与下次启动使用同一位置。
            *stores = Stores::new(target.clone());
            Ok(StorageView {
                directory: target.to_string_lossy().into_owned(),
                available: true,
            })
        })();
        if result.is_err() {
            for path in installed {
                let _ = fs::remove_dir_all(path);
            }
        }
        let _ = fs::remove_dir_all(stage);
        result
    }

    fn save_location(&self, directory: &Path) -> Result<(), UiError> {
        fs::create_dir_all(&self.config_directory)
            .map_err(|error| io_error("创建配置目录", &self.config_directory, error))?;
        let path = self.config_directory.join("storage.json");
        let temporary = self.config_directory.join(unique("storage"));
        let bytes = serde_json::to_vec(&Location {
            directory: directory.into(),
        })
        .map_err(|_| UiError::new("storage_config", "无法保存数据位置"))?;
        let result = (|| {
            let mut file = new_file(&temporary)?;
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|error| io_error("保存数据位置", &temporary, error))?;
            drop(file);
            fs::rename(&temporary, &path).map_err(|error| io_error("更新数据位置", &path, error))
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }
}

#[cfg(test)]
mod tests;
