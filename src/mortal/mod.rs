//! 本地 Mortal V4 推理适配。独立进程维护模型状态，领域层不依赖 Python。

mod protocol;

use std::error::Error;
use std::fmt;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use convlog::Event;
use serde::Deserialize;

use crate::mahjong::player_index::PlayerIndex;
pub use protocol::{Action, Candidate, Decision, KanCandidate, ProtocolError};

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;

/// 本地引擎所需的 Python、官方 Mortal 源码目录及模型文件。
pub struct MortalConfig<'a> {
    pub python: &'a Path,
    /// 包含 `mortal/model.py` 与已编译 `libriichi` 扩展的仓库根目录。
    pub runtime: &'a Path,
    pub checkpoint: &'a Path,
}

/// 实际加载的模型身份；社区权重不能仅凭架构版本视作官网模型。
#[derive(Debug, Deserialize)]
pub struct ModelInfo {
    pub version: u8,
    pub tag: String,
    pub sha256: String,
}

/// 推理进程或协议失败。当前事件编号由调用方附加。
#[derive(Debug)]
pub enum MortalError {
    Spawn(io::Error),
    Write(io::Error),
    Read(io::Error),
    Json(serde_json::Error),
    Protocol(ProtocolError),
    UnexpectedEof,
    Timeout,
    Closed,
    Exit(ExitStatus),
}

impl fmt::Display for MortalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(e) => write!(f, "cannot start Mortal: {e}"),
            Self::Write(e) => write!(f, "cannot write to Mortal: {e}"),
            Self::Read(e) => write!(f, "cannot read from Mortal: {e}"),
            Self::Json(e) => write!(f, "invalid Mortal JSON: {e}"),
            Self::Protocol(e) => write!(f, "invalid Mortal response: {e}"),
            Self::UnexpectedEof => {
                write!(f, "Mortal closed stdout unexpectedly; see engine stderr")
            }
            Self::Timeout => write!(f, "Mortal did not respond within 60 seconds"),
            Self::Closed => write!(f, "Mortal session is closed after an earlier failure"),
            Self::Exit(status) => write!(f, "Mortal exited with {status}"),
        }
    }
}

impl Error for MortalError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Spawn(e) | Self::Write(e) | Self::Read(e) => Some(e),
            Self::Json(e) => Some(e),
            Self::Protocol(e) => Some(e),
            _ => None,
        }
    }
}

/// 一位玩家的一次牌谱推理会话。按顺序输入事件，失败后不可继续复用。
pub struct Mortal {
    child: Child,
    stdin: Option<ChildStdin>,
    output: Receiver<io::Result<String>>,
    player: PlayerIndex,
    model: ModelInfo,
    failed: bool,
}

impl Mortal {
    /// 加载模型并等待就绪；启动失败、无响应或协议不兼容均返回错误。
    pub fn start(config: &MortalConfig<'_>, player: PlayerIndex) -> Result<Self, MortalError> {
        let mut command = Command::new(config.python);
        command
            // 独立运行，避免用户的 Python 环境变量或包影响内置引擎；不写入应用资源。
            .args(["-I", "-B", "-u", "-c", include_str!("bridge.py")])
            .arg(config.runtime)
            .arg(config.checkpoint)
            .arg(player.get_id().to_string());
        Self::spawn(command, player)
    }

    fn spawn(mut command: Command, player: PlayerIndex) -> Result<Self, MortalError> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(MortalError::Spawn)?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("stdout was configured as piped");
        let (sender, output) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                let result = reader
                    .by_ref()
                    .take(MAX_RESPONSE_BYTES + 1)
                    .read_line(&mut line);
                let message = match result {
                    Ok(0) => break,
                    Ok(_) if line.len() as u64 > MAX_RESPONSE_BYTES => Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Mortal response exceeds 1 MiB",
                    )),
                    Ok(_) => Ok(line),
                    Err(error) => Err(error),
                };
                let failed = message.is_err();
                if sender.send(message).is_err() || failed {
                    break;
                }
            }
        });
        let mut engine = Self {
            child,
            stdin,
            output,
            player,
            model: ModelInfo {
                version: 0,
                tag: String::new(),
                sha256: String::new(),
            },
            failed: false,
        };
        engine.model = serde_json::from_str(&engine.read_line()?).map_err(MortalError::Json)?;
        if engine.model.version != 4
            || engine.model.sha256.len() != 64
            || !engine.model.sha256.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(MortalError::Protocol(ProtocolError::InvalidModelInfo));
        }
        Ok(engine)
    }

    /// 返回实际加载的模型信息。
    pub fn model(&self) -> &ModelInfo {
        &self.model
    }

    /// 应用一个真实牌谱事件，返回该事件之后的模型判断。
    ///
    /// 对手起手牌和摸牌会遮蔽；无决策机会时返回 `None`，可行动但选择跳过时
    /// 返回动作是 `Event::None` 的 `Some(Decision)`。不自动执行推荐动作。
    pub fn react(&mut self, event: &Event) -> Result<Option<Decision>, MortalError> {
        if self.failed {
            return Err(MortalError::Closed);
        }
        let result = self.exchange(event);
        if result.is_err() {
            self.failed = true;
            self.stdin.take();
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        result
    }

    fn exchange(&mut self, event: &Event) -> Result<Option<Decision>, MortalError> {
        let message = protocol::visible_event(event, self.player).map_err(MortalError::Json)?;
        let stdin = self.stdin.as_mut().ok_or(MortalError::Closed)?;
        writeln!(stdin, "{message}").map_err(MortalError::Write)?;
        stdin.flush().map_err(MortalError::Write)?;
        protocol::decision(&self.read_line()?, self.player)
    }

    fn read_line(&self) -> Result<String, MortalError> {
        match self.output.recv_timeout(RESPONSE_TIMEOUT) {
            Ok(line) => line.map_err(MortalError::Read),
            Err(RecvTimeoutError::Timeout) => Err(MortalError::Timeout),
            Err(RecvTimeoutError::Disconnected) => Err(MortalError::UnexpectedEof),
        }
    }

    /// 关闭输入并检查引擎退出状态；也可在牌谱中途完成单局面查询后调用。
    pub fn finish(mut self) -> Result<(), MortalError> {
        if self.failed {
            return Err(MortalError::Closed);
        }
        self.stdin.take();
        let deadline = Instant::now() + RESPONSE_TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().map_err(MortalError::Read)? {
                return if status.success() {
                    Ok(())
                } else {
                    Err(MortalError::Exit(status))
                };
            }
            if Instant::now() >= deadline {
                return Err(MortalError::Timeout);
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Mortal {
    fn drop(&mut self) {
        self.stdin.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(all(test, unix))]
mod tests;
