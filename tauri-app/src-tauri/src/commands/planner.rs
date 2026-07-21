use std::sync::Arc;
use std::time::Instant;

use beambench_planner::{ExecutionPlan, PlanStats};
use beambench_service::ServiceContext;
use beambench_service::ops::planning::{self, SessionJobOptions};
use tauri::State;

/// Generate an execution plan from the current project.
/// Caches the result so subsequent get_plan_stats calls are fast.
#[tauri::command]
pub async fn generate_plan(
    svc: State<'_, Arc<ServiceContext>>,
    job_options: Option<SessionJobOptions>,
) -> Result<ExecutionPlan, String> {
    let svc = svc.inner().clone();
    let job_options = job_options.unwrap_or_default();
    let started_at = Instant::now();
    let plan = tokio::task::spawn_blocking(move || {
        planning::generate_plan_with_options(&svc, &job_options)
    })
    .await
    .map_err(|e| format!("Plan task failed: {e}"))?
    .map_err(|e| e.to_string())?;
    tracing::info!(target: "perf", operation = "generate_plan", duration_ms = started_at.elapsed().as_millis());
    Ok(plan)
}

/// Get stats for the current plan.
/// Returns cached stats only if the cache matches the current project revision.
/// Otherwise generates a fresh plan.
#[tauri::command]
pub async fn get_plan_stats(svc: State<'_, Arc<ServiceContext>>) -> Result<PlanStats, String> {
    let svc = svc.inner().clone();
    let stats = tokio::task::spawn_blocking(move || planning::get_plan_stats(&svc))
        .await
        .map_err(|e| format!("Plan stats task failed: {e}"))?
        .map_err(|e| e.to_string())?;
    Ok(stats)
}

#[tauri::command]
pub fn cancel_planning(svc: State<'_, Arc<ServiceContext>>) -> Result<(), String> {
    planning::cancel_planning(&svc).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::Id;
    use beambench_common::geometry::{Bounds, Point2D};
    use beambench_common::markers::ProjectMarker;
    use beambench_core::layer::{Layer, OperationType};
    use beambench_core::object::{ObjectData, ProjectObject, ShapeKind};
    use beambench_core::project::{Project, ProjectMetadata};
    use beambench_core::workspace::{Workspace, WorkspaceOrigin};
    use std::collections::HashMap;

    fn make_project(name: &str) -> Project {
        let mut metadata = ProjectMetadata::new(name);
        metadata.project_id = Id::<ProjectMarker>::new();
        Project {
            metadata,
            workspace: Workspace {
                bed_width_mm: 400.0,
                bed_height_mm: 300.0,
                origin: WorkspaceOrigin::TopLeft,
            },
            layers: vec![],
            objects: vec![],
            assets: vec![],
            machine_profile_id: None,
            machine_profile_snapshot: None,
            asset_data: HashMap::new(),
            dirty: false,
            notes: String::new(),
            start_from: Default::default(),
            job_origin: Default::default(),
            transform_locks: Default::default(),
            user_origin: None,
            optimization: Default::default(),
            material_height_mm: None,
        }
    }

    fn make_project_with_rect(name: &str) -> Project {
        let mut project = make_project(name);
        let layer = Layer::new("Lines", OperationType::Line);
        let layer_id = layer.id;
        project.layers.push(layer);
        project.objects.push(ProjectObject::new(
            "rect",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(50.0, 50.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 40.0,
                height: 40.0,
                corner_radius: 0.0,
            },
        ));
        project
    }

    #[test]
    fn cache_valid_when_project_unchanged() {
        let project = make_project_with_rect("Test");
        let plan = beambench_planner::build_plan(&project).unwrap();
        let svc = ServiceContext::new();

        *svc.plan_cache.lock().unwrap() = Some(plan.clone());

        assert!(planning::is_cached_plan_valid(&svc, &project));
    }

    #[test]
    fn cache_invalid_after_project_mutation() {
        let mut project = make_project_with_rect("Test");
        let plan = beambench_planner::build_plan(&project).unwrap();
        let svc = ServiceContext::new();

        *svc.plan_cache.lock().unwrap() = Some(plan.clone());

        // Mutate the project — add a second object
        let layer_id = project.layers[0].id;
        project.objects.push(ProjectObject::new(
            "rect2",
            layer_id,
            Bounds::new(Point2D::new(60.0, 60.0), Point2D::new(90.0, 90.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 30.0,
                height: 30.0,
                corner_radius: 0.0,
            },
        ));

        assert!(!planning::is_cached_plan_valid(&svc, &project));
    }

    #[test]
    fn cache_invalid_for_different_project() {
        let project_a = make_project_with_rect("Project A");
        let plan = beambench_planner::build_plan(&project_a).unwrap();

        let svc = ServiceContext::new();
        *svc.plan_cache.lock().unwrap() = Some(plan);

        // Different project (different project_id)
        let project_b = make_project_with_rect("Project B");

        assert!(!planning::is_cached_plan_valid(&svc, &project_b));
    }

    #[test]
    fn cache_invalid_when_empty() {
        let project = make_project_with_rect("Test");
        let svc = ServiceContext::new();

        assert!(!planning::is_cached_plan_valid(&svc, &project));
    }

    #[test]
    fn revision_hash_matches_plan_hash() {
        let project = make_project_with_rect("Test");
        let plan = beambench_planner::build_plan(&project).unwrap();
        let hash = planning::revision_hash(&project).unwrap();

        assert_eq!(hash, plan.revision_hash);
    }
}
