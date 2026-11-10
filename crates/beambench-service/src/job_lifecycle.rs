use beambench_common::machine::ContinuousFrameProgress;
use uuid::Uuid;

use crate::ops::machine::PreparedFrame;
use crate::power::SleepGuard;

/// Installation and cleanup are protected by the job lock. The finish request
/// only needs the resources mutex to disarm the next lap.
#[derive(Default)]
pub(crate) struct JobResources {
    pub(crate) _sleep: Option<SleepGuard>,
    pub(crate) repeat: Option<PreparedFrame>,
    pub(crate) continuous_frame_id: Option<Uuid>,
    pub(crate) is_frame: bool,
    /// Emergency-stop generation when the job started. A repeated frame never
    /// restarts after a later emergency stop.
    pub(crate) estop_generation: u64,
}
impl JobResources {
    pub(crate) fn continuous_frame(&self) -> Option<ContinuousFrameProgress> {
        self.continuous_frame_id.map(|id| ContinuousFrameProgress {
            id,
            finishing: self.repeat.is_none(),
        })
    }
}
