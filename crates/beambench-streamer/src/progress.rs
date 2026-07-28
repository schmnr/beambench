//! Job progress tracking.

use beambench_common::machine::{JobProgress, JobProgressBucket, JobState};
use std::time::{Duration, Instant};

/// Tracks progress of a streaming job.
pub struct ProgressTracker {
    state: JobState,
    total_lines: usize,
    queued_lines: usize,
    sent_lines: usize,
    acknowledged_lines: usize,
    running_since: Option<Instant>,
    active_elapsed: Duration,
    planned_duration_secs: Option<f64>,
    buffer_fill_bytes: usize,
    error_message: Option<String>,
    buckets: Vec<JobProgressBucket>,
}

impl ProgressTracker {
    pub fn new(total_lines: usize) -> Self {
        Self::with_buckets(total_lines, Vec::new())
    }

    pub fn with_buckets(total_lines: usize, buckets: Vec<JobProgressBucket>) -> Self {
        Self::with_buckets_and_duration(total_lines, buckets, None)
    }

    pub fn with_buckets_and_duration(
        total_lines: usize,
        buckets: Vec<JobProgressBucket>,
        planned_duration_secs: Option<f64>,
    ) -> Self {
        Self {
            state: JobState::Preparing,
            total_lines,
            queued_lines: total_lines,
            sent_lines: 0,
            acknowledged_lines: 0,
            running_since: None,
            active_elapsed: Duration::ZERO,
            planned_duration_secs: planned_duration_secs.filter(|value| *value > 0.0),
            buffer_fill_bytes: 0,
            error_message: None,
            buckets,
        }
    }

    pub fn set_state(&mut self, state: JobState) {
        if self.state == JobState::Running && state != JobState::Running {
            if let Some(started) = self.running_since.take() {
                self.active_elapsed += started.elapsed();
            }
        } else if self.state != JobState::Running && state == JobState::Running {
            self.running_since = Some(Instant::now());
        }
        self.state = state;
        if state != JobState::Failed {
            self.error_message = None;
        }
    }

    pub fn state(&self) -> JobState {
        self.state
    }

    pub fn record_sent(&mut self) {
        self.sent_lines += 1;
        self.queued_lines = self.total_lines.saturating_sub(self.sent_lines);
    }

    pub fn record_acknowledged(&mut self) {
        self.acknowledged_lines += 1;
    }

    pub fn set_buffer_fill(&mut self, bytes: usize) {
        self.buffer_fill_bytes = bytes;
    }

    pub fn set_failed(&mut self, message: impl Into<String>) {
        self.set_state(JobState::Failed);
        self.error_message = Some(message.into());
    }

    pub fn is_complete(&self) -> bool {
        self.acknowledged_lines >= self.total_lines
    }

    /// Take a snapshot of current progress.
    pub fn snapshot(&self) -> JobProgress {
        let elapsed = self.active_elapsed.as_secs_f64()
            + self
                .running_since
                .map(|started| started.elapsed().as_secs_f64())
                .unwrap_or(0.0);

        let estimated_remaining = if matches!(self.state, JobState::Running | JobState::Paused)
            && self.planned_duration_secs.is_some()
        {
            (self.planned_duration_secs.unwrap_or_default() - elapsed).max(0.0)
        } else if matches!(self.state, JobState::Running | JobState::Paused)
            && self.acknowledged_lines > 0
            && self.total_lines > 0
        {
            let rate = self.acknowledged_lines as f64 / elapsed.max(0.001);
            let remaining = self.total_lines.saturating_sub(self.acknowledged_lines) as f64;
            remaining / rate
        } else {
            0.0
        };

        JobProgress {
            state: self.state,
            total_lines: self.total_lines,
            queued_lines: self.queued_lines,
            sent_lines: self.sent_lines,
            acknowledged_lines: self.acknowledged_lines,
            elapsed_secs: elapsed,
            estimated_remaining_secs: estimated_remaining,
            buffer_fill_bytes: self.buffer_fill_bytes,
            error_message: self.error_message.clone(),
            buckets: self.buckets.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_tracker_starts_in_preparing() {
        let tracker = ProgressTracker::new(100);
        assert_eq!(tracker.state(), JobState::Preparing);
    }

    #[test]
    fn snapshot_initial_state() {
        let tracker = ProgressTracker::new(100);
        let snap = tracker.snapshot();
        assert_eq!(snap.total_lines, 100);
        assert_eq!(snap.queued_lines, 100);
        assert_eq!(snap.sent_lines, 0);
        assert_eq!(snap.acknowledged_lines, 0);
    }

    #[test]
    fn snapshot_preserves_bucket_metadata() {
        let tracker = ProgressTracker::with_buckets(
            100,
            vec![JobProgressBucket {
                layer_id: "layer-1".to_string(),
                cut_entry_id: "entry-1".to_string(),
                segment_count: 2,
            }],
        );
        let snap = tracker.snapshot();
        assert_eq!(snap.buckets.len(), 1);
        assert_eq!(snap.buckets[0].cut_entry_id, "entry-1");
    }

    #[test]
    fn record_sent_and_acknowledged() {
        let mut tracker = ProgressTracker::new(10);
        tracker.set_state(JobState::Running);
        tracker.record_sent();
        tracker.record_sent();
        tracker.record_acknowledged();

        let snap = tracker.snapshot();
        assert_eq!(snap.sent_lines, 2);
        assert_eq!(snap.acknowledged_lines, 1);
        assert_eq!(snap.queued_lines, 8);
    }

    #[test]
    fn completion_detection() {
        let mut tracker = ProgressTracker::new(2);
        assert!(!tracker.is_complete());
        tracker.record_sent();
        tracker.record_acknowledged();
        tracker.record_sent();
        tracker.record_acknowledged();
        assert!(tracker.is_complete());
    }

    #[test]
    fn buffer_fill_tracking() {
        let mut tracker = ProgressTracker::new(10);
        tracker.set_buffer_fill(64);
        assert_eq!(tracker.snapshot().buffer_fill_bytes, 64);
    }

    #[test]
    fn planned_eta_does_not_jump_when_zero_duration_lines_are_acknowledged() {
        let mut tracker = ProgressTracker::with_buckets_and_duration(1_000, Vec::new(), Some(60.0));
        tracker.set_state(JobState::Running);
        let before = tracker.snapshot().estimated_remaining_secs;
        for _ in 0..900 {
            tracker.record_acknowledged();
        }
        let after = tracker.snapshot().estimated_remaining_secs;

        assert!(
            (before - after).abs() < 0.1,
            "before={before}, after={after}"
        );
        assert!(after > 59.0);
    }

    #[test]
    fn pause_freezes_active_elapsed_and_planned_eta() {
        let mut tracker = ProgressTracker::with_buckets_and_duration(100, Vec::new(), Some(60.0));
        tracker.set_state(JobState::Running);
        std::thread::sleep(Duration::from_millis(10));
        tracker.set_state(JobState::Paused);
        let paused = tracker.snapshot();
        std::thread::sleep(Duration::from_millis(20));
        let still_paused = tracker.snapshot();

        assert!((still_paused.elapsed_secs - paused.elapsed_secs).abs() < 0.001);
        assert!(
            (still_paused.estimated_remaining_secs - paused.estimated_remaining_secs).abs() < 0.001
        );

        tracker.set_state(JobState::Running);
        std::thread::sleep(Duration::from_millis(10));
        let resumed = tracker.snapshot();
        assert!(resumed.elapsed_secs > paused.elapsed_secs);
        assert!(resumed.estimated_remaining_secs < paused.estimated_remaining_secs);
    }
}
