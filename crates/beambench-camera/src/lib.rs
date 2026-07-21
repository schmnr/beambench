use std::f64::consts::PI;
use std::fs;
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::sync::{Mutex, OnceLock};
#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};

use beambench_common::{
    AlignmentPointSet, CalibrationPointSet, CalibrationSolveResult, CameraAlignment,
    CameraAlignmentSource, CameraBackendKind, CameraCalibration, CameraDeviceInfo,
    CameraFrameHandle, SimilarityTransform,
};
use chrono::Utc;
use image::{ImageBuffer, Rgba, RgbaImage};
#[cfg(not(target_os = "macos"))]
use nokhwa::{
    Camera,
    pixel_format::RgbFormat,
    utils::{RequestedFormat, RequestedFormatType},
};
use tracing::warn;
use uuid::Uuid;

const NATIVE_CAMERA_PREFIX: &str = "camera-native:";
#[cfg(target_os = "macos")]
const MACOS_NATIVE_DEVICE_CACHE_TTL: Duration = Duration::from_secs(2);
const DEFAULT_CAMERAS: [(&str, &str, u32, u32); 2] = [
    ("camera-mock-overhead", "Mock Overhead Camera", 1920, 1080),
    ("camera-mock-closeup", "Mock Closeup Camera", 1280, 720),
];

#[cfg(target_os = "macos")]
#[derive(Clone)]
struct MacosNativeDeviceCache {
    devices: Vec<CameraDeviceInfo>,
    updated_at: Instant,
}

#[cfg(target_os = "macos")]
static MACOS_NATIVE_DEVICE_CACHE: OnceLock<Mutex<Option<MacosNativeDeviceCache>>> = OnceLock::new();

fn default_devices() -> Vec<CameraDeviceInfo> {
    DEFAULT_CAMERAS
        .into_iter()
        .map(
            |(camera_id, display_name, width_px, height_px)| CameraDeviceInfo {
                camera_id: camera_id.to_string(),
                display_name: display_name.to_string(),
                backend_kind: CameraBackendKind::MockSnapshot,
                available: true,
                width_px,
                height_px,
                status_text: "Ready".to_string(),
            },
        )
        .collect()
}

fn mock_cameras_enabled() -> bool {
    cfg!(debug_assertions)
        || std::env::var("BEAMBENCH_ENABLE_MOCK_CAMERAS")
            .ok()
            .is_some_and(|value| {
                matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes")
            })
}

#[cfg(not(target_os = "macos"))]
fn native_camera_id(info: &nokhwa::utils::CameraInfo) -> String {
    format!("{NATIVE_CAMERA_PREFIX}{}", info.index().as_string())
}

#[cfg(target_os = "macos")]
fn native_camera_slug(display_name: &str) -> String {
    let mut slug = String::new();
    for ch in display_name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }

    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "camera".to_string()
    } else {
        slug.to_string()
    }
}

#[cfg(target_os = "macos")]
fn query_macos_native_devices() -> Vec<CameraDeviceInfo> {
    let output = match std::process::Command::new("system_profiler")
        .args(["-json", "SPCameraDataType"])
        .output()
    {
        Ok(output) => output,
        Err(err) => {
            warn!("Failed to run system_profiler SPCameraDataType for camera discovery: {err}");
            return Vec::new();
        }
    };
    if !output.status.success() {
        warn!(
            status = ?output.status,
            stderr = %String::from_utf8_lossy(&output.stderr),
            "system_profiler SPCameraDataType failed during camera discovery",
        );
        return Vec::new();
    }

    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        warn!("system_profiler SPCameraDataType returned invalid JSON during camera discovery");
        return Vec::new();
    };
    let Some(devices) = value
        .get("SPCameraDataType")
        .and_then(serde_json::Value::as_array)
    else {
        warn!("system_profiler SPCameraDataType JSON did not contain a camera device array");
        return Vec::new();
    };

    let mut counts = std::collections::HashMap::<String, usize>::new();
    devices
        .iter()
        .filter_map(|info| {
            let display_name = info.get("_name")?.as_str()?.trim().to_string();
            if display_name.is_empty() {
                return None;
            }
            let stable_name = info
                .get("spcamera_unique-id")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(&display_name);
            let base_slug = native_camera_slug(stable_name);
            let count = counts.entry(base_slug.clone()).or_insert(0);
            *count += 1;
            let camera_slug = if *count == 1 {
                base_slug
            } else {
                format!("{base_slug}-{count}")
            };
            Some(CameraDeviceInfo {
                camera_id: format!("{NATIVE_CAMERA_PREFIX}{camera_slug}"),
                display_name: display_name.to_string(),
                backend_kind: CameraBackendKind::Native,
                available: true,
                width_px: 0,
                height_px: 0,
                status_text: "Ready".to_string(),
            })
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn native_devices() -> Vec<CameraDeviceInfo> {
    let cache = MACOS_NATIVE_DEVICE_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache.lock()
        && let Some(cached) = guard.as_ref()
        && cached.updated_at.elapsed() < MACOS_NATIVE_DEVICE_CACHE_TTL
    {
        return cached.devices.clone();
    }

    let devices = query_macos_native_devices();
    if let Ok(mut guard) = cache.lock() {
        *guard = Some(MacosNativeDeviceCache {
            devices: devices.clone(),
            updated_at: Instant::now(),
        });
    }
    devices
}

#[cfg(not(target_os = "macos"))]
fn native_devices() -> Vec<CameraDeviceInfo> {
    let Some(backend) = nokhwa::native_api_backend() else {
        warn!("No native camera backend is available");
        return Vec::new();
    };
    let devices = match nokhwa::query(backend) {
        Ok(devices) => devices,
        Err(err) => {
            warn!("Failed to query native cameras: {err}");
            return Vec::new();
        }
    };

    devices
        .into_iter()
        .enumerate()
        .map(|(idx, info)| {
            let human_name = info.human_name();
            let display_name = if human_name.trim().is_empty() {
                format!("Camera {}", idx + 1)
            } else {
                human_name
            };
            CameraDeviceInfo {
                camera_id: native_camera_id(&info),
                display_name,
                backend_kind: CameraBackendKind::Native,
                available: true,
                width_px: 0,
                height_px: 0,
                status_text: "Ready".to_string(),
            }
        })
        .collect()
}

pub fn list_cameras() -> Vec<CameraDeviceInfo> {
    let mut devices = native_devices();
    if mock_cameras_enabled() {
        devices.extend(default_devices());
    }
    devices
}

pub fn get_camera(camera_id: &str) -> Result<CameraDeviceInfo, String> {
    list_cameras()
        .into_iter()
        .find(|camera| camera.camera_id == camera_id)
        .ok_or_else(|| format!("Camera '{camera_id}' not found"))
}

#[cfg(not(target_os = "macos"))]
fn native_camera_info_for_id(camera_id: &str) -> Result<nokhwa::utils::CameraInfo, String> {
    let Some(backend) = nokhwa::native_api_backend() else {
        return Err("Native camera capture is not supported on this platform".to_string());
    };
    nokhwa::query(backend)
        .map_err(|e| format!("Failed to list native cameras: {e}"))?
        .into_iter()
        .find(|info| native_camera_id(info) == camera_id)
        .ok_or_else(|| format!("Camera '{camera_id}' not found"))
}

fn frame_dir() -> PathBuf {
    std::env::temp_dir().join("beam-bench-camera")
}

fn draw_mock_frame(camera: &CameraDeviceInfo) -> RgbaImage {
    let mut img: RgbaImage = ImageBuffer::from_pixel(
        camera.width_px,
        camera.height_px,
        Rgba([245, 248, 252, 255]),
    );

    let major_color = Rgba([64, 122, 201, 255]);
    let minor_color = Rgba([210, 219, 234, 255]);
    let accent = Rgba([241, 93, 57, 255]);

    for x in 0..camera.width_px {
        for y in 0..camera.height_px {
            if x % 160 == 0 || y % 160 == 0 {
                img.put_pixel(x, y, major_color);
            } else if x % 40 == 0 || y % 40 == 0 {
                img.put_pixel(x, y, minor_color);
            }
        }
    }

    let center_x = camera.width_px / 2;
    let center_y = camera.height_px / 2;
    let cross_half = (camera.width_px.min(camera.height_px) / 8).max(20);
    for dx in 0..=cross_half {
        if center_x + dx < camera.width_px {
            img.put_pixel(center_x + dx, center_y, accent);
        }
        if center_x >= dx {
            img.put_pixel(center_x - dx, center_y, accent);
        }
    }
    for dy in 0..=cross_half {
        if center_y + dy < camera.height_px {
            img.put_pixel(center_x, center_y + dy, accent);
        }
        if center_y >= dy {
            img.put_pixel(center_x, center_y - dy, accent);
        }
    }

    img
}

fn capture_mock_frame(camera: &CameraDeviceInfo) -> Result<CameraFrameHandle, String> {
    let dir = frame_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create frame directory: {e}"))?;
    let handle_id = Uuid::new_v4().to_string();
    let file_path = dir.join(format!("{handle_id}.png"));
    let img = draw_mock_frame(camera);
    img.save(&file_path)
        .map_err(|e| format!("Failed to save camera frame: {e}"))?;

    Ok(CameraFrameHandle {
        handle_id,
        file_path: file_path.to_string_lossy().to_string(),
        width_px: camera.width_px,
        height_px: camera.height_px,
        media_type: "image/png".to_string(),
        captured_at: Utc::now().to_rfc3339(),
    })
}

fn capture_native_frame(camera: &CameraDeviceInfo) -> Result<CameraFrameHandle, String> {
    #[cfg(target_os = "macos")]
    {
        let _ = camera;
        Err(
            "Native camera capture on macOS is handled by the in-app camera capture path"
                .to_string(),
        )
    }

    #[cfg(not(target_os = "macos"))]
    {
        let info = native_camera_info_for_id(&camera.camera_id)?;
        let Some(backend) = nokhwa::native_api_backend() else {
            return Err("Native camera capture is not supported on this platform".to_string());
        };
        let requested =
            RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestResolution);
        let mut native_camera = Camera::with_backend(info.index().clone(), requested, backend)
            .map_err(|e| {
                format!(
                    "Failed to open native camera '{}': {e}",
                    camera.display_name
                )
            })?;
        native_camera.open_stream().map_err(|e| {
            format!(
                "Failed to start native camera '{}': {e}",
                camera.display_name
            )
        })?;
        let frame = native_camera
            .frame()
            .map_err(|e| format!("Failed to capture native camera frame: {e}"))?;
        let img = frame
            .decode_image::<RgbFormat>()
            .map_err(|e| format!("Failed to decode native camera frame: {e}"))?;

        let dir = frame_dir();
        fs::create_dir_all(&dir).map_err(|e| format!("Failed to create frame directory: {e}"))?;
        let handle_id = Uuid::new_v4().to_string();
        let file_path = dir.join(format!("{handle_id}.png"));
        img.save(&file_path)
            .map_err(|e| format!("Failed to save camera frame: {e}"))?;

        Ok(CameraFrameHandle {
            handle_id,
            file_path: file_path.to_string_lossy().to_string(),
            width_px: img.width(),
            height_px: img.height(),
            media_type: "image/png".to_string(),
            captured_at: Utc::now().to_rfc3339(),
        })
    }
}

pub fn capture_frame_for_device(camera: &CameraDeviceInfo) -> Result<CameraFrameHandle, String> {
    match camera.backend_kind {
        CameraBackendKind::MockSnapshot => capture_mock_frame(camera),
        CameraBackendKind::Native => capture_native_frame(camera),
    }
}

pub fn capture_frame(camera_id: &str) -> Result<CameraFrameHandle, String> {
    let camera = get_camera(camera_id)?;
    capture_frame_for_device(&camera)
}

#[derive(Debug, Clone, Copy)]
struct SimilaritySolution {
    scale: f64,
    rotation_rad: f64,
    tx: f64,
    ty: f64,
    rmse: f64,
}

fn quality_score(rmse: f64) -> f64 {
    (1.0 / (1.0 + rmse.max(0.0))).clamp(0.0, 1.0)
}

fn solve_similarity(src: &[(f64, f64)], dst: &[(f64, f64)]) -> Result<SimilaritySolution, String> {
    if src.len() != dst.len() {
        return Err("Point counts do not match".to_string());
    }
    if src.len() < 3 {
        return Err("At least three point pairs are required".to_string());
    }
    if src
        .iter()
        .chain(dst.iter())
        .any(|(x, y)| !x.is_finite() || !y.is_finite())
    {
        return Err("Calibration points must contain finite coordinates".to_string());
    }
    if !points_span_area(src) {
        return Err("Source points must span a non-zero area".to_string());
    }
    if !points_span_area(dst) {
        return Err("Destination points must span a non-zero area".to_string());
    }

    let count = src.len() as f64;
    let (src_cx, src_cy) = src
        .iter()
        .fold((0.0, 0.0), |(ax, ay), (x, y)| (ax + x, ay + y));
    let (dst_cx, dst_cy) = dst
        .iter()
        .fold((0.0, 0.0), |(ax, ay), (x, y)| (ax + x, ay + y));
    let src_cx = src_cx / count;
    let src_cy = src_cy / count;
    let dst_cx = dst_cx / count;
    let dst_cy = dst_cy / count;

    let mut a = 0.0;
    let mut b = 0.0;
    let mut denom = 0.0;
    for ((sx, sy), (dx, dy)) in src.iter().zip(dst.iter()) {
        let sx = sx - src_cx;
        let sy = sy - src_cy;
        let dx = dx - dst_cx;
        let dy = dy - dst_cy;
        a += sx * dx + sy * dy;
        b += sx * dy - sy * dx;
        denom += sx * sx + sy * sy;
    }
    if denom.abs() < f64::EPSILON {
        return Err("Source points are degenerate".to_string());
    }

    let scale = (a.hypot(b)) / denom;
    if !scale.is_finite() || scale <= f64::EPSILON {
        return Err("Calibration produced an invalid scale".to_string());
    }
    let rotation_rad = b.atan2(a);
    let cos_r = rotation_rad.cos();
    let sin_r = rotation_rad.sin();
    let tx = dst_cx - scale * (cos_r * src_cx - sin_r * src_cy);
    let ty = dst_cy - scale * (sin_r * src_cx + cos_r * src_cy);

    let rmse = (src
        .iter()
        .zip(dst.iter())
        .map(|((sx, sy), (dx, dy))| {
            let x = scale * (cos_r * sx - sin_r * sy) + tx;
            let y = scale * (sin_r * sx + cos_r * sy) + ty;
            let ex = x - dx;
            let ey = y - dy;
            ex * ex + ey * ey
        })
        .sum::<f64>()
        / count)
        .sqrt();

    Ok(SimilaritySolution {
        scale,
        rotation_rad,
        tx,
        ty,
        rmse,
    })
}

fn points_span_area(points: &[(f64, f64)]) -> bool {
    let mut max_distance_squared = 0.0_f64;
    for (index, first) in points.iter().enumerate() {
        for second in points.iter().skip(index + 1) {
            max_distance_squared = max_distance_squared
                .max((first.0 - second.0).powi(2) + (first.1 - second.1).powi(2));
        }
    }
    if max_distance_squared <= f64::EPSILON {
        return false;
    }

    for first_index in 0..points.len() {
        for second_index in (first_index + 1)..points.len() {
            for third_index in (second_index + 1)..points.len() {
                let first = points[first_index];
                let second = points[second_index];
                let third = points[third_index];
                let twice_area = (second.0 - first.0) * (third.1 - first.1)
                    - (second.1 - first.1) * (third.0 - first.0);
                if twice_area.abs() > max_distance_squared * 1e-9 {
                    return true;
                }
            }
        }
    }
    false
}

pub fn solve_calibration(points: &CalibrationPointSet) -> Result<CalibrationSolveResult, String> {
    if points.image_width_px == 0 || points.image_height_px == 0 {
        return Err("Camera frame dimensions must be greater than zero".to_string());
    }
    if points.points.iter().any(|point| {
        point.image_x < 0.0
            || point.image_y < 0.0
            || point.image_x > f64::from(points.image_width_px)
            || point.image_y > f64::from(points.image_height_px)
    }) {
        return Err("Image calibration points must fall within the captured frame".to_string());
    }
    let src = points
        .points
        .iter()
        .map(|point| (point.image_x, point.image_y))
        .collect::<Vec<_>>();
    let dst = points
        .points
        .iter()
        .map(|point| (point.reference_x, point.reference_y))
        .collect::<Vec<_>>();
    let solution = solve_similarity(&src, &dst)?;
    let calibration = CameraCalibration {
        image_width_px: points.image_width_px,
        image_height_px: points.image_height_px,
        transform: SimilarityTransform {
            scale: solution.scale,
            rotation_deg: solution.rotation_rad * 180.0 / PI,
            translation_x: solution.tx,
            translation_y: solution.ty,
        },
        rmse_px: solution.rmse,
        quality_score: quality_score(solution.rmse),
        solved_at: Utc::now().to_rfc3339(),
    };
    Ok(CalibrationSolveResult {
        calibration,
        point_count: points.points.len(),
    })
}

pub fn solve_alignment(points: &AlignmentPointSet) -> Result<CameraAlignment, String> {
    let src = points
        .points
        .iter()
        .map(|point| (point.camera_x, point.camera_y))
        .collect::<Vec<_>>();
    let dst = points
        .points
        .iter()
        .map(|point| (point.workspace_x_mm, point.workspace_y_mm))
        .collect::<Vec<_>>();
    let solution = solve_similarity(&src, &dst)?;
    Ok(CameraAlignment {
        transform: SimilarityTransform {
            scale: solution.scale,
            rotation_deg: solution.rotation_rad * 180.0 / PI,
            translation_x: solution.tx,
            translation_y: solution.ty,
        },
        image_width_px: None,
        image_height_px: None,
        rmse_mm: solution.rmse,
        quality_score: quality_score(solution.rmse),
        solved_at: Utc::now().to_rfc3339(),
        source: CameraAlignmentSource::SolvedPoints,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::{AlignmentPoint, CalibrationPoint};

    #[test]
    fn list_cameras_returns_devices() {
        let devices = list_cameras();
        assert_eq!(
            devices
                .iter()
                .any(|device| device.backend_kind == CameraBackendKind::MockSnapshot),
            mock_cameras_enabled()
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_camera_slug_preserves_stable_unique_ids() {
        assert_eq!(
            native_camera_slug("6C707041-05AC-0010-0008-000000000001"),
            "6c707041-05ac-0010-0008-000000000001"
        );
    }

    #[test]
    fn capture_frame_writes_png() {
        let handle = capture_frame_for_device(&default_devices()[0]).unwrap();
        assert!(handle.file_path.ends_with(".png"));
        assert!(std::path::Path::new(&handle.file_path).exists());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_capture_on_macos_is_handled_by_browser_path() {
        let err = capture_native_frame(&CameraDeviceInfo {
            camera_id: "camera-native:test".to_string(),
            display_name: "Test Camera".to_string(),
            backend_kind: CameraBackendKind::Native,
            available: true,
            width_px: 0,
            height_px: 0,
            status_text: "Ready".to_string(),
        })
        .unwrap_err();

        assert_eq!(
            err,
            "Native camera capture on macOS is handled by the in-app camera capture path"
        );
    }

    #[test]
    fn solve_calibration_returns_quality() {
        let result = solve_calibration(&CalibrationPointSet {
            image_width_px: 1000,
            image_height_px: 1000,
            points: vec![
                CalibrationPoint {
                    image_x: 0.0,
                    image_y: 0.0,
                    reference_x: 10.0,
                    reference_y: 20.0,
                },
                CalibrationPoint {
                    image_x: 100.0,
                    image_y: 0.0,
                    reference_x: 20.0,
                    reference_y: 20.0,
                },
                CalibrationPoint {
                    image_x: 0.0,
                    image_y: 100.0,
                    reference_x: 10.0,
                    reference_y: 30.0,
                },
            ],
        })
        .unwrap();
        assert_eq!(result.point_count, 3);
        assert!(result.calibration.quality_score > 0.0);
    }

    #[test]
    fn solve_alignment_returns_transform() {
        let alignment = solve_alignment(&AlignmentPointSet {
            points: vec![
                AlignmentPoint {
                    camera_x: 0.0,
                    camera_y: 0.0,
                    workspace_x_mm: 5.0,
                    workspace_y_mm: 8.0,
                },
                AlignmentPoint {
                    camera_x: 10.0,
                    camera_y: 0.0,
                    workspace_x_mm: 15.0,
                    workspace_y_mm: 8.0,
                },
                AlignmentPoint {
                    camera_x: 0.0,
                    camera_y: 10.0,
                    workspace_x_mm: 5.0,
                    workspace_y_mm: 18.0,
                },
            ],
        })
        .unwrap();
        assert!((alignment.transform.translation_x - 5.0).abs() < 0.001);
        assert!((alignment.transform.translation_y - 8.0).abs() < 0.001);
    }

    #[test]
    fn solve_alignment_rejects_two_or_degenerate_point_sets() {
        let two_points = solve_alignment(&AlignmentPointSet {
            points: vec![
                AlignmentPoint {
                    camera_x: 0.0,
                    camera_y: 0.0,
                    workspace_x_mm: 0.0,
                    workspace_y_mm: 0.0,
                },
                AlignmentPoint {
                    camera_x: 10.0,
                    camera_y: 0.0,
                    workspace_x_mm: 10.0,
                    workspace_y_mm: 0.0,
                },
            ],
        })
        .unwrap_err();
        assert_eq!(two_points, "At least three point pairs are required");

        let degenerate = solve_alignment(&AlignmentPointSet {
            points: vec![
                AlignmentPoint {
                    camera_x: 0.0,
                    camera_y: 0.0,
                    workspace_x_mm: 5.0,
                    workspace_y_mm: 5.0,
                },
                AlignmentPoint {
                    camera_x: 10.0,
                    camera_y: 0.0,
                    workspace_x_mm: 5.0,
                    workspace_y_mm: 5.0,
                },
                AlignmentPoint {
                    camera_x: 0.0,
                    camera_y: 10.0,
                    workspace_x_mm: 5.0,
                    workspace_y_mm: 5.0,
                },
            ],
        })
        .unwrap_err();
        assert_eq!(degenerate, "Destination points must span a non-zero area");
    }
}
