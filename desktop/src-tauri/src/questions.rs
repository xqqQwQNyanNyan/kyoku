use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use kyoku::agent::QuestionControl;

use crate::{UiError, lock};

/// 以会话和本轮编号定位停止操作，迟到的停止请求不能影响下一轮。
#[derive(Default)]
pub(crate) struct Questions {
    active: Mutex<HashMap<String, (String, QuestionControl)>>,
}

pub(crate) struct RunningQuestion {
    questions: Arc<Questions>,
    id: String,
    pub control: QuestionControl,
}

impl Questions {
    pub fn begin(
        self: &Arc<Self>,
        id: &str,
        request_id: &str,
        control: QuestionControl,
    ) -> Result<RunningQuestion, UiError> {
        if !crate::sessions::valid_id(id) || !crate::sessions::valid_id(request_id) {
            return Err(UiError::new("question", "问答编号无效"));
        }
        let mut active = lock(&self.active)?;
        if active.contains_key(id) {
            return Err(UiError::new("busy", "此会话仍在处理，请稍后重试"));
        }
        active.insert(id.into(), (request_id.into(), control.clone()));
        Ok(RunningQuestion {
            questions: self.clone(),
            id: id.into(),
            control,
        })
    }

    pub fn cancel(&self, id: &str, request_id: &str) -> Result<(), UiError> {
        if !crate::sessions::valid_id(id) || !crate::sessions::valid_id(request_id) {
            return Err(UiError::new("question", "问答编号无效"));
        }
        if let Some((current, control)) = lock(&self.active)?.get(id)
            && current == request_id
        {
            control.cancel();
        }
        Ok(())
    }
}

impl Drop for RunningQuestion {
    fn drop(&mut self) {
        if let Ok(mut active) = self.questions.active.lock() {
            active.remove(&self.id);
        }
    }
}
