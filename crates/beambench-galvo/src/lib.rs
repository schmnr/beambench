use beambench_common::machine::{
    ControllerModel, JobProgress, JobState, MachinePosition, MachineRunState, MachineStatus,
    SessionState,
};
use beambench_planner::ExecutionPlan;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalvoSession {
    pub model: ControllerModel,
    pub device_path: String,
    pub session_state: SessionState,
    pub machine_status: MachineStatus,
}

#[derive(Debug, Clone)]
pub struct GalvoJob {
    progress: JobProgress,
    started_at: DateTime<Utc>,
    frame: bool,
}

impl GalvoSession {
    pub fn connect(model: ControllerModel, device_path: String) -> Self {
        Self {
            model,
            device_path,
            session_state: SessionState::Ready,
            machine_status: MachineStatus {
                run_state: MachineRunState::Idle,
                machine_position: MachinePosition::default(),
                work_position: MachinePosition::default(),
                feed_rate: 0.0,
                spindle_speed: 0.0,
                feed_override: 100,
                spindle_override: 100,
                rapid_override: 100,
                pin_states: String::new(),
            },
        }
    }

    pub fn disconnect(&mut self) {
        self.session_state = SessionState::Disconnected;
        self.machine_status.run_state = MachineRunState::Idle;
    }

    pub fn status(&self) -> MachineStatus {
        self.machine_status.clone()
    }

    pub fn frame_job(&mut self) -> GalvoJob {
        self.machine_status.run_state = MachineRunState::Run;
        GalvoJob::new(3, true)
    }

    pub fn start_job(&mut self, plan: &ExecutionPlan) -> GalvoJob {
        self.machine_status.run_state = MachineRunState::Run;
        GalvoJob::new(plan.segments.len().max(1), false)
    }
}

impl GalvoJob {
    fn new(total_lines: usize, frame: bool) -> Self {
        Self {
            progress: JobProgress {
                state: JobState::Running,
                total_lines,
                queued_lines: total_lines,
                sent_lines: 0,
                acknowledged_lines: 0,
                elapsed_secs: 0.0,
                estimated_remaining_secs: total_lines as f64,
                buffer_fill_bytes: 0,
                error_message: None,
                buckets: Vec::new(),
            },
            started_at: Utc::now(),
            frame,
        }
    }

    pub fn tick(&mut self) -> JobProgress {
        if self.progress.state == JobState::Running {
            self.progress.sent_lines =
                (self.progress.sent_lines + 1).min(self.progress.total_lines);
            self.progress.acknowledged_lines =
                (self.progress.acknowledged_lines + 1).min(self.progress.total_lines);
            self.progress.queued_lines = self
                .progress
                .total_lines
                .saturating_sub(self.progress.sent_lines);
            self.progress.elapsed_secs =
                (Utc::now() - self.started_at).num_milliseconds() as f64 / 1000.0;
            self.progress.estimated_remaining_secs =
                self.progress
                    .total_lines
                    .saturating_sub(self.progress.acknowledged_lines) as f64;
            if self.progress.acknowledged_lines >= self.progress.total_lines {
                self.progress.state = JobState::Completed;
            }
        }
        self.progress.clone()
    }

    pub fn cancel(&mut self) -> JobProgress {
        self.progress.state = JobState::Cancelled;
        self.progress.clone()
    }

    pub fn progress(&self) -> JobProgress {
        self.progress.clone()
    }

    pub fn is_frame(&self) -> bool {
        self.frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_planner::ExecutionPlan;
    use uuid::Uuid;

    fn sample_plan() -> ExecutionPlan {
        ExecutionPlan {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            revision_hash: "abc".to_string(),
            created_at: Utc::now(),
            bounds: beambench_common::Bounds::new(
                beambench_common::Point2D::new(0.0, 0.0),
                beambench_common::Point2D::new(10.0, 10.0),
            ),
            total_distance_mm: 0.0,
            estimated_duration_secs: 0.0,
            segments: Vec::new(),
            layer_order: Vec::new(),
            warnings: Vec::new(),
            failed_entries: Vec::new(),
        }
    }

    #[test]
    fn galvo_job_runs_to_completion() {
        let mut session =
            GalvoSession::connect(ControllerModel::Ezcad2, "/dev/usb/galvo0".to_string());
        let mut job = session.start_job(&sample_plan());
        let progress = job.tick();
        assert!(matches!(progress.state, JobState::Completed));
    }
}
