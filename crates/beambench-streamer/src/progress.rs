//! Job progress tracking.

use beambench_common::machine::{JobProgress, JobProgressBucket, JobState};
use std::time::Instant;

/// Tracks progress of a streaming job.
pub struct ProgressTracker {
    state: JobState,
    total_lines: usize,
    queued_lines: usize,
    sent_lines: usize,
    acknowledged_lines: usize,
    start_time: Option<Instant>,
    buffer_fill_bytes: usize,
    error_message: Option<String>,
    buckets: Vec<JobProgressBucket>,
}

impl ProgressTracker {
    pub fn new(total_lines: usize) -> Self {
        Self::with_buckets(total_lines, Vec::new())
    }

    pub fn with_buckets(total_lines: usize, buckets: Vec<JobProgressBucket>) -> Self {
        Self {
            state: JobState::Preparing,
            total_lines,
            queued_lines: total_lines,
            sent_lines: 0,
            acknowledged_lines: 0,
            start_time: None,
            buffer_fill_bytes: 0,
            error_message: None,
            buckets,
        }
    }

    pub fn set_state(&mut self, state: JobState) {
        self.state = state;
        if state != JobState::Failed {
            self.error_message = None;
        }
        if state == JobState::Running && self.start_time.is_none() {
            self.start_time = Some(Instant::now());
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
        self.state = JobState::Failed;
        self.error_message = Some(message.into());
    }

    pub fn is_complete(&self) -> bool {
        self.acknowledged_lines >= self.total_lines
    }

    /// Take a snapshot of current progress.
    pub fn snapshot(&self) -> JobProgress {
        let elapsed = self
            .start_time
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(0.0);

        let estimated_remaining = if self.acknowledged_lines > 0 && self.total_lines > 0 {
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
}
