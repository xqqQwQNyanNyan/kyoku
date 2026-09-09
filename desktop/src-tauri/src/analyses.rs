use crate::{UiError, lock};
use kyoku::review::ReviewControl;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

/// 按牌谱和请求定位取消，旧请求不能取消后续重试。
#[derive(Default)]
pub(crate) struct Analyses {
    active: Mutex<HashMap<u64, (String, ReviewControl)>>,
}

pub(crate) struct RunningAnalysis {
    analyses: Arc<Analyses>,
    id: u64,
}

impl Analyses {
    pub fn begin(
        self: &Arc<Self>,
        id: u64,
        request_id: &str,
        control: ReviewControl,
    ) -> Result<RunningAnalysis, UiError> {
        if !crate::sessions::valid_id(request_id) {
            return Err(UiError::new("analysis", "分析请求编号无效"));
        }
        let mut active = lock(&self.active)?;
        if active.contains_key(&id) {
            return Err(UiError::new("busy", "该牌谱正在分析，请稍候"));
        }
        active.insert(id, (request_id.into(), control));
        Ok(RunningAnalysis {
            analyses: self.clone(),
            id,
        })
    }

    pub fn cancel(&self, id: u64, request_id: &str) -> Result<(), UiError> {
        if let Some((current, control)) = lock(&self.active)?.get(&id)
            && current == request_id
        {
            control.cancel();
        }
        Ok(())
    }
}

impl Drop for RunningAnalysis {
    fn drop(&mut self) {
        if let Ok(mut active) = self.analyses.active.lock() {
            active.remove(&self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kyoku::{
        mahjong::player_index::PlayerIndex,
        mortal::MortalConfig,
        review::{ReviewError, review_game_with_control},
    };
    use std::path::Path;

    fn is_cancelled(control: &ReviewControl) -> bool {
        matches!(
            review_game_with_control(
                &[],
                PlayerIndex::try_from(0u8).unwrap(),
                &MortalConfig {
                    python: Path::new("unused"),
                    runtime: Path::new("unused"),
                    checkpoint: Path::new("unused"),
                },
                control
            ),
            Err(ReviewError::Cancelled)
        )
    }

    #[test]
    fn cancellation_is_scoped_and_late_requests_cannot_cancel_a_retry() {
        let analyses = Arc::new(Analyses::default());
        let first = ReviewControl::default();
        let running = analyses.begin(1, "first", first.clone()).unwrap();
        assert!(
            analyses
                .begin(1, "duplicate", ReviewControl::default())
                .is_err()
        );
        analyses.cancel(2, "first").unwrap();
        analyses.cancel(1, "different").unwrap();
        assert!(!is_cancelled(&first));
        analyses.cancel(1, "first").unwrap();
        assert!(is_cancelled(&first));
        drop(running);
        let retry = ReviewControl::default();
        let _running = analyses.begin(1, "retry", retry.clone()).unwrap();
        analyses.cancel(1, "first").unwrap();
        assert!(!is_cancelled(&retry));
    }
}
