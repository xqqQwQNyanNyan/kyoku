use std::sync::Arc;

use serde::Serialize;
use tokio::sync::watch;

use super::AgentError;

/// 一轮问答当前正在进行的操作，不包含问题、密钥或工具结果。
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum QuestionProgress {
    /// 正在准备本轮局面与会话。
    Preparing,
    /// 正在等待本轮第几次模型请求。
    Model { request: usize },
    /// 正在执行模型选择的本地工具。
    Tool { name: String },
}

/// 单轮问答的停止信号与进度接收器；每次提问或重试必须创建新实例。
#[derive(Clone)]
pub struct QuestionControl {
    cancelled: watch::Sender<bool>,
    progress: Arc<dyn Fn(QuestionProgress) + Send + Sync>,
}

impl QuestionControl {
    /// 接收即时阶段变化；回调应尽快返回，不阻塞问答。
    pub fn new(progress: impl Fn(QuestionProgress) + Send + Sync + 'static) -> Self {
        Self {
            cancelled: watch::channel(false).0,
            progress: Arc::new(progress),
        }
    }

    /// 中断本机网络等待，并在本地工具的执行边界停止；不能撤销服务商已收到的请求。
    pub fn cancel(&self) {
        self.cancelled.send_replace(true);
    }

    pub(super) fn check(&self) -> Result<(), AgentError> {
        if *self.cancelled.borrow() {
            Err(AgentError::Cancelled)
        } else {
            Ok(())
        }
    }

    pub(super) fn report(&self, progress: QuestionProgress) -> Result<(), AgentError> {
        self.check()?;
        (self.progress)(progress);
        self.check()
    }

    pub(super) async fn cancelled(&self) {
        let mut receiver = self.cancelled.subscribe();
        while !*receiver.borrow_and_update() {
            if receiver.changed().await.is_err() {
                return;
            }
        }
    }
}

impl Default for QuestionControl {
    fn default() -> Self {
        Self::new(|_| {})
    }
}
