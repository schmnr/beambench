use crate::ops::machine::PreparedFrame;
use crate::power::SleepGuard;

/// Protected by the job lock as well as its own mutex. A repeated frame keeps
/// the same prepared motion and sleep assertion between passes.
#[derive(Default)]
pub(crate) struct JobResources {
    pub(crate) _sleep: Option<SleepGuard>,
    pub(crate) repeat: Option<PreparedFrame>,
    /// Emergency-stop generation when the job started. A repeated frame never
    /// restarts after a later emergency stop.
    pub(crate) estop_generation: u64,
}
