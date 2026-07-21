//! Ramp expansion helper for threshold raster runs.
//!
//! When a raster layer has `ramp_length_mm > 0`, threshold (binary) runs are
//! expanded into a ramp-in / constant / ramp-out power profile. This avoids
//! overburn at the stops where dwell is longer than steady-state traversal.
//!
//! Grayscale runs already carry per-pixel power values and are not ramped —
//! they already vary power along the run.
//!
//! The expansion is applied AFTER dot-width correction so the ramp sits inside
//! the already-trimmed physical bounds. Both the G-code emitter and the
//! preview distiller call `expand_threshold_run` so the preview always matches
//! the emitted output.

/// Number of discrete steps used to approximate each linear ramp side.
/// Higher values give smoother ramps at the cost of more G-code lines / more
/// preview tone strips. Six steps per side = 12 strips plus the constant
/// middle = 13 segments per run, which remains under the preview's 4000-strip
/// cap for any practical job while still looking visibly graduated.
pub const RAMP_STEPS: usize = 6;

/// A single expanded segment of a ramped threshold run.
///
/// `start_x_mm` and `end_x_mm` preserve the original run's direction of
/// motion — `end_x_mm` may be less than `start_x_mm` for right-to-left runs.
/// `power_fraction` is in `[0.0, 1.0]` where 1.0 = layer max power.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RampSegment {
    pub start_x_mm: f64,
    pub end_x_mm: f64,
    pub power_fraction: f64,
}

/// Expand a single threshold (binary) run into ramp-in / constant / ramp-out
/// segments.
///
/// `start_x_mm` / `end_x_mm` are the run's endpoints in motion order.
/// `ramp_length_mm` is the requested ramp length per side in mm; it is
/// clamped to at most `run_length / 2` so the run always has a well-defined
/// mid-line even for short runs.
///
/// If `ramp_length_mm <= 0` or the run is degenerate, returns a single
/// constant segment at full power matching the original run.
pub fn expand_threshold_run(
    start_x_mm: f64,
    end_x_mm: f64,
    ramp_length_mm: f64,
) -> Vec<RampSegment> {
    let run_length = (end_x_mm - start_x_mm).abs();
    if ramp_length_mm <= 0.0 || run_length <= 1e-9 {
        return vec![RampSegment {
            start_x_mm,
            end_x_mm,
            power_fraction: 1.0,
        }];
    }

    let dir = if end_x_mm >= start_x_mm { 1.0 } else { -1.0 };
    let effective_ramp = ramp_length_mm.min(run_length / 2.0);
    let const_length = run_length - 2.0 * effective_ramp;

    let mut segments = Vec::with_capacity(2 * RAMP_STEPS + 1);
    let step_len = effective_ramp / RAMP_STEPS as f64;

    // Ramp-in: power 0 → 1 across the first `effective_ramp` of the run.
    // Each mini-segment holds the mid-point power of its sub-interval so the
    // average power of the ramp side equals 0.5 * max_power.
    for i in 0..RAMP_STEPS {
        let t_start = i as f64 / RAMP_STEPS as f64;
        let t_end = (i + 1) as f64 / RAMP_STEPS as f64;
        let p_mid = 0.5 * (t_start + t_end);
        let sx = start_x_mm + dir * (i as f64 * step_len);
        let ex = start_x_mm + dir * ((i + 1) as f64 * step_len);
        segments.push(RampSegment {
            start_x_mm: sx,
            end_x_mm: ex,
            power_fraction: p_mid,
        });
    }

    // Constant region (may be empty when ramp_length >= run_length/2).
    let const_start = start_x_mm + dir * effective_ramp;
    let const_end = start_x_mm + dir * (effective_ramp + const_length);
    if const_length > 1e-9 {
        segments.push(RampSegment {
            start_x_mm: const_start,
            end_x_mm: const_end,
            power_fraction: 1.0,
        });
    }

    // Ramp-out: power 1 → 0 across the final `effective_ramp` of the run.
    for i in 0..RAMP_STEPS {
        let t_start = 1.0 - (i as f64 / RAMP_STEPS as f64);
        let t_end = 1.0 - ((i + 1) as f64 / RAMP_STEPS as f64);
        let p_mid = 0.5 * (t_start + t_end);
        let sx = const_end + dir * (i as f64 * step_len);
        let ex = const_end + dir * ((i + 1) as f64 * step_len);
        segments.push(RampSegment {
            start_x_mm: sx,
            end_x_mm: ex,
            power_fraction: p_mid,
        });
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn zero_ramp_returns_single_full_power_segment() {
        let segments = expand_threshold_run(0.0, 10.0, 0.0);
        assert_eq!(segments.len(), 1);
        assert!(approx(segments[0].start_x_mm, 0.0));
        assert!(approx(segments[0].end_x_mm, 10.0));
        assert!(approx(segments[0].power_fraction, 1.0));
    }

    #[test]
    fn negative_ramp_returns_single_full_power_segment() {
        let segments = expand_threshold_run(0.0, 10.0, -1.0);
        assert_eq!(segments.len(), 1);
        assert!(approx(segments[0].power_fraction, 1.0));
    }

    #[test]
    fn degenerate_run_returns_single_segment() {
        let segments = expand_threshold_run(5.0, 5.0, 1.0);
        assert_eq!(segments.len(), 1);
    }

    #[test]
    fn ltr_ramp_expands_with_const_region() {
        // 20mm run, 2mm ramp per side → 4mm ramped + 16mm constant
        let segments = expand_threshold_run(0.0, 20.0, 2.0);
        // 6 ramp-in + 1 const + 6 ramp-out = 13
        assert_eq!(segments.len(), 2 * RAMP_STEPS + 1);

        // First segment covers [0, 2/6], power ~= 1/12 (midpoint of [0, 1/6])
        assert!(approx(segments[0].start_x_mm, 0.0));
        assert!(approx(segments[0].end_x_mm, 2.0 / 6.0));
        assert!(approx(segments[0].power_fraction, 1.0 / 12.0));

        // Constant segment at index 6
        let c = segments[RAMP_STEPS];
        assert!(approx(c.start_x_mm, 2.0));
        assert!(approx(c.end_x_mm, 18.0));
        assert!(approx(c.power_fraction, 1.0));

        // Last segment ends at 20.0 with near-zero power
        let last = segments[segments.len() - 1];
        assert!(approx(last.end_x_mm, 20.0));
        assert!(approx(last.power_fraction, 1.0 / 12.0));
    }

    #[test]
    fn rtl_ramp_preserves_direction() {
        // RTL run: 20 → 0, 2mm ramp per side
        let segments = expand_threshold_run(20.0, 0.0, 2.0);
        assert_eq!(segments.len(), 2 * RAMP_STEPS + 1);

        // Ramp-in starts at 20.0 and moves toward 18.0
        assert!(approx(segments[0].start_x_mm, 20.0));
        assert!(approx(segments[0].end_x_mm, 20.0 - 2.0 / 6.0));
        assert!(approx(segments[0].power_fraction, 1.0 / 12.0));

        // Constant segment runs 18 → 2
        let c = segments[RAMP_STEPS];
        assert!(approx(c.start_x_mm, 18.0));
        assert!(approx(c.end_x_mm, 2.0));
        assert!(approx(c.power_fraction, 1.0));

        // Last segment ends at 0.0
        let last = segments[segments.len() - 1];
        assert!(approx(last.end_x_mm, 0.0));
    }

    #[test]
    fn ramp_longer_than_half_run_clamps_with_no_constant() {
        // 4mm run, 5mm requested ramp → effective ramp = 2mm, const = 0mm
        let segments = expand_threshold_run(0.0, 4.0, 5.0);
        // 6 ramp-in + 0 const + 6 ramp-out = 12
        assert_eq!(segments.len(), 2 * RAMP_STEPS);

        // First segment starts at 0, last ends at 4
        assert!(approx(segments[0].start_x_mm, 0.0));
        assert!(approx(segments[segments.len() - 1].end_x_mm, 4.0));
    }

    #[test]
    fn total_length_preserved_across_expansion() {
        let start = 3.5;
        let end = 23.5;
        let segments = expand_threshold_run(start, end, 4.0);
        let total: f64 = segments
            .iter()
            .map(|s| (s.end_x_mm - s.start_x_mm).abs())
            .sum();
        assert!(approx(total, 20.0));
    }

    #[test]
    fn ramp_is_continuous() {
        // Successive segments should meet: each segment's end == next segment's start.
        let segments = expand_threshold_run(0.0, 10.0, 1.5);
        for pair in segments.windows(2) {
            assert!(
                approx(pair[0].end_x_mm, pair[1].start_x_mm),
                "gap between segments: {:?} → {:?}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn ramp_power_reaches_peak_at_middle() {
        let segments = expand_threshold_run(0.0, 10.0, 1.0);
        // Constant segment should have power 1.0
        let peak_count = segments
            .iter()
            .filter(|s| approx(s.power_fraction, 1.0))
            .count();
        assert_eq!(peak_count, 1);
    }
}
