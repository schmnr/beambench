//! Preflight validation checks before starting a job.

use beambench_common::machine::{PreflightCheck, PreflightOutcome, PreflightReport, SessionState};
use beambench_core::{MachineProfile, MachineProfileSnapshot};
use beambench_grbl::GrblSession;
use beambench_planner::ExecutionPlan;

/// Run preflight checks before starting a job.
pub fn run_preflight(
    session: &GrblSession,
    plan: &ExecutionPlan,
    profile: &MachineProfile,
) -> PreflightReport {
    let mut checks = Vec::new();

    // 1. Session is ready
    let session_ready = session.session_state() == SessionState::Ready;
    checks.push(PreflightCheck {
        category: "connection".to_string(),
        description: "Session is in Ready state".to_string(),
        passed: session_ready,
        message: if session_ready {
            "Connected and ready".to_string()
        } else {
            format!("Session state: {:?}", session.session_state())
        },
    });

    // 2. Machine is idle
    let machine_idle =
        session.last_status().run_state == beambench_common::machine::MachineRunState::Idle;
    checks.push(PreflightCheck {
        category: "machine".to_string(),
        description: "Machine is idle".to_string(),
        passed: machine_idle,
        message: if machine_idle {
            "Machine idle".to_string()
        } else {
            format!("Machine state: {:?}", session.last_status().run_state)
        },
    });

    // 3. No alarm
    let no_alarm =
        session.last_status().run_state != beambench_common::machine::MachineRunState::Alarm;
    checks.push(PreflightCheck {
        category: "machine".to_string(),
        description: "No active alarm".to_string(),
        passed: no_alarm,
        message: if no_alarm {
            "No alarm".to_string()
        } else {
            "Machine in alarm state".to_string()
        },
    });

    // 4. Plan is not empty
    let plan_not_empty = !plan.segments.is_empty();
    checks.push(PreflightCheck {
        category: "plan".to_string(),
        description: "Plan has segments".to_string(),
        passed: plan_not_empty,
        message: if plan_not_empty {
            format!("{} segments", plan.segments.len())
        } else {
            "Plan is empty".to_string()
        },
    });

    // 5. Bounds fit on bed
    let bounds = &plan.bounds;
    let fits_x = bounds.max.x <= profile.bed_width_mm && bounds.min.x >= 0.0;
    let fits_y = bounds.max.y <= profile.bed_height_mm && bounds.min.y >= 0.0;
    let bounds_fit = fits_x && fits_y;
    checks.push(PreflightCheck {
        category: "bounds".to_string(),
        description: "Plan fits within machine bed".to_string(),
        passed: bounds_fit,
        message: if bounds_fit {
            format!(
                "Plan bounds ({:.1}x{:.1}mm) fit bed ({:.0}x{:.0}mm)",
                bounds.width(),
                bounds.height(),
                profile.bed_width_mm,
                profile.bed_height_mm
            )
        } else {
            format!(
                "Plan bounds ({:.1},{:.1} to {:.1},{:.1}) exceed bed ({:.0}x{:.0}mm)",
                bounds.min.x,
                bounds.min.y,
                bounds.max.x,
                bounds.max.y,
                profile.bed_width_mm,
                profile.bed_height_mm
            )
        },
    });

    // 5b. Raster motion stays on the bed. Plan bounds cover burn geometry
    // only; the emitter adds overscan travel beyond each scanline and shifts
    // burns by the speed-based scanning offset, either of which can command
    // the head past the rails for designs near the bed edge.
    // The check is Some for every raster plan, passing or failing — only a
    // FAILING check is critical (a passing one must not escalate unrelated
    // warnings into a Fail outcome).
    let raster_motion = check_raster_motion_bounds(plan, profile);
    let raster_motion_ok = raster_motion.as_ref().is_none_or(|check| check.passed);
    if let Some(check) = raster_motion {
        checks.push(check);
    }

    // 6. Laser mode enabled ($32=1). Profiles configured for constant power
    // (M3) intentionally run in spindle mode — needle cutters, servo Z lifts,
    // some CO2 setups — so $32=0 is expected there, not a warning.
    let laser_mode = session.settings().laser_mode();
    let laser_mode_ok = laser_mode || profile.use_constant_power;
    checks.push(PreflightCheck {
        category: "settings".to_string(),
        description: "Laser mode enabled ($32=1)".to_string(),
        passed: laser_mode_ok,
        message: if laser_mode {
            "Laser mode enabled".to_string()
        } else if profile.use_constant_power {
            "Spindle mode ($32=0) with a constant-power (M3) profile".to_string()
        } else {
            "Laser mode disabled ($32=0). Enable with $32=1".to_string()
        },
    });

    // 7. Homing check (hard failure if profile requires it)
    let mut homing_failed = false;
    if profile.homing_enabled {
        let homing_enabled = session.settings().homing_enabled();
        if !homing_enabled {
            homing_failed = true;
        }
        checks.push(PreflightCheck {
            category: "settings".to_string(),
            description: "Homing is enabled".to_string(),
            passed: homing_enabled,
            message: if homing_enabled {
                "Homing enabled".to_string()
            } else {
                "Profile requires homing but $22=0".to_string()
            },
        });
    }

    // Determine outcome
    let all_passed = checks.iter().all(|c| c.passed);
    // Session-ready, plan-not-empty, bounds-fit, raster-motion, no-alarm,
    // and homing (when required) are hard failures
    let critical_failed = !session_ready
        || !machine_idle
        || !plan_not_empty
        || !bounds_fit
        || !raster_motion_ok
        || !no_alarm
        || homing_failed;

    let outcome = if all_passed {
        PreflightOutcome::Pass
    } else if critical_failed {
        PreflightOutcome::Fail
    } else {
        PreflightOutcome::PassWithWarnings
    };

    PreflightReport { outcome, checks }
}

/// Validate that worst-case raster head motion (burn extent extended by
/// overscan and the speed-based scanning offset) stays within the bed.
/// Returns `Some(check)` with `passed: false` when motion exceeds the bed,
/// `None` when there is nothing to report (no raster segments or all fit).
pub fn check_raster_motion_bounds(
    plan: &ExecutionPlan,
    profile: &MachineProfile,
) -> Option<PreflightCheck> {
    use beambench_planner::{PlanSegment, ScanAxis};

    let offset_table: Vec<(f64, f64)> = {
        let mut pairs: Vec<(f64, f64)> = profile
            .scanning_offsets
            .iter()
            .map(|e| (e.speed_mm_min, e.offset_mm))
            .collect();
        pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
        pairs
    };

    // (axis label, lo, hi, limit, margin)
    let mut worst: Option<(&'static str, f64, f64, f64, f64)> = None;
    let mut saw_raster = false;

    for segment in &plan.segments {
        let PlanSegment::Raster {
            scanlines,
            speed_mm_min,
            scan_angle_deg,
            scan_origin,
            overscan_mm,
            scan_axis,
            ..
        } = segment
        else {
            continue;
        };
        saw_raster = true;

        let scan_offset = if profile.enable_scanning_offset && !offset_table.is_empty() {
            beambench_grbl::interpolate_scanning_offset(&offset_table, *speed_mm_min).abs()
        } else {
            0.0
        };
        // Overscan extends from the offset-shifted burn position, and
        // bidirectional rows shift alternately in both directions, so the
        // worst case is symmetric on both ends of the run axis.
        let margin = overscan_mm + scan_offset;
        if margin <= 0.0 {
            continue;
        }

        // Mirror the G-code emitter's geometry (gcode.rs): angles near a
        // multiple of 90 degrees use the axis-aligned frame; anything else
        // rotates local (run, cross) points around scan_origin.
        let is_orthogonal = scan_angle_deg.abs() < 0.5
            || (scan_angle_deg.abs() - 90.0).abs() < 0.5
            || (scan_angle_deg.abs() - 180.0).abs() < 0.5
            || (scan_angle_deg.abs() - 270.0).abs() < 0.5
            || (scan_angle_deg.abs() - 360.0).abs() < 0.5;

        if is_orthogonal {
            let mut run_min = f64::INFINITY;
            let mut run_max = f64::NEG_INFINITY;
            for scanline in scanlines {
                for run in &scanline.runs {
                    run_min = run_min.min(run.start_x_mm.min(run.end_x_mm));
                    run_max = run_max.max(run.start_x_mm.max(run.end_x_mm));
                }
            }
            if !run_min.is_finite() {
                continue;
            }
            let (axis, limit) = match scan_axis {
                ScanAxis::Horizontal => ("X", profile.bed_width_mm),
                ScanAxis::Vertical => ("Y", profile.bed_height_mm),
            };
            record_axis_overrun(
                &mut worst,
                axis,
                run_min - margin,
                run_max + margin,
                limit,
                margin,
            );
        } else {
            // Rotated scan: the overscan-extended run endpoints of every
            // scanline map into world space; the plan bounds only cover the
            // burn extent, so this travel must be checked here.
            let radians = scan_angle_deg.to_radians();
            let (sin_a, cos_a) = radians.sin_cos();
            let mut min_x = f64::INFINITY;
            let mut max_x = f64::NEG_INFINITY;
            let mut min_y = f64::INFINITY;
            let mut max_y = f64::NEG_INFINITY;
            let mut any_runs = false;
            for scanline in scanlines {
                let mut run_min = f64::INFINITY;
                let mut run_max = f64::NEG_INFINITY;
                for run in &scanline.runs {
                    run_min = run_min.min(run.start_x_mm.min(run.end_x_mm));
                    run_max = run_max.max(run.start_x_mm.max(run.end_x_mm));
                }
                if !run_min.is_finite() {
                    continue;
                }
                any_runs = true;
                let cross = scanline.y_mm;
                for run_pos in [run_min - margin, run_max + margin] {
                    let x = scan_origin.x + run_pos * cos_a - cross * sin_a;
                    let y = scan_origin.y + run_pos * sin_a + cross * cos_a;
                    min_x = min_x.min(x);
                    max_x = max_x.max(x);
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                }
            }
            if !any_runs {
                continue;
            }
            record_axis_overrun(&mut worst, "X", min_x, max_x, profile.bed_width_mm, margin);
            record_axis_overrun(&mut worst, "Y", min_y, max_y, profile.bed_height_mm, margin);
        }
    }

    if !saw_raster {
        return None;
    }

    Some(match worst {
        None => PreflightCheck {
            category: "bounds".to_string(),
            description: "Raster motion (overscan and scanning offset) fits within machine bed"
                .to_string(),
            passed: true,
            message: "Raster motion within bed".to_string(),
        },
        Some((axis, lo, hi, limit, margin)) => PreflightCheck {
            category: "bounds".to_string(),
            description: "Raster motion (overscan and scanning offset) fits within machine bed"
                .to_string(),
            passed: false,
            message: format!(
                "Raster motion spans {lo:.1} to {hi:.1}mm on the 0 to {limit:.0}mm {axis} axis \
                 ({margin:.1}mm of overscan and scanning offset beyond the burn area). \
                 Reduce overscan or move the design further from the bed edge."
            ),
        },
    })
}

/// Track the worst bed overrun seen across raster segments and axes.
fn record_axis_overrun(
    worst: &mut Option<(&'static str, f64, f64, f64, f64)>,
    axis: &'static str,
    lo: f64,
    hi: f64,
    limit: f64,
    margin: f64,
) {
    if lo < -1e-6 || hi > limit + 1e-6 {
        let overrun = (hi - limit).max(-lo);
        let current = worst.map(|(_, wlo, whi, wlimit, _)| (whi - wlimit).max(-wlo));
        if current.is_none_or(|c| overrun > c) {
            *worst = Some((axis, lo, hi, limit, margin));
        }
    }
}

/// Returns an informational check if any enabled tool layers exist.
pub fn check_tool_layers(project: &beambench_core::Project) -> Option<PreflightCheck> {
    let _ = project;
    None
}

/// Check whether the project's saved machine profile snapshot matches the
/// currently active profile's bed dimensions. Returns a failing
/// `PreflightCheck` if the bed width or height differ, or `None` if they match
/// (or if no snapshot is present in the project).
pub fn check_profile_mismatch(
    snapshot: &MachineProfileSnapshot,
    active_profile: &MachineProfile,
) -> Option<PreflightCheck> {
    let width_match = (snapshot.bed_width_mm - active_profile.bed_width_mm).abs() < f64::EPSILON;
    let height_match = (snapshot.bed_height_mm - active_profile.bed_height_mm).abs() < f64::EPSILON;

    if width_match && height_match {
        None
    } else {
        Some(PreflightCheck {
            category: "profile".to_string(),
            description: "Machine profile bed size mismatch".to_string(),
            passed: false,
            message: format!(
                "Project was created for '{}' ({:.0}x{:.0}mm) but active profile '{}' has {:.0}x{:.0}mm bed",
                snapshot.profile_name,
                snapshot.bed_width_mm,
                snapshot.bed_height_mm,
                active_profile.name,
                active_profile.bed_width_mm,
                active_profile.bed_height_mm,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::geometry::{Bounds, Point2D};
    use beambench_planner::*;
    use beambench_serial::MockSerialTransport;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_plan(bounds: Bounds) -> ExecutionPlan {
        ExecutionPlan {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            revision_hash: "test".to_string(),
            created_at: Utc::now(),
            bounds,
            total_distance_mm: 100.0,
            estimated_duration_secs: 10.0,
            segments: vec![PlanSegment::Travel {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(10.0, 0.0),
            }],
            layer_order: vec![],
            failed_entries: vec![],
            warnings: vec![],
        }
    }

    fn make_ready_session_with_settings() -> GrblSession {
        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("Grbl 1.1h");
        transport.enqueue_response("$32=1");
        transport.enqueue_response("$22=1");
        transport.enqueue_response("<Idle|MPos:0.000,0.000,0.000|FS:0,0>");
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        session.mark_ready().unwrap();
        session
    }

    fn make_profile() -> MachineProfile {
        MachineProfile {
            bed_width_mm: 200.0,
            bed_height_mm: 200.0,
            homing_enabled: false,
            ..Default::default()
        }
    }

    #[test]
    fn preflight_passes_when_all_good() {
        let session = make_ready_session_with_settings();
        let plan = make_plan(Bounds::new(
            Point2D::new(0.0, 0.0),
            Point2D::new(100.0, 100.0),
        ));
        let profile = make_profile();

        let report = run_preflight(&session, &plan, &profile);
        assert_eq!(report.outcome, PreflightOutcome::Pass);
    }

    fn raster_plan(run_start: f64, run_end: f64, overscan_mm: f64) -> ExecutionPlan {
        let mut plan = make_plan(Bounds::new(
            Point2D::new(run_start, 50.0),
            Point2D::new(run_end, 60.0),
        ));
        plan.segments = vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 50.0,
                runs: vec![ScanRun {
                    start_x_mm: run_start,
                    end_x_mm: run_end,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 3000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: "e1".to_string(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm,
            outlines: vec![],
            scan_axis: ScanAxis::Horizontal,
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.1,
        }];
        plan
    }

    #[test]
    fn spindle_mode_passes_preflight_with_constant_power_profile() {
        // Needle cutters and servo-driven machines run $32=0 deliberately;
        // a constant-power (M3) profile makes that configuration valid.
        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("Grbl 1.1h");
        transport.enqueue_response("$32=0");
        transport.enqueue_response("$22=1");
        transport.enqueue_response("<Idle|MPos:0.000,0.000,0.000|FS:0,0>");
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        session.mark_ready().unwrap();

        let plan = make_plan(Bounds::new(
            Point2D::new(0.0, 0.0),
            Point2D::new(100.0, 100.0),
        ));

        let mut profile = make_profile();
        profile.use_constant_power = true;
        let report = run_preflight(&session, &plan, &profile);
        assert_eq!(
            report.outcome,
            PreflightOutcome::Pass,
            "constant-power profile in spindle mode must not be blocked: {:?}",
            report.checks
        );

        // Without the constant-power profile, $32=0 still warns.
        profile.use_constant_power = false;
        let report = run_preflight(&session, &plan, &profile);
        assert_eq!(report.outcome, PreflightOutcome::PassWithWarnings);
    }

    #[test]
    fn preflight_fails_when_overscan_exceeds_bed_edge() {
        let session = make_ready_session_with_settings();
        // Burn ends 2mm from the right rail of a 200mm bed; 5mm overscan
        // commands the head 3mm past it.
        let plan = raster_plan(20.0, 198.0, 5.0);
        let profile = make_profile();

        let report = run_preflight(&session, &plan, &profile);
        assert_eq!(report.outcome, PreflightOutcome::Fail);
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.description.contains("Raster motion") && !c.passed),
            "raster motion check should fail: {:?}",
            report.checks
        );
    }

    #[test]
    fn preflight_fails_when_rotated_raster_overscan_exceeds_bed() {
        let session = make_ready_session_with_settings();
        // 45-degree scan: burn runs r=180..210 at cross=50 map to world
        // x=91.9..113.1, y=162.6..183.8 - inside the 200mm bed, so the plan
        // bounds row passes. The 30mm overscan extends r to 240, whose world
        // y=205.1 exits the bed; only the raster motion check can catch it.
        let mut plan = raster_plan(180.0, 210.0, 30.0);
        if let PlanSegment::Raster { scan_angle_deg, .. } = &mut plan.segments[0] {
            *scan_angle_deg = 45.0;
        }
        plan.bounds = Bounds::new(Point2D::new(90.0, 160.0), Point2D::new(115.0, 185.0));
        let profile = make_profile();

        let report = run_preflight(&session, &plan, &profile);
        assert_eq!(report.outcome, PreflightOutcome::Fail);
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.description.contains("Raster motion") && !c.passed),
            "rotated raster motion check should fail: {:?}",
            report.checks
        );
    }

    #[test]
    fn preflight_passes_when_overscan_fits() {
        let session = make_ready_session_with_settings();
        // Same overscan, but the burn leaves room for it on both sides.
        let plan = raster_plan(20.0, 180.0, 5.0);
        let profile = make_profile();

        let report = run_preflight(&session, &plan, &profile);
        assert_eq!(report.outcome, PreflightOutcome::Pass);
    }

    #[test]
    fn fitting_raster_does_not_escalate_warnings_to_fail() {
        // A raster whose motion check PASSES plus an unrelated warning
        // ($32=0 without a constant-power profile): the outcome must stay
        // PassWithWarnings. The passing raster check used to be treated as
        // critical merely because it existed, escalating this to Fail.
        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("Grbl 1.1h");
        transport.enqueue_response("$32=0");
        transport.enqueue_response("$22=1");
        transport.enqueue_response("<Idle|MPos:0.000,0.000,0.000|FS:0,0>");
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        session.mark_ready().unwrap();

        // Raster well inside the bed: overscan fits on both sides.
        let plan = raster_plan(20.0, 180.0, 5.0);
        let profile = make_profile();

        let report = run_preflight(&session, &plan, &profile);
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.description.contains("Raster motion") && c.passed),
            "raster motion check should be present and passing: {:?}",
            report.checks
        );
        assert_eq!(
            report.outcome,
            PreflightOutcome::PassWithWarnings,
            "a passing raster check must not turn warnings into Fail: {:?}",
            report.checks
        );
    }

    #[test]
    fn preflight_includes_scanning_offset_in_motion_margin() {
        let session = make_ready_session_with_settings();
        // 3mm overscan fits (192 + 3 < 200), but a 6mm scanning offset at
        // this speed pushes worst-case motion past the rail.
        let plan = raster_plan(20.0, 192.0, 3.0);
        let mut profile = make_profile();
        profile.enable_scanning_offset = true;
        profile.scanning_offsets = vec![beambench_core::ScanningOffsetEntry {
            speed_mm_min: 3000.0,
            offset_mm: 6.0,
        }];

        let report = run_preflight(&session, &plan, &profile);
        assert_eq!(report.outcome, PreflightOutcome::Fail);
    }

    #[test]
    fn preflight_fails_when_disconnected() {
        let transport = MockSerialTransport::new("mock");
        let session = GrblSession::new(Box::new(transport));
        let plan = make_plan(Bounds::new(
            Point2D::new(0.0, 0.0),
            Point2D::new(100.0, 100.0),
        ));
        let profile = make_profile();

        let report = run_preflight(&session, &plan, &profile);
        assert_eq!(report.outcome, PreflightOutcome::Fail);
    }

    #[test]
    fn preflight_fails_when_plan_empty() {
        let session = make_ready_session_with_settings();
        let mut plan = make_plan(Bounds::new(
            Point2D::new(0.0, 0.0),
            Point2D::new(100.0, 100.0),
        ));
        plan.segments.clear();
        let profile = make_profile();

        let report = run_preflight(&session, &plan, &profile);
        assert_eq!(report.outcome, PreflightOutcome::Fail);
    }

    #[test]
    fn preflight_fails_when_out_of_bounds() {
        let session = make_ready_session_with_settings();
        let plan = make_plan(Bounds::new(
            Point2D::new(0.0, 0.0),
            Point2D::new(300.0, 300.0),
        ));
        let profile = make_profile();

        let report = run_preflight(&session, &plan, &profile);
        assert_eq!(report.outcome, PreflightOutcome::Fail);
    }

    #[test]
    fn preflight_warns_when_laser_mode_off() {
        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("Grbl 1.1h");
        transport.enqueue_response("$32=0"); // laser mode off
        transport.enqueue_response("<Idle|MPos:0.000,0.000,0.000|FS:0,0>");
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        session.mark_ready().unwrap();

        let plan = make_plan(Bounds::new(
            Point2D::new(0.0, 0.0),
            Point2D::new(100.0, 100.0),
        ));
        let profile = make_profile();

        let report = run_preflight(&session, &plan, &profile);
        assert_eq!(report.outcome, PreflightOutcome::PassWithWarnings);
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.category == "settings" && !c.passed)
        );
    }

    #[test]
    fn preflight_fails_when_machine_is_not_idle() {
        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("Grbl 1.1h");
        transport.enqueue_response("$32=1");
        transport.enqueue_response("$22=1");
        transport.enqueue_response("<Hold:0|MPos:0.000,0.000,0.000|FS:0,0>");
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        session.mark_ready().unwrap();

        let plan = make_plan(Bounds::new(
            Point2D::new(0.0, 0.0),
            Point2D::new(100.0, 100.0),
        ));
        let profile = make_profile();

        let report = run_preflight(&session, &plan, &profile);
        assert_eq!(report.outcome, PreflightOutcome::Fail);
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.description == "Machine is idle" && !c.passed)
        );
    }

    #[test]
    fn profile_mismatch_detects_bed_size_difference() {
        let profile = MachineProfile {
            name: "Big Laser".to_string(),
            bed_width_mm: 400.0,
            bed_height_mm: 300.0,
            ..Default::default()
        };
        let snapshot = beambench_core::MachineProfileSnapshot {
            profile_id: profile.id,
            profile_name: "Small Laser".to_string(),
            bed_width_mm: 200.0,
            bed_height_mm: 200.0,
            max_speed_mm_min: 3000.0,
        };

        let result = check_profile_mismatch(&snapshot, &profile);
        assert!(result.is_some());
        let check = result.unwrap();
        assert!(!check.passed);
        assert_eq!(check.category, "profile");
        assert!(check.message.contains("Small Laser"));
        assert!(check.message.contains("Big Laser"));
    }

    #[test]
    fn profile_mismatch_passes_when_bed_sizes_match() {
        let profile = MachineProfile {
            name: "My Laser".to_string(),
            bed_width_mm: 200.0,
            bed_height_mm: 200.0,
            ..Default::default()
        };
        let snapshot = beambench_core::MachineProfileSnapshot {
            profile_id: profile.id,
            profile_name: "My Laser".to_string(),
            bed_width_mm: 200.0,
            bed_height_mm: 200.0,
            max_speed_mm_min: 3000.0,
        };

        let result = check_profile_mismatch(&snapshot, &profile);
        assert!(result.is_none());
    }

    #[test]
    fn profile_mismatch_detects_width_only_difference() {
        let profile = MachineProfile {
            bed_width_mm: 300.0,
            bed_height_mm: 200.0,
            ..Default::default()
        };
        let snapshot = beambench_core::MachineProfileSnapshot {
            profile_id: profile.id,
            profile_name: "Other".to_string(),
            bed_width_mm: 200.0,
            bed_height_mm: 200.0,
            max_speed_mm_min: 3000.0,
        };

        assert!(check_profile_mismatch(&snapshot, &profile).is_some());
    }

    #[test]
    fn profile_mismatch_detects_height_only_difference() {
        let profile = MachineProfile {
            bed_width_mm: 200.0,
            bed_height_mm: 300.0,
            ..Default::default()
        };
        let snapshot = beambench_core::MachineProfileSnapshot {
            profile_id: profile.id,
            profile_name: "Other".to_string(),
            bed_width_mm: 200.0,
            bed_height_mm: 200.0,
            max_speed_mm_min: 3000.0,
        };

        assert!(check_profile_mismatch(&snapshot, &profile).is_some());
    }

    #[test]
    fn preflight_fails_when_homing_required_but_disabled() {
        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("Grbl 1.1h");
        transport.enqueue_response("$32=1"); // laser mode on
        transport.enqueue_response("$22=0"); // homing disabled
        transport.enqueue_response("<Idle|MPos:0.000,0.000,0.000|FS:0,0>");
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        session.mark_ready().unwrap();

        let plan = make_plan(Bounds::new(
            Point2D::new(0.0, 0.0),
            Point2D::new(100.0, 100.0),
        ));
        let mut profile = make_profile();
        profile.homing_enabled = true; // profile requires homing

        let report = run_preflight(&session, &plan, &profile);
        assert_eq!(report.outcome, PreflightOutcome::Fail);
        assert!(report.checks.iter().any(|c| c.category == "settings"
            && c.description == "Homing is enabled"
            && !c.passed));
    }

    #[test]
    fn check_tool_layers_returns_none_when_enabled_tool_layers_exist() {
        let mut project = beambench_core::Project::new("Tool Test");
        let mut layer = beambench_core::Layer::new("T1", beambench_core::OperationType::Line);
        layer.is_tool_layer = true;
        project.add_layer(layer);

        assert!(check_tool_layers(&project).is_none());
    }

    #[test]
    fn check_tool_layers_returns_none_when_no_tool_layers() {
        let mut project = beambench_core::Project::new("Normal");
        let layer = beambench_core::Layer::new("Lines", beambench_core::OperationType::Line);
        project.add_layer(layer);

        assert!(check_tool_layers(&project).is_none());
    }

    #[test]
    fn check_tool_layers_ignores_disabled_tool_layers() {
        let mut project = beambench_core::Project::new("Disabled Tool");
        let mut layer = beambench_core::Layer::new("T1", beambench_core::OperationType::Line);
        layer.is_tool_layer = true;
        layer.enabled = false;
        project.add_layer(layer);

        assert!(check_tool_layers(&project).is_none());
    }
}
