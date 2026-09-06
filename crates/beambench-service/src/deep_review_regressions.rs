// Regressions for the deeper project review.
use crate::{ServiceContext, ops::planning};
use beambench_common::{Bounds, Point2D};
use beambench_core::{ObjectData, Project, ProjectObject, ShapeKind};
#[test]
fn invalid_mask_prevents_gcode_export() {
    use beambench_core::object::{ImageMaskPolarity, ImageMaskRef};
    use beambench_core::{Asset, AssetMediaType, Layer, OperationType};
    let ctx = ServiceContext::with_settings(beambench_core::AppSettings::default());
    let mut project = Project::new("masked");
    let layer = Layer::new("image", OperationType::Image);
    let layer_id = layer.id;
    project.layers.push(layer);
    let mut mask_layer_def = Layer::new("mask", OperationType::Line);
    mask_layer_def.enabled = false;
    let mask_layer = mask_layer_def.id;
    project.layers.push(mask_layer_def);
    let mask = ProjectObject::new(
        "mask",
        mask_layer,
        Bounds::new(Point2D::new(20.0, 20.0), Point2D::new(25.0, 30.0)),
        ObjectData::Shape {
            kind: ShapeKind::Rectangle,
            width: 5.0,
            height: 10.0,
            corner_radius: 0.0,
        },
    );
    let mask_id = mask.id;
    project.add_object(mask);
    let png = vec![
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 10, 0, 0, 0, 10, 8,
        0, 0, 0, 0, 168, 89, 144, 97, 0, 0, 0, 12, 73, 68, 65, 84, 120, 156, 99, 96, 160, 39, 0, 0,
        0, 110, 0, 1, 72, 93, 122, 99, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let asset = Asset::new(
        "black.png",
        AssetMediaType::Png,
        png.len() as u64,
        Some(10),
        Some(10),
    );
    let asset_id = asset.id;
    project.add_asset(asset, png);
    let object = ProjectObject::new(
        "image",
        layer_id,
        Bounds::new(Point2D::new(20.0, 20.0), Point2D::new(30.0, 30.0)),
        ObjectData::RasterImage {
            asset_key: asset_id.to_string(),
            original_width_px: 10,
            original_height_px: 10,
            adjustments: None,
            masks: vec![ImageMaskRef {
                object_id: mask_id,
                polarity: ImageMaskPolarity::KeepInside,
            }],
        },
    );
    project.add_object(object);
    project.objects[0].data = ObjectData::VectorPath {
        path_data: "M20 20 L25 30".into(),
        closed: false,
        ruler_guide_axis: None,
    };
    *ctx.project.lock().unwrap() = Some(project);
    let plan = planning::generate_plan(&ctx).unwrap();
    assert!(!plan.failed_entries.is_empty());
    let path = std::env::temp_dir().join(format!(
        "beambench-deep-review-{}.gcode",
        std::process::id()
    ));
    assert!(planning::export_gcode_to_path_with_options(&ctx, &path, &Default::default()).is_err());
    assert!(!path.exists());
}

#[test]
fn compact_gcode_preserves_coordinates() {
    let spaced = beambench_core::import_gcode::parse_gcode("G1 X10 Y20");
    let compact = beambench_core::import_gcode::parse_gcode("G1X10Y20");
    assert_eq!(spaced[0].params.len(), 2);
    assert_eq!(compact[0].params, spaced[0].params);
    let multi = beambench_core::import_gcode::parse_gcode("G91 G1 X10");
    assert_eq!(multi[0].commands, ["G91", "G1"]);
}
#[test]
fn malformed_pdf_is_rejected() {
    // Blank page contents, but an unused Form XObject contains a painted rectangle.
    let pdf = b"%PDF-1.4\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R >> endobj\n4 0 obj << /Length 0 >> stream\n\nendstream endobj\n5 0 obj << /Type /XObject /Subtype /Form /BBox [0 0 10 10] /Length 16 >> stream\n0 0 10 10 re f\nendstream endobj\n%%EOF";
    assert!(beambench_core::import_pdf::parse_pdf_painted_paths(pdf).is_err());
}

#[test]
fn cancel_retains_job_after_reset_or_laser_off_write_fails() {
    use beambench_serial::{MockSerialTransport, SerialError, SerialTransport};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    struct FailingTransport {
        inner: MockSerialTransport,
        fail: Arc<AtomicBool>,
        failed: Arc<AtomicBool>,
        fail_reset: bool,
    }
    impl SerialTransport for FailingTransport {
        fn open(&mut self) -> Result<(), SerialError> {
            self.inner.open()
        }
        fn close(&mut self) -> Result<(), SerialError> {
            self.inner.close()
        }
        fn is_open(&self) -> bool {
            self.inner.is_open()
        }
        fn write_bytes(&mut self, data: &[u8]) -> Result<usize, SerialError> {
            if self.fail_reset && self.fail.load(Ordering::SeqCst) {
                self.failed.store(true, Ordering::SeqCst);
                return Err(SerialError::WriteFailed("injected reset failure".into()));
            }
            self.inner.write_bytes(data)
        }
        fn write_line(&mut self, line: &str) -> Result<(), SerialError> {
            if !self.fail_reset && line == "M5" && self.fail.load(Ordering::SeqCst) {
                self.failed.store(true, Ordering::SeqCst);
                return Err(SerialError::WriteFailed("injected M5 failure".into()));
            }
            self.inner.write_line(line)
        }
        fn read_available(&mut self) -> Result<Vec<u8>, SerialError> {
            self.inner.read_available()
        }
        fn read_line(&mut self) -> Result<Option<String>, SerialError> {
            self.inner.read_line()
        }
        fn flush(&mut self) -> Result<(), SerialError> {
            self.inner.flush()
        }
        fn port_name(&self) -> &str {
            self.inner.port_name()
        }
    }
    for fail_reset in [true, false] {
        let ctx = ServiceContext::with_settings(beambench_core::AppSettings::default());
        let mut project = Project::new("cancel probe");
        let layer = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "rectangle",
            layer,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(20.0, 20.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        *ctx.project.lock().unwrap() = Some(project);
        let plan = planning::generate_plan(&ctx).unwrap();
        let fail = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let mut inner = MockSerialTransport::new("review-mock");
        inner.enqueue_response("Grbl 1.1h");
        let mut session = beambench_grbl::GrblSession::new(Box::new(FailingTransport {
            inner,
            fail: fail.clone(),
            failed: failed.clone(),
            fail_reset,
        }));
        session.connect().unwrap();
        session.poll().unwrap();
        session.mark_ready().unwrap();
        let mut job =
            beambench_streamer::JobController::prepare(&plan, &Default::default()).unwrap();
        job.start(&mut session).unwrap();
        *ctx.job.lock().unwrap() = Some(crate::runtime::ActiveJobHandle::Grbl(job));
        *ctx.session.lock().unwrap() =
            Some(crate::runtime::MachineSessionHandle::Grbl(session.into()));
        fail.store(true, Ordering::SeqCst);
        assert!(crate::ops::machine::cancel_job(&ctx).is_err());
        assert!(failed.load(Ordering::SeqCst));
        assert!(ctx.job.lock().unwrap().is_some());
        fail.store(false, Ordering::SeqCst);
        crate::ops::machine::cancel_job(&ctx).unwrap();
        assert!(ctx.job.lock().unwrap().is_none());
    }
}
