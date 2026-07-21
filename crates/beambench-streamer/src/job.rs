//! Job controller — orchestrates the lifecycle of a streaming job.

use crate::engine::StreamingEngine;
use crate::error::StreamerError;
use crate::progress::ProgressTracker;
use beambench_common::ConsoleEntry;
use beambench_common::machine::{JobProgress, JobProgressBucket, JobState, MachineRunState};
use beambench_grbl::{GcodeConfig, GrblSession, generate_gcode};
use beambench_planner::{ExecutionPlan, PlanSegment};
use std::time::{Duration, Instant};

/// How often to query controller status during a job. Keeps the UI's
/// machine state live while streaming and gives the completion gate fresh
/// reports to act on.
const STATUS_POLL_INTERVAL: Duration = Duration::from_millis(200);
const IDLE_ACK_DESYNC_REPORT_LIMIT: u8 = 5;

/// Orchestrates a complete job lifecycle: prepare → start → tick → complete.
pub struct JobController {
    engine: StreamingEngine,
    progress: ProgressTracker,
    last_status_poll: Option<Instant>,
    /// Status-report count when all lines became acknowledged. Completion
    /// requires a report RECEIVED AFTER this point: `last_status` may hold a
    /// stale pre-job Idle, and trusting it would mark jobs complete while
    /// the machine is still executing buffered motion.
    awaiting_idle_baseline: Option<u64>,
    /// Status-report count at job start. Used to distinguish fresh job-time
    /// Idle reports from the pre-job Idle snapshot held by the session.
    job_start_status_baseline: u64,
    /// Number of fresh Idle reports observed while bytes are still in flight.
    /// This catches a serial/ack desync where GRBL has drained the job window
    /// and gone Idle but Beam Bench is still waiting for missing `ok`s.
    idle_ack_desync_reports: u8,
    last_idle_ack_desync_status_count: u64,
}

impl JobController {
    /// Prepare a job from an execution plan.
    pub fn prepare(plan: &ExecutionPlan, config: &GcodeConfig) -> Result<Self, StreamerError> {
        let commands = generate_gcode(plan, config)?;
        let total = commands.len();
        let engine = StreamingEngine::new_with_transfer_mode(commands, config.transfer_mode);
        let progress = ProgressTracker::with_buckets(total, build_progress_buckets(plan));

        Ok(Self {
            engine,
            progress,
            last_status_poll: None,
            awaiting_idle_baseline: None,
            job_start_status_baseline: 0,
            idle_ack_desync_reports: 0,
            last_idle_ack_desync_status_count: 0,
        })
    }

    /// Start the job.
    pub fn start(&mut self, session: &mut GrblSession) -> Result<(), StreamerError> {
        session.start_running()?;
        self.progress.set_state(JobState::Running);
        self.awaiting_idle_baseline = None;
        self.job_start_status_baseline = session.status_report_count();
        self.idle_ack_desync_reports = 0;
        self.last_idle_ack_desync_status_count = self.job_start_status_baseline;
        // Send initial batch
        self.engine.send_tick(session, &mut self.progress)?;
        Ok(())
    }

    /// Process one tick: handle responses and send more commands.
    pub fn tick(&mut self, session: &mut GrblSession) -> Result<(), StreamerError> {
        // Poll for responses
        let responses = session.poll()?;
        for response in &responses {
            self.engine.handle_response(response, &mut self.progress)?;
        }

        // Send more if possible
        self.engine.send_tick(session, &mut self.progress)?;

        // Keep machine status live throughout the job: nothing else queries
        // status while a job is active, so without this the panel freezes on
        // the pre-job snapshot and completion has nothing fresh to act on.
        if self
            .last_status_poll
            .is_none_or(|at| at.elapsed() >= STATUS_POLL_INTERVAL)
        {
            session.poll_status()?;
            self.last_status_poll = Some(Instant::now());
        }

        self.fail_if_idle_with_unacknowledged_bytes(session)?;

        // Check completion. A GRBL `ok` only means the line was accepted into
        // the controller's buffer — motion (including the final M5) can lag by
        // many seconds, so completion requires an Idle status RECEIVED AFTER
        // the last acknowledgment (last_status alone may be a stale pre-job
        // Idle, which would complete the job while the laser is still moving).
        if self.engine.all_acknowledged() && !self.engine.is_failed() && !self.engine.is_cancelled()
        {
            let baseline = *self
                .awaiting_idle_baseline
                .get_or_insert_with(|| session.status_report_count());
            let fresh_status_seen = session.status_report_count() > baseline;
            if fresh_status_seen && session.last_status().run_state == MachineRunState::Idle {
                self.progress.set_state(JobState::Completed);
                session.stop()?;
            }
        }

        Ok(())
    }

    fn fail_if_idle_with_unacknowledged_bytes(
        &mut self,
        session: &mut GrblSession,
    ) -> Result<(), StreamerError> {
        let status_count = session.status_report_count();
        let fresh_job_status_seen = status_count > self.job_start_status_baseline;
        let idle_with_unacknowledged_bytes = fresh_job_status_seen
            && !self.engine.all_acknowledged()
            && self.engine.bytes_in_flight() > 0
            && session.last_status().run_state == MachineRunState::Idle;

        if !idle_with_unacknowledged_bytes {
            self.idle_ack_desync_reports = 0;
            self.last_idle_ack_desync_status_count = status_count;
            return Ok(());
        }

        if status_count != self.last_idle_ack_desync_status_count {
            self.idle_ack_desync_reports = self.idle_ack_desync_reports.saturating_add(1);
            self.last_idle_ack_desync_status_count = status_count;
        }

        if self.idle_ack_desync_reports < IDLE_ACK_DESYNC_REPORT_LIMIT {
            return Ok(());
        }

        let message = format!(
            "Controller reported Idle while {} bytes were still waiting for acknowledgement. The serial stream is desynchronized, so the job was stopped instead of hanging.",
            self.engine.bytes_in_flight()
        );
        self.engine.fail(message.clone(), &mut self.progress);
        let _ = session.stop();
        Err(StreamerError::JobFailed(message))
    }

    /// Pause the job.
    pub fn pause(&mut self, session: &mut GrblSession) -> Result<(), StreamerError> {
        self.engine.pause();
        session.feed_hold()?;
        session.pause()?;
        self.progress.set_state(JobState::Paused);
        Ok(())
    }

    /// Resume the job.
    pub fn resume(&mut self, session: &mut GrblSession) -> Result<(), StreamerError> {
        self.engine.resume();
        session.cycle_start()?;
        session.resume()?;
        self.progress.set_state(JobState::Running);
        Ok(())
    }

    /// Cancel the job.
    pub fn cancel(&mut self, session: &mut GrblSession) -> Result<(), StreamerError> {
        self.engine.cancel();
        session.soft_reset()?;
        session.send_command("M5")?;
        self.progress.set_state(JobState::Cancelled);
        Ok(())
    }

    /// Get a snapshot of current progress.
    pub fn progress(&self) -> JobProgress {
        self.progress.snapshot()
    }

    /// Get job-stream console entries (newest first, up to limit).
    pub fn get_console_entries(&self, limit: usize) -> Vec<ConsoleEntry> {
        self.engine.get_console_entries(limit)
    }

    /// Check if the job is complete.
    pub fn is_complete(&self) -> bool {
        self.progress.state() == JobState::Completed
    }

    /// Check if the job failed.
    pub fn is_failed(&self) -> bool {
        self.engine.is_failed()
    }
}

fn build_progress_buckets(plan: &ExecutionPlan) -> Vec<JobProgressBucket> {
    let mut buckets = Vec::<JobProgressBucket>::new();
    for segment in &plan.segments {
        let (layer_id, cut_entry_id) = match segment {
            PlanSegment::Vector {
                layer_id,
                cut_entry_id,
                ..
            }
            | PlanSegment::Raster {
                layer_id,
                cut_entry_id,
                ..
            } if !cut_entry_id.is_empty() => (layer_id, cut_entry_id),
            _ => continue,
        };

        if let Some(existing) = buckets
            .iter_mut()
            .find(|bucket| bucket.layer_id == *layer_id && bucket.cut_entry_id == *cut_entry_id)
        {
            existing.segment_count += 1;
        } else {
            buckets.push(JobProgressBucket {
                layer_id: layer_id.clone(),
                cut_entry_id: cut_entry_id.clone(),
                segment_count: 1,
            });
        }
    }
    buckets
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::geometry::{Bounds, Point2D};
    use beambench_planner::{ExecutionPlan, PlanSegment};
    use beambench_serial::MockSerialTransport;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_plan() -> ExecutionPlan {
        ExecutionPlan {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            revision_hash: "test".to_string(),
            created_at: Utc::now(),
            bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 100.0)),
            total_distance_mm: 100.0,
            estimated_duration_secs: 10.0,
            segments: vec![
                PlanSegment::Travel {
                    start: Point2D::new(0.0, 0.0),
                    end: Point2D::new(10.0, 0.0),
                },
                PlanSegment::Vector {
                    polyline: vec![Point2D::new(10.0, 0.0), Point2D::new(20.0, 0.0)],
                    closed: false,
                    power_percent: 50.0,
                    speed_mm_min: 1000.0,
                    layer_id: "l1".to_string(),
                    cut_entry_id: "entry-1".to_string(),
                    perforation_enabled: false,
                    perforation_on_ms: 0.0,
                    perforation_off_ms: 0.0,
                    source_object_id: None,
                    source_subpath_index: None,
                },
            ],
            layer_order: vec!["l1".to_string()],
            failed_entries: vec![],
            warnings: vec![],
        }
    }

    fn make_ready_session() -> GrblSession {
        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("Grbl 1.1h");
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        session.mark_ready().unwrap();
        session
    }

    #[test]
    fn prepare_creates_job() {
        let plan = make_plan();
        let config = GcodeConfig::default();
        let job = JobController::prepare(&plan, &config).unwrap();
        let snap = job.progress();
        assert!(snap.total_lines > 0);
        assert_eq!(snap.state, JobState::Preparing);
    }

    #[test]
    fn start_transitions_to_running() {
        let plan = make_plan();
        let config = GcodeConfig::default();
        let mut job = JobController::prepare(&plan, &config).unwrap();
        let mut session = make_ready_session();

        job.start(&mut session).unwrap();
        assert_eq!(job.progress().state, JobState::Running);
        assert!(job.progress().sent_lines > 0);
    }

    #[test]
    fn pause_and_resume() {
        let plan = make_plan();
        let config = GcodeConfig::default();
        let mut job = JobController::prepare(&plan, &config).unwrap();
        let mut session = make_ready_session();

        job.start(&mut session).unwrap();
        job.pause(&mut session).unwrap();
        assert_eq!(job.progress().state, JobState::Paused);

        job.resume(&mut session).unwrap();
        assert_eq!(job.progress().state, JobState::Running);
    }

    #[test]
    fn cancel_sets_cancelled() {
        let plan = make_plan();
        let config = GcodeConfig::default();
        let mut job = JobController::prepare(&plan, &config).unwrap();
        let mut session = make_ready_session();

        job.start(&mut session).unwrap();
        job.cancel(&mut session).unwrap();
        assert_eq!(job.progress().state, JobState::Cancelled);
        assert_eq!(session.get_console_log(1)[0].content, "M5");
    }

    #[test]
    fn completion_waits_for_machine_idle() {
        let plan = make_plan();
        let config = GcodeConfig::default();
        let mut job = JobController::prepare(&plan, &config).unwrap();

        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("Grbl 1.1h");
        let mock = transport.handle();
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        session.mark_ready().unwrap();

        job.start(&mut session).unwrap();
        let total = job.progress().total_lines;

        // Ack every line while the machine keeps reporting Run.
        let mut guard = 0;
        loop {
            let snap = job.progress();
            if snap.acknowledged_lines >= total {
                break;
            }
            for _ in 0..(snap.sent_lines - snap.acknowledged_lines) {
                mock.enqueue_response("ok");
            }
            mock.enqueue_response("<Run|MPos:10.000,20.000,0.000|FS:1000,500>");
            job.tick(&mut session).unwrap();
            guard += 1;
            assert!(guard < 100, "job never drained");
        }

        // Every line is acknowledged, but GRBL is still executing buffered
        // motion (status Run) — the job must not report Completed yet.
        job.tick(&mut session).unwrap();
        assert_eq!(job.progress().state, JobState::Running);

        // A fresh Idle report finally completes the job.
        mock.enqueue_response("<Idle|MPos:10.000,20.000,0.000|FS:0,0>");
        job.tick(&mut session).unwrap();
        assert_eq!(job.progress().state, JobState::Completed);
    }

    #[test]
    fn completion_rejects_stale_pre_job_idle_status() {
        // last_status defaults to Idle before any report arrives. With all
        // lines acknowledged but NO status received after the acks, the job
        // must stay Running — trusting the stale Idle completed jobs while
        // the machine was still cutting (field report: ACMER S1).
        let plan = make_plan();
        let config = GcodeConfig::default();
        let mut job = JobController::prepare(&plan, &config).unwrap();

        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("Grbl 1.1h");
        let mock = transport.handle();
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        session.mark_ready().unwrap();

        job.start(&mut session).unwrap();
        let total = job.progress().total_lines;

        let mut guard = 0;
        while job.progress().acknowledged_lines < total {
            let snap = job.progress();
            for _ in 0..(snap.sent_lines - snap.acknowledged_lines) {
                mock.enqueue_response("ok");
            }
            job.tick(&mut session).unwrap();
            guard += 1;
            assert!(guard < 100, "job never drained");
        }

        // All acked, no fresh status: several ticks must NOT complete.
        for _ in 0..5 {
            job.tick(&mut session).unwrap();
        }
        assert_eq!(
            job.progress().state,
            JobState::Running,
            "stale pre-job Idle must not complete the job"
        );

        // A fresh Idle report completes it.
        mock.enqueue_response("<Idle|MPos:0.000,0.000,0.000|FS:0,0>");
        job.tick(&mut session).unwrap();
        assert_eq!(job.progress().state, JobState::Completed);
    }

    #[test]
    fn repeated_fresh_idle_with_unacknowledged_bytes_fails_job() {
        let plan = make_plan();
        let config = GcodeConfig::default();
        let mut job = JobController::prepare(&plan, &config).unwrap();

        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("Grbl 1.1h");
        let mock = transport.handle();
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        session.mark_ready().unwrap();

        job.start(&mut session).unwrap();
        assert!(job.progress().sent_lines > 0);
        assert_eq!(job.progress().acknowledged_lines, 0);

        for _ in 0..(IDLE_ACK_DESYNC_REPORT_LIMIT - 1) {
            mock.enqueue_response("<Idle|MPos:24.400,94.875,0.000|FS:0,0>");
            job.tick(&mut session).unwrap();
            assert_eq!(job.progress().state, JobState::Running);
        }

        mock.enqueue_response("<Idle|MPos:24.400,94.875,0.000|FS:0,0>");
        let err = job.tick(&mut session).unwrap_err();
        assert!(
            err.to_string().contains("serial stream is desynchronized"),
            "unexpected error: {err}"
        );
        assert_eq!(job.progress().state, JobState::Failed);
    }

    #[test]
    fn prepare_includes_distinct_cut_entry_buckets() {
        let mut plan = make_plan();
        plan.segments.push(PlanSegment::Vector {
            polyline: vec![Point2D::new(20.0, 0.0), Point2D::new(30.0, 0.0)],
            closed: false,
            power_percent: 60.0,
            speed_mm_min: 900.0,
            layer_id: "l1".to_string(),
            cut_entry_id: "entry-2".to_string(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        });
        let config = GcodeConfig::default();
        let job = JobController::prepare(&plan, &config).unwrap();
        let progress = job.progress();
        assert_eq!(progress.buckets.len(), 2);
        assert_eq!(progress.buckets[0].cut_entry_id, "entry-1");
        assert_eq!(progress.buckets[1].cut_entry_id, "entry-2");
    }
}
