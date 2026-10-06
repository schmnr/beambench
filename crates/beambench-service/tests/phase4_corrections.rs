use beambench_common::{Bounds, Point2D, Transform2D};
use beambench_core::vector::convert::object_to_world_vecpath;
use beambench_core::{ObjectData, ObjectId, Project, ProjectObject};
use beambench_service::{
    ServiceContext,
    ops::{project as edits, vector},
};

fn setup(data: &str, bounds: Bounds, transform: Transform2D) -> (ServiceContext, ObjectId) {
    let ctx = ServiceContext::new();
    let mut project = Project::new("audit");
    let layer = project.ensure_default_layer();
    let mut object = ProjectObject::new(
        "path",
        layer,
        bounds,
        ObjectData::VectorPath {
            path_data: data.into(),
            closed: false,
            ruler_guide_axis: None,
        },
    );
    object.transform = transform;
    let id = object.id;
    project.add_object(object);
    *ctx.project.lock().unwrap() = Some(project);
    (ctx, id)
}
fn bounds(x: f64, y: f64, w: f64, h: f64) -> Bounds {
    Bounds::new(Point2D::new(x, y), Point2D::new(x + w, y + h))
}

#[test]
fn nonzero_intrinsic_origin_stays_in_place() {
    let (ctx, id) = setup(
        "M10 20 L20 20 L20 30",
        bounds(100., 100., 20., 20.),
        Transform2D::identity(),
    );
    let edited = vector::update_node(
        &ctx,
        vector::UpdateNodeInput {
            object_id: id,
            subpath_idx: 0,
            command_idx: 1,
            x: 22.,
            y: 20.,
            handle_type: None,
        },
    )
    .unwrap();
    let b = object_to_world_vecpath(&edited).unwrap().bounds().unwrap();
    eprintln!("nonzero intrinsic edit bounds {b:?}");
    assert_eq!(
        b.min,
        Point2D::new(100., 100.),
        "untouched first node changed position"
    );
}

#[test]
fn second_node_edit_stays_in_place() {
    let (ctx, id) = setup(
        "M0 0 L10 0 L10 10",
        bounds(100., 100., 20., 20.),
        Transform2D::identity(),
    );
    vector::update_node(
        &ctx,
        vector::UpdateNodeInput {
            object_id: id,
            subpath_idx: 0,
            command_idx: 1,
            x: 12.,
            y: 0.,
            handle_type: None,
        },
    )
    .unwrap();
    let edited = vector::update_node(
        &ctx,
        vector::UpdateNodeInput {
            object_id: id,
            subpath_idx: 0,
            command_idx: 1,
            x: 126.,
            y: 100.,
            handle_type: None,
        },
    )
    .unwrap();
    let b = object_to_world_vecpath(&edited).unwrap().bounds().unwrap();
    eprintln!("second node edit bounds {b:?}");
    assert_eq!(b.min, Point2D::new(100., 100.));
}

#[test]
fn failed_edit_cannot_erase_a_concurrent_success() {
    use std::sync::{Arc, mpsc};
    use std::time::Duration;
    let (ctx, id) = setup(
        "M0 0 L10 0",
        bounds(0., 0., 10., 0.),
        Transform2D::identity(),
    );
    let ctx = Arc::new(ctx);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let failing_ctx = ctx.clone();
    let failing = std::thread::spawn(move || {
        let result: beambench_service::ServiceResult<()> = failing_ctx.atomic_edit(|| {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Err(beambench_service::ServiceError::invalid_input(
                "validation failure",
            ))
        });
        assert!(result.is_err());
    });
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let editing_ctx = ctx.clone();
    let editing = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        let result = edits::nudge_objects(&editing_ctx, &[id], 5., 0.);
        done_tx.send(result).unwrap();
    });
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let blocked = matches!(
        done_rx.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    );
    release_tx.send(()).unwrap();
    failing.join().unwrap();
    editing.join().unwrap();
    assert!(blocked, "another edit entered before rollback finished");
    done_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    assert_eq!(
        ctx.project
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .find_object(id)
            .unwrap()
            .bounds
            .min
            .x,
        5.
    );
    edits::undo_project(&ctx).unwrap();
    assert_eq!(
        ctx.project
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .find_object(id)
            .unwrap()
            .bounds
            .min
            .x,
        0.
    );
}

#[test]
fn deleting_nested_child_refreshes_ancestor_bounds() {
    let (ctx, a) = setup(
        "M0 0 L10 0",
        bounds(0., 0., 10., 0.),
        Transform2D::identity(),
    );
    let (b, c) = {
        let mut g = ctx.project.lock().unwrap();
        let p = g.as_mut().unwrap();
        let layer = p.layers[0].id;
        let add = |p: &mut Project, x| {
            p.add_object(ProjectObject::new(
                "s",
                layer,
                bounds(x, 0., 10., 0.),
                ObjectData::VectorPath {
                    path_data: "M0 0 L10 0".into(),
                    closed: false,
                    ruler_guide_axis: None,
                },
            ))
            .id
        };
        (add(p, 20.), add(p, 40.))
    };
    let inner = vector::group_objects(
        &ctx,
        vector::GroupObjectsInput {
            object_ids: vec![a, b],
        },
    )
    .unwrap();
    let outer = vector::group_objects(
        &ctx,
        vector::GroupObjectsInput {
            object_ids: vec![inner.id, c],
        },
    )
    .unwrap();
    edits::remove_object(&ctx, a).unwrap();
    let g = ctx.project.lock().unwrap();
    let p = g.as_ref().unwrap();
    eprintln!(
        "inner min {}, outer min {}",
        p.find_object(inner.id).unwrap().bounds.min.x,
        p.find_object(outer.id).unwrap().bounds.min.x
    );
    assert_eq!(p.find_object(outer.id).unwrap().bounds.min.x, 20.);
}

#[test]
fn failed_edit_preserves_full_undo_history() {
    let (ctx, id) = setup(
        "M0 0 L10 0",
        bounds(0., 0., 10., 0.),
        Transform2D::identity(),
    );
    for _ in 0..50 {
        edits::nudge_objects(&ctx, &[id], 1., 0.).unwrap();
    }
    assert!(
        vector::update_node(
            &ctx,
            vector::UpdateNodeInput {
                object_id: id,
                subpath_idx: 0,
                command_idx: 999,
                x: 1.,
                y: 1.,
                handle_type: None
            }
        )
        .is_err()
    );
    let mut undos = 0;
    while ctx.undo_state().unwrap().can_undo {
        edits::undo_project(&ctx).unwrap();
        undos += 1;
    }
    eprintln!("undo steps after 50 edits and one failed edit: {undos}");
    assert_eq!(undos, 50, "failed edit evicted a valid undo step");
}

#[test]
fn document_replacement_waits_until_failure_rollback_finishes() {
    use std::sync::{Arc, mpsc};
    use std::time::Duration;
    let (ctx, _) = setup(
        "M0 0 L10 0",
        bounds(0., 0., 10., 0.),
        Transform2D::identity(),
    );
    let ctx = Arc::new(ctx);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let failing_ctx = ctx.clone();
    let failing = std::thread::spawn(move || {
        let result: beambench_service::ServiceResult<()> = failing_ctx.atomic_edit(|| {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Err(beambench_service::ServiceError::invalid_input(
                "validation failure",
            ))
        });
        assert!(result.is_err());
    });
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let replacing_ctx = ctx.clone();
    let replacement = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        done_tx
            .send(edits::create_project(&replacing_ctx, "replacement"))
            .unwrap();
    });
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let blocked = matches!(
        done_rx.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    );
    release_tx.send(()).unwrap();
    failing.join().unwrap();
    replacement.join().unwrap();
    assert!(blocked, "replacement ran before rollback finished");
    let created = done_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    assert_eq!(
        ctx.project
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .metadata
            .project_id,
        created.metadata.project_id
    );
    assert!(!ctx.undo_state().unwrap().can_undo);
}
