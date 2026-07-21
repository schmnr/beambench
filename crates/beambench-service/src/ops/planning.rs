use beambench_common::StartFromMode;
use beambench_common::geometry::Bounds;
use beambench_core::object::ProjectObject;
use beambench_core::{MachineProfile, Project};
use beambench_grbl::generate_gcode;
use beambench_planner::{
    ExecutionPlan, PlanStats, PlannerCancellation, PlannerError, PlannerInput,
    build_plan_with_input_and_cache,
};
use beambench_preview::{PreviewData, distill_preview};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::context::ServiceContext;
use crate::error::{ServiceError, ServiceResult};
use crate::events;
use crate::ops::{output, project};
use crate::runtime::MachineSessionHandle;

fn lock_err(name: &str, e: impl std::fmt::Display) -> ServiceError {
    ServiceError::internal(format!("Failed to lock {name}: {e}"))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionJobOptions {
    #[serde(default)]
    pub cut_selected_graphics: bool,
    #[serde(default)]
    pub use_selection_origin: bool,
    #[serde(default)]
    pub selected_object_ids: Vec<String>,
}

impl SessionJobOptions {
    fn affects_plan(&self) -> bool {
        self.cut_selected_graphics || self.use_selection_origin
    }
}

pub fn revision_hash(project: &Project) -> ServiceResult<String> {
    let json = serde_json::to_string(project)
        .map_err(|e| ServiceError::internal(format!("Failed to serialize project: {e}")))?;
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn is_cached_plan_valid(ctx: &ServiceContext, project: &Project) -> bool {
    let Ok(current_hash) = revision_hash(project) else {
        return false;
    };
    let current_project_id = *project.metadata.project_id.as_uuid();
    let Ok(guard) = ctx.plan_cache.lock() else {
        return false;
    };
    guard
        .as_ref()
        .is_some_and(|p| p.project_id == current_project_id && p.revision_hash == current_hash)
}

pub fn current_project(ctx: &ServiceContext) -> ServiceResult<Project> {
    let guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let mut project = guard
        .clone()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    project::refresh_project_text_caches(&mut project);
    Ok(project)
}

pub fn invalidate_plan_cache(ctx: &ServiceContext) -> ServiceResult<()> {
    let mut guard = ctx
        .plan_cache
        .lock()
        .map_err(|e| lock_err("plan_cache", e))?;
    *guard = None;
    Ok(())
}

fn active_profile(ctx: &ServiceContext) -> ServiceResult<MachineProfile> {
    let settings = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
    Ok(settings
        .active_profile_id
        .and_then(|id| settings.machine_profiles.iter().find(|p| p.id == id))
        .cloned()
        .unwrap_or_default())
}

fn bounds_of_objects(objects: &[ProjectObject]) -> Option<Bounds> {
    objects
        .iter()
        .map(|object| object.bounds)
        .reduce(|acc, bounds| acc.union(&bounds))
}

fn apply_session_job_options(
    mut project: Project,
    options: &SessionJobOptions,
) -> ServiceResult<(Project, Option<Bounds>)> {
    if !options.affects_plan() {
        return Ok((project, None));
    }

    if options.use_selection_origin && !options.cut_selected_graphics {
        return Err(ServiceError::invalid_input(
            "use_selection_origin requires cut_selected_graphics",
        ));
    }

    if options.selected_object_ids.is_empty() {
        return Err(ServiceError::invalid_input(
            "Select at least one graphic before using selection-dependent job options",
        ));
    }

    let selected_ids: HashSet<&str> = options
        .selected_object_ids
        .iter()
        .map(String::as_str)
        .collect();
    let selected_objects: Vec<ProjectObject> = project
        .objects
        .iter()
        .filter(|object| {
            selected_ids.contains(object.id.to_string().as_str())
                && object.visible
                && !object.locked
        })
        .cloned()
        .collect();

    if selected_objects.is_empty() {
        return Err(ServiceError::invalid_input(
            "Selected-only job options require at least one visible, unlocked selected graphic",
        ));
    }

    let selection_bounds = if options.use_selection_origin {
        Some(bounds_of_objects(&selected_objects).ok_or_else(|| {
            ServiceError::invalid_input("Selected graphics do not have usable bounds")
        })?)
    } else {
        None
    };

    if options.cut_selected_graphics {
        project.objects.retain(|object| {
            selected_ids.contains(object.id.to_string().as_str())
                && object.visible
                && !object.locked
        });
    }

    Ok((project, selection_bounds))
}

fn build_plan_for_project(
    ctx: &ServiceContext,
    project: &Project,
    selection_origin_bounds: Option<Bounds>,
) -> ServiceResult<ExecutionPlan> {
    let runtime = ctx
        .optimization_runtime
        .lock()
        .map_err(|e| lock_err("optimization_runtime", e))?
        .clone();
    let calibration = output::build_planner_calibration(&active_profile(ctx)?);
    let request_id = ctx
        .latest_planning_request_id
        .fetch_add(1, Ordering::AcqRel)
        + 1;
    let input =
        PlannerInput::new(project.optimization.clone(), runtime, calibration).with_cancellation(
            PlannerCancellation::new(Arc::clone(&ctx.latest_planning_request_id), request_id),
        );
    let input = if let Some(bounds) = selection_origin_bounds {
        input.with_job_origin_bounds(bounds)
    } else {
        input
    };

    build_plan_with_input_and_cache(project, &input, &ctx.raster_cache, &ctx.scaled_image_cache)
        .map_err(|e| match e {
            PlannerError::Cancelled => ServiceError::stale_revision("Plan generation cancelled"),
            other => ServiceError::invalid_state(format!("Plan generation failed: {other}")),
        })
}

/// If the project uses CurrentPosition mode and a machine session is
/// available, capture the live `work_position` into the runtime overlay
/// (`ctx.optimization_runtime.current_position`) and invalidate the
/// plan cache so the next build uses it. For other modes, ensures the
/// overlay's `current_position` is `None`.
///
/// The persisted `project.optimization` block is **never touched** here —
/// `current_position` is runtime-only by design and lives solely on the
/// ephemeral overlay (see `context::ServiceContext::optimization_runtime`).
pub fn sync_current_position(ctx: &ServiceContext) -> ServiceResult<()> {
    let start_from = {
        let proj_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        match proj_guard.as_ref() {
            Some(p) => p.start_from,
            None => return Ok(()),
        }
    };

    let live_pos = if start_from == StartFromMode::CurrentPosition {
        let session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
        match session_lock.as_ref() {
            Some(MachineSessionHandle::Grbl(session)) => {
                let status = session.last_status();
                Some((status.work_position.x, status.work_position.y))
            }
            Some(MachineSessionHandle::Dsp(session)) => {
                let wp = &session.machine_status.work_position;
                Some((wp.x, wp.y))
            }
            _ => None,
        }
    } else {
        None
    };

    let mut runtime = ctx
        .optimization_runtime
        .lock()
        .map_err(|e| lock_err("optimization_runtime", e))?;
    if runtime.current_position != live_pos {
        runtime.current_position = live_pos;
        drop(runtime);
        invalidate_plan_cache(ctx)?;
    }
    Ok(())
}

pub fn generate_plan(ctx: &ServiceContext) -> ServiceResult<ExecutionPlan> {
    generate_plan_with_options(ctx, &SessionJobOptions::default())
}

pub fn generate_plan_with_options(
    ctx: &ServiceContext,
    options: &SessionJobOptions,
) -> ServiceResult<ExecutionPlan> {
    sync_current_position(ctx)?;
    let project = current_project(ctx)?;
    let use_project_cache = !options.affects_plan();
    let (effective_project, selection_origin_bounds) =
        apply_session_job_options(project.clone(), options)?;
    let plan = build_plan_for_project(ctx, &effective_project, selection_origin_bounds)?;
    if use_project_cache {
        let mut guard = ctx
            .plan_cache
            .lock()
            .map_err(|e| lock_err("plan_cache", e))?;
        *guard = Some(plan.clone());
    }
    let path = {
        let path_guard = ctx
            .project_path
            .lock()
            .map_err(|e| lock_err("project_path", e))?;
        path_guard.clone()
    };
    ctx.emit_event(
        "plan.generated",
        json!({
            "project": events::project_summary(&project, path.as_deref()),
            "stats": plan.stats(),
            "bounds": plan.bounds,
            "session_options": options.affects_plan(),
        }),
    );
    Ok(plan)
}

pub fn cancel_planning(ctx: &ServiceContext) -> ServiceResult<()> {
    ctx.latest_planning_request_id
        .fetch_add(1, Ordering::AcqRel);
    ctx.emit_event("planning.cancelled", json!({}));
    Ok(())
}

pub fn ensure_current_plan(ctx: &ServiceContext) -> ServiceResult<ExecutionPlan> {
    ensure_current_plan_with_options(ctx, &SessionJobOptions::default())
}

pub fn ensure_current_plan_with_options(
    ctx: &ServiceContext,
    options: &SessionJobOptions,
) -> ServiceResult<ExecutionPlan> {
    if options.affects_plan() {
        return generate_plan_with_options(ctx, options);
    }

    // Sync live machine position so CurrentPosition mode always uses fresh data.
    // This may invalidate the cache, forcing a rebuild below.
    sync_current_position(ctx)?;

    let project = current_project(ctx)?;

    if is_cached_plan_valid(ctx, &project) {
        let guard = ctx
            .plan_cache
            .lock()
            .map_err(|e| lock_err("plan_cache", e))?;
        if let Some(plan) = guard.as_ref() {
            return Ok(plan.clone());
        }
    }

    generate_plan_with_options(ctx, options)
}

pub fn require_current_plan(ctx: &ServiceContext) -> ServiceResult<ExecutionPlan> {
    let project = current_project(ctx)?;
    if !is_cached_plan_valid(ctx, &project) {
        return Err(ServiceError::stale_revision(
            "Cached plan is stale — project has changed. Regenerate the plan.",
        ));
    }
    let guard = ctx
        .plan_cache
        .lock()
        .map_err(|e| lock_err("plan_cache", e))?;
    guard.clone().ok_or_else(|| {
        ServiceError::invalid_state("No execution plan available. Generate a plan first.")
    })
}

pub fn get_plan_stats(ctx: &ServiceContext) -> ServiceResult<PlanStats> {
    Ok(ensure_current_plan(ctx)?.stats())
}

pub fn generate_preview(ctx: &ServiceContext) -> ServiceResult<PreviewData> {
    generate_preview_with_options(ctx, &SessionJobOptions::default())
}

pub fn generate_preview_with_options(
    ctx: &ServiceContext,
    options: &SessionJobOptions,
) -> ServiceResult<PreviewData> {
    let plan = ensure_current_plan_with_options(ctx, options)?;
    let preview = distill_preview(&plan);
    ctx.emit_event(
        "preview.generated",
        json!({
            "project_id": plan.project_id,
            "revision_hash": plan.revision_hash,
            "segment_count": preview.stats.segment_count,
            "bounds": plan.bounds,
        }),
    );
    Ok(preview)
}

pub fn export_gcode_to_path(ctx: &ServiceContext, path: &std::path::Path) -> ServiceResult<String> {
    export_gcode_to_path_with_options(ctx, path, &SessionJobOptions::default())
}

pub fn export_gcode_to_path_with_options(
    ctx: &ServiceContext,
    path: &std::path::Path,
    options: &SessionJobOptions,
) -> ServiceResult<String> {
    let plan = ensure_current_plan_with_options(ctx, options)?;
    let project = current_project(ctx)?;
    let profile = active_profile(ctx)?;
    let mut gcode_config = output::build_gcode_config(&project.optimization, &profile);
    output::apply_project_gcode_metadata(&mut gcode_config, &project);
    let gcode_lines = generate_gcode(&plan, &gcode_config)
        .map_err(|e| ServiceError::invalid_state(format!("G-code generation failed: {e}")))?;
    std::fs::write(path, gcode_lines.join("\n"))
        .map_err(|e| ServiceError::persistence(format!("Failed to write G-code file: {e}")))?;
    ctx.emit_event(
        "preview.gcode.exported",
        json!({
            "project_id": plan.project_id,
            "revision_hash": plan.revision_hash,
            "path": path.to_string_lossy().to_string(),
            "line_count": gcode_lines.len(),
        }),
    );
    Ok(path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::AnchorPoint;
    use beambench_common::geometry::{Bounds, Point2D};
    use beambench_core::layer::{Layer, OperationType};
    use beambench_core::object::{ObjectData, ProjectObject, ShapeKind};
    use beambench_core::{DirectionOrder, FinishPosition};
    use beambench_planner::PlanSegment;

    use crate::ops::imports::{ImportSvgInput, import_svg_from_path};

    /// Helper: mutate the currently-open project's `optimization` block
    /// in-place. Persisted optimization state lives on the project, not on
    /// the service context, so tests that want to
    /// exercise plan/preview/export with a non-default optimization go
    /// through the project lock.
    fn mutate_project_optimization<F>(ctx: &ServiceContext, f: F)
    where
        F: FnOnce(&mut beambench_core::ProjectOptimization),
    {
        let mut guard = ctx.project.lock().unwrap();
        let project = guard.as_mut().expect("no project open");
        f(&mut project.optimization);
        drop(guard);
        invalidate_plan_cache(ctx).unwrap();
    }

    fn create_test_ctx_with_project() -> ServiceContext {
        let ctx = ServiceContext::with_settings(beambench_core::AppSettings::default());

        let mut project = Project::new("Test");
        let layer = Layer::new("Lines", OperationType::Line);
        let layer_id = layer.id;
        project.layers.push(layer);

        // Add scattered rectangles
        let positions = [(10.0, 10.0), (200.0, 50.0), (50.0, 200.0), (300.0, 250.0)];
        for (i, (x, y)) in positions.iter().enumerate() {
            project.add_object(ProjectObject::new(
                &format!("rect{i}"),
                layer_id,
                Bounds::new(Point2D::new(*x, *y), Point2D::new(x + 30.0, y + 30.0)),
                ObjectData::Shape {
                    kind: ShapeKind::Rectangle,
                    width: 30.0,
                    height: 30.0,
                    corner_radius: 0.0,
                },
            ));
        }

        *ctx.project.lock().unwrap() = Some(project);
        ctx
    }

    #[test]
    fn preview_reflects_optimization_segment_count() {
        let ctx = create_test_ctx_with_project();
        mutate_project_optimization(&ctx, |opt| {
            opt.reduce_travel = true;
        });

        let plan = generate_plan(&ctx).unwrap();
        let preview = generate_preview(&ctx).unwrap();

        // Preview segment count should match plan segment count
        assert_eq!(
            preview.stats.segment_count,
            plan.segments.len(),
            "Preview segment count should match plan segment count"
        );
    }

    #[test]
    fn preview_fills_imported_svg_text_on_fill_layer() {
        let ctx = ServiceContext::with_settings(beambench_core::AppSettings::default());
        let mut project = Project::new("Imported Text Fill");
        let layer = Layer::new("Fill", OperationType::Fill);
        let layer_id = layer.id;
        project.layers.push(layer);
        *ctx.project.lock().unwrap() = Some(project);

        let dir = tempfile::tempdir().unwrap();
        let svg_path = dir.path().join("text.svg");
        std::fs::write(
            &svg_path,
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50">
                <text x="10" y="30" font-family="sans-serif" font-size="16">Fill</text>
            </svg>"#,
        )
        .unwrap();

        let imported = import_svg_from_path(
            &ctx,
            ImportSvgInput {
                file_path: svg_path.to_string_lossy().to_string(),
                layer_id,
            },
        )
        .unwrap();
        assert!(
            !imported.is_empty(),
            "SVG text should import into the fill layer"
        );

        let preview = generate_preview(&ctx).unwrap();
        let raster_region_count: usize = preview
            .layers
            .iter()
            .map(|layer| layer.raster_regions.len())
            .sum();
        assert!(
            raster_region_count > 0,
            "Fill-layer SVG text should produce raster preview regions, not only workspace wireframes"
        );
        assert!(
            preview.failed_entries.is_empty(),
            "Fill-layer SVG text preview should not fail entries: {:?}",
            preview.failed_entries
        );
    }

    #[test]
    fn export_with_custom_finish_contains_travel_to_custom_point() {
        let ctx = create_test_ctx_with_project();
        mutate_project_optimization(&ctx, |opt| {
            opt.finish_position = FinishPosition::CustomXY;
            opt.finish_x = Some(99.0);
            opt.finish_y = Some(88.0);
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.gcode");
        export_gcode_to_path(&ctx, &path).unwrap();

        let gcode = std::fs::read_to_string(&path).unwrap();
        assert!(
            gcode.contains("X99.000") && gcode.contains("Y88.000"),
            "G-code should contain travel to custom finish point (99, 88), got:\n{}",
            gcode.lines().rev().take(10).collect::<Vec<_>>().join("\n")
        );
    }

    #[test]
    fn export_honors_finish_position_when_optimization_is_disabled() {
        let ctx = create_test_ctx_with_project();
        mutate_project_optimization(&ctx, |opt| {
            opt.enabled = false;
            opt.finish_position = FinishPosition::CustomXY;
            opt.finish_x = Some(77.0);
            opt.finish_y = Some(66.0);
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("disabled-finish.gcode");
        export_gcode_to_path(&ctx, &path).unwrap();

        let gcode = std::fs::read_to_string(&path).unwrap();
        assert!(
            gcode.contains("X77.000") && gcode.contains("Y66.000"),
            "Optimization disabled should not suppress finish-position travel, got:\n{}",
            gcode.lines().rev().take(10).collect::<Vec<_>>().join("\n")
        );
    }

    #[test]
    fn export_with_dont_move_omits_return_to_origin() {
        let ctx = create_test_ctx_with_project();
        mutate_project_optimization(&ctx, |opt| {
            opt.finish_position = FinishPosition::DontMove;
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.gcode");
        export_gcode_to_path(&ctx, &path).unwrap();

        let gcode = std::fs::read_to_string(&path).unwrap();
        let last_lines: Vec<&str> = gcode.lines().rev().take(5).collect();
        // Should NOT have a "G0 X0.000 Y0.000" at the end
        let has_origin_return = last_lines
            .iter()
            .any(|l| l.contains("X0.000") && l.contains("Y0.000") && l.starts_with("G0"));
        assert!(
            !has_origin_return,
            "DontMove should not add return-to-origin, but found it in last lines: {:?}",
            last_lines
        );
    }

    #[test]
    fn export_with_origin_finish_emits_single_return_to_origin() {
        let ctx = create_test_ctx_with_project();
        mutate_project_optimization(&ctx, |opt| {
            opt.finish_position = FinishPosition::Origin;
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.gcode");
        export_gcode_to_path(&ctx, &path).unwrap();

        let gcode = std::fs::read_to_string(&path).unwrap();
        let origin_returns = gcode
            .lines()
            .filter(|line| {
                line.starts_with("G0")
                    && line.contains("X0")
                    && line.contains("Y0")
                    && !line.contains("X10")
                    && !line.contains("Y10")
            })
            .count();
        assert_eq!(
            origin_returns, 1,
            "Origin finish should be represented by the planner once, not duplicated by the G-code postamble:\n{}",
            gcode
        );
    }

    #[test]
    fn preview_sequence_order_matches_plan_segment_order() {
        // Use a manual-ordering flag (`direction_order: TopDown`) so the
        // plan's per-layer polyline sequence is deterministic and not
        // subject to nearest-neighbor reshuffling. The preview must
        // faithfully mirror that sequence regardless of which flag
        // forced the ordering.
        let ctx = create_test_ctx_with_project();
        mutate_project_optimization(&ctx, |opt| {
            opt.direction_order = DirectionOrder::TopDown;
        });

        let plan = generate_plan(&ctx).unwrap();
        let preview = generate_preview(&ctx).unwrap();

        // Collect plan vector segment starting positions in order
        let plan_vector_starts: Vec<(f64, f64)> = plan
            .segments
            .iter()
            .filter_map(|s| {
                if let PlanSegment::Vector { polyline, .. } = s {
                    Some((polyline[0].x, polyline[0].y))
                } else {
                    None
                }
            })
            .collect();

        // Collect preview vector paths sorted by sequence number
        let mut preview_vectors: Vec<(usize, f64, f64)> = preview
            .layers
            .iter()
            .flat_map(|l| &l.vector_paths)
            .map(|v| (v.sequence, v.points[0].x, v.points[0].y))
            .collect();
        preview_vectors.sort_by_key(|(seq, _, _)| *seq);
        let preview_vector_starts: Vec<(f64, f64)> =
            preview_vectors.iter().map(|(_, x, y)| (*x, *y)).collect();

        assert_eq!(
            plan_vector_starts.len(),
            preview_vector_starts.len(),
            "Plan and preview should have same number of vector segments"
        );
        for (i, (plan_pt, preview_pt)) in plan_vector_starts
            .iter()
            .zip(preview_vector_starts.iter())
            .enumerate()
        {
            assert!(
                (plan_pt.0 - preview_pt.0).abs() < 1e-6 && (plan_pt.1 - preview_pt.1).abs() < 1e-6,
                "Segment {i}: plan start ({:.1},{:.1}) != preview start ({:.1},{:.1})",
                plan_pt.0,
                plan_pt.1,
                preview_pt.0,
                preview_pt.1,
            );
        }
    }

    #[test]
    fn selected_only_job_options_filter_generated_plan_without_serializing() {
        let ctx = create_test_ctx_with_project();
        let selected_id = {
            let guard = ctx.project.lock().unwrap();
            guard.as_ref().unwrap().objects[0].id.to_string()
        };

        let all_plan = generate_plan(&ctx).unwrap();
        let selected_plan = generate_plan_with_options(
            &ctx,
            &SessionJobOptions {
                cut_selected_graphics: true,
                use_selection_origin: false,
                selected_object_ids: vec![selected_id],
            },
        )
        .unwrap();

        let vector_count = |plan: &beambench_planner::ExecutionPlan| {
            plan.segments
                .iter()
                .filter(|segment| matches!(segment, PlanSegment::Vector { .. }))
                .count()
        };
        assert!(
            vector_count(&selected_plan) < vector_count(&all_plan),
            "selected-only planning should emit fewer vector segments than the full project"
        );

        let project_after = current_project(&ctx).unwrap();
        assert_eq!(
            project_after.objects.len(),
            4,
            "session job options must not mutate or serialize into the open project"
        );
    }

    #[test]
    fn selected_only_job_options_reject_empty_selection() {
        let ctx = create_test_ctx_with_project();
        let err = generate_plan_with_options(
            &ctx,
            &SessionJobOptions {
                cut_selected_graphics: true,
                use_selection_origin: false,
                selected_object_ids: Vec::new(),
            },
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("Select at least one graphic"),
            "empty selection should produce a clear validation error, got {err}"
        );
    }

    #[test]
    fn selection_origin_requires_cut_selected_graphics() {
        let ctx = create_test_ctx_with_project();
        let selected_id = {
            let mut guard = ctx.project.lock().unwrap();
            let project = guard.as_mut().unwrap();
            project.job_origin = AnchorPoint::Center;
            project.objects[0].id.to_string()
        };

        let err = generate_plan_with_options(
            &ctx,
            &SessionJobOptions {
                cut_selected_graphics: false,
                use_selection_origin: true,
                selected_object_ids: vec![selected_id],
            },
        )
        .unwrap_err();

        assert_eq!(err.code, crate::error::ServiceErrorCode::InvalidInput);
        assert_eq!(
            err.message,
            "use_selection_origin requires cut_selected_graphics"
        );
    }

    #[test]
    fn settings_change_propagates_through_plan_preview_export() {
        // Build plan+preview+export with direction_order TopDown (forces
        // AsDrawn inside the planner via `effective_cut_strategy`), then
        // with reduce_travel=true and no manual ordering (runs
        // nearest-neighbor at both the per-layer and cross-layer
        // stages). Plan segment order, preview travel, and G-code
        // content should all differ.
        let ctx = create_test_ctx_with_project();

        // --- "AsDrawn"-equivalent ---
        mutate_project_optimization(&ctx, |opt| {
            opt.direction_order = DirectionOrder::TopDown;
            opt.reduce_travel = false;
        });
        let plan_a = generate_plan(&ctx).unwrap();
        let preview_a = generate_preview(&ctx).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path_a = dir.path().join("a.gcode");
        export_gcode_to_path(&ctx, &path_a).unwrap();
        let gcode_a = std::fs::read_to_string(&path_a).unwrap();

        // --- Full travel optimization ---
        mutate_project_optimization(&ctx, |opt| {
            opt.direction_order = DirectionOrder::None;
            opt.reduce_travel = true;
        });
        let plan_b = generate_plan(&ctx).unwrap();
        let preview_b = generate_preview(&ctx).unwrap();
        let path_b = dir.path().join("b.gcode");
        export_gcode_to_path(&ctx, &path_b).unwrap();
        let gcode_b = std::fs::read_to_string(&path_b).unwrap();

        // Plan segment ordering should differ
        let seg_starts = |plan: &beambench_planner::ExecutionPlan| -> Vec<String> {
            plan.segments
                .iter()
                .filter_map(|s| {
                    if let PlanSegment::Vector { polyline, .. } = s {
                        Some(format!("{:.0},{:.0}", polyline[0].x, polyline[0].y))
                    } else {
                        None
                    }
                })
                .collect()
        };
        assert_ne!(
            seg_starts(&plan_a),
            seg_starts(&plan_b),
            "Plan segment order should differ between manual-ordering and nearest-neighbor modes"
        );

        // Preview travel distances should differ
        assert_ne!(
            format!("{:.2}", preview_a.stats.travel_distance_mm),
            format!("{:.2}", preview_b.stats.travel_distance_mm),
            "Preview travel distance should change with optimization settings"
        );

        // G-code content should differ
        assert_ne!(
            gcode_a, gcode_b,
            "G-code output should change with optimization settings"
        );
    }

    #[test]
    fn plan_and_export_use_same_optimization_settings_source() {
        let ctx = create_test_ctx_with_project();
        mutate_project_optimization(&ctx, |opt| {
            opt.direction_order = DirectionOrder::TopDown;
            opt.reduce_travel = true;
        });

        let plan = generate_plan(&ctx).unwrap();
        assert!(!plan.segments.is_empty(), "Plan should have segments");

        // Export should also work with the same settings
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.gcode");
        let result = export_gcode_to_path(&ctx, &path);
        assert!(result.is_ok(), "Export should succeed with same settings");

        let gcode = std::fs::read_to_string(&path).unwrap();
        assert!(!gcode.is_empty(), "G-code should not be empty");
    }
}
