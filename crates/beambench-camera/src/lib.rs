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
    CameraFrameHandle, CameraImageWarp, SimilarityTransform,
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

fn solve_linear_8(mut matrix: [[f64; 8]; 8], mut rhs: [f64; 8]) -> Option<[f64; 8]> {
    for column in 0..8 {
        let mut pivot = column;
        for row in (column + 1)..8 {
            if matrix[row][column].abs() > matrix[pivot][column].abs() {
                pivot = row;
            }
        }
        if !matrix[pivot][column].is_finite() || matrix[pivot][column].abs() < 1e-10 {
            return None;
        }
        if pivot != column {
            matrix.swap(pivot, column);
            rhs.swap(pivot, column);
        }

        let divisor = matrix[column][column];
        for value in &mut matrix[column][column..] {
            *value /= divisor;
        }
        rhs[column] /= divisor;

        for row in 0..8 {
            if row == column {
                continue;
            }
            let factor = matrix[row][column];
            if factor.abs() < f64::EPSILON {
                continue;
            }
            let pivot_values = matrix[column];
            for (value, pivot_value) in matrix[row][column..]
                .iter_mut()
                .zip(&pivot_values[column..])
            {
                *value -= factor * pivot_value;
            }
            rhs[row] -= factor * rhs[column];
        }
    }
    rhs.iter().all(|value| value.is_finite()).then_some(rhs)
}

fn solve_homography(src: &[(f64, f64)], dst: &[(f64, f64)]) -> Option<[f64; 9]> {
    if src.len() != dst.len() || src.len() < 4 {
        return None;
    }

    let mut normal = [[0.0_f64; 8]; 8];
    let mut rhs = [0.0_f64; 8];
    let mut accumulate = |row: [f64; 8], target: f64| {
        for i in 0..8 {
            rhs[i] += row[i] * target;
            for j in 0..8 {
                normal[i][j] += row[i] * row[j];
            }
        }
    };

    for ((x, y), (u, v)) in src.iter().zip(dst.iter()) {
        accumulate([*x, *y, 1.0, 0.0, 0.0, 0.0, -*u * *x, -*u * *y], *u);
        accumulate([0.0, 0.0, 0.0, *x, *y, 1.0, -*v * *x, -*v * *y], *v);
    }

    let solved = solve_linear_8(normal, rhs)?;
    Some([
        solved[0], solved[1], solved[2], solved[3], solved[4], solved[5], solved[6], solved[7], 1.0,
    ])
}

fn apply_homography(point: (f64, f64), homography: &[f64; 9]) -> Option<(f64, f64)> {
    let denominator = homography[6] * point.0 + homography[7] * point.1 + homography[8];
    if !denominator.is_finite() || denominator.abs() < 1e-8 {
        return None;
    }
    let x = (homography[0] * point.0 + homography[1] * point.1 + homography[2]) / denominator;
    let y = (homography[3] * point.0 + homography[4] * point.1 + homography[5]) / denominator;
    (x.is_finite() && y.is_finite()).then_some((x, y))
}

fn normalize_image_point(x: f64, y: f64, width_px: u32, height_px: u32) -> (f64, f64) {
    (
        x * 2.0 / f64::from(width_px) - 1.0,
        y * 2.0 / f64::from(height_px) - 1.0,
    )
}

fn correct_radial(point: (f64, f64), coefficient: f64) -> Option<(f64, f64)> {
    let radius_squared = point.0 * point.0 + point.1 * point.1;
    let denominator = 1.0 + coefficient * radius_squared;
    if !denominator.is_finite() || denominator <= 0.1 {
        return None;
    }
    Some((point.0 / denominator, point.1 / denominator))
}

fn inverse_similarity_point(point: (f64, f64), transform: &SimilarityTransform) -> (f64, f64) {
    let translated_x = point.0 - transform.translation_x;
    let translated_y = point.1 - transform.translation_y;
    let rotation = -transform.rotation_deg.to_radians();
    let cos = rotation.cos();
    let sin = rotation.sin();
    (
        (cos * translated_x - sin * translated_y) / transform.scale,
        (sin * translated_x + cos * translated_y) / transform.scale,
    )
}

fn normalized_output_point(
    workspace_point: (f64, f64),
    output_width_px: u32,
    output_height_px: u32,
    transform: &SimilarityTransform,
) -> (f64, f64) {
    let output = inverse_similarity_point(workspace_point, transform);
    (
        output.0 * 2.0 / f64::from(output_width_px) - 1.0,
        output.1 * 2.0 / f64::from(output_height_px) - 1.0,
    )
}

fn warp_rmse_mm(
    src: &[(f64, f64)],
    workspace_dst: &[(f64, f64)],
    homography: &[f64; 9],
    radial_coefficient: f64,
    output_width_px: u32,
    output_height_px: u32,
    transform: &SimilarityTransform,
) -> Option<f64> {
    let mut squared_error = 0.0;
    for (source, expected) in src.iter().zip(workspace_dst.iter()) {
        let corrected = correct_radial(*source, radial_coefficient)?;
        let normalized_output = apply_homography(corrected, homography)?;
        let output_x = (normalized_output.0 + 1.0) * f64::from(output_width_px) / 2.0;
        let output_y = (normalized_output.1 + 1.0) * f64::from(output_height_px) / 2.0;
        let actual = {
            let rotation = transform.rotation_deg.to_radians();
            let cos = rotation.cos();
            let sin = rotation.sin();
            (
                transform.scale * (cos * output_x - sin * output_y) + transform.translation_x,
                transform.scale * (sin * output_x + cos * output_y) + transform.translation_y,
            )
        };
        squared_error += (actual.0 - expected.0).powi(2) + (actual.1 - expected.1).powi(2);
    }
    Some((squared_error / src.len() as f64).sqrt())
}

fn fit_warp(
    src: &[(f64, f64)],
    normalized_dst: &[(f64, f64)],
    workspace_dst: &[(f64, f64)],
    radial_coefficient: f64,
    output_width_px: u32,
    output_height_px: u32,
    transform: &SimilarityTransform,
) -> Option<([f64; 9], f64)> {
    let corrected = src
        .iter()
        .map(|point| correct_radial(*point, radial_coefficient))
        .collect::<Option<Vec<_>>>()?;
    let homography = solve_homography(&corrected, normalized_dst)?;
    let rmse = warp_rmse_mm(
        src,
        workspace_dst,
        &homography,
        radial_coefficient,
        output_width_px,
        output_height_px,
        transform,
    )?;
    Some((homography, rmse))
}

/// Solve an installed-camera alignment that can correct perspective and, with
/// six or more well-spread points, radial wide-angle distortion.
pub fn solve_warped_alignment(
    points: &AlignmentPointSet,
    image_width_px: u32,
    image_height_px: u32,
    workspace_width_mm: f64,
    workspace_height_mm: f64,
) -> Result<CameraAlignment, String> {
    if image_width_px == 0 || image_height_px == 0 {
        return Err("Camera frame dimensions must be greater than zero".to_string());
    }
    if !workspace_width_mm.is_finite()
        || !workspace_height_mm.is_finite()
        || workspace_width_mm <= 0.0
        || workspace_height_mm <= 0.0
    {
        return Err("Workspace dimensions must be greater than zero".to_string());
    }
    if points.points.len() < 4 {
        return Err("At least four point pairs are required for corrected alignment".to_string());
    }
    if points.points.iter().any(|point| {
        !point.camera_x.is_finite()
            || !point.camera_y.is_finite()
            || !point.workspace_x_mm.is_finite()
            || !point.workspace_y_mm.is_finite()
            || point.camera_x < 0.0
            || point.camera_y < 0.0
            || point.camera_x > f64::from(image_width_px)
            || point.camera_y > f64::from(image_height_px)
            || point.workspace_x_mm < 0.0
            || point.workspace_y_mm < 0.0
            || point.workspace_x_mm > workspace_width_mm
            || point.workspace_y_mm > workspace_height_mm
    }) {
        return Err(
            "Alignment points must fall within the captured frame and workspace".to_string(),
        );
    }

    let source_pixels = points
        .points
        .iter()
        .map(|point| (point.camera_x, point.camera_y))
        .collect::<Vec<_>>();
    let workspace_dst = points
        .points
        .iter()
        .map(|point| (point.workspace_x_mm, point.workspace_y_mm))
        .collect::<Vec<_>>();
    if !points_span_area(&source_pixels) {
        return Err("Camera points must span a non-zero area".to_string());
    }
    if !points_span_area(&workspace_dst) {
        return Err("Workspace points must span a non-zero area".to_string());
    }

    let pixels_per_mm = (f64::from(image_width_px) / workspace_width_mm)
        .min(f64::from(image_height_px) / workspace_height_mm)
        .max(1.0);
    let output_width_px = (workspace_width_mm * pixels_per_mm)
        .round()
        .clamp(1.0, f64::from(image_width_px)) as u32;
    let output_height_px = (workspace_height_mm * pixels_per_mm)
        .round()
        .clamp(1.0, f64::from(image_height_px)) as u32;
    let transform = {
        let scale = (workspace_width_mm / f64::from(output_width_px))
            .min(workspace_height_mm / f64::from(output_height_px));
        SimilarityTransform {
            scale,
            rotation_deg: 0.0,
            translation_x: (workspace_width_mm - f64::from(output_width_px) * scale) / 2.0,
            translation_y: (workspace_height_mm - f64::from(output_height_px) * scale) / 2.0,
        }
    };
    let src = source_pixels
        .iter()
        .map(|(x, y)| normalize_image_point(*x, *y, image_width_px, image_height_px))
        .collect::<Vec<_>>();
    let normalized_dst = workspace_dst
        .iter()
        .map(|point| normalized_output_point(*point, output_width_px, output_height_px, &transform))
        .collect::<Vec<_>>();

    let (zero_homography, zero_rmse) = fit_warp(
        &src,
        &normalized_dst,
        &workspace_dst,
        0.0,
        output_width_px,
        output_height_px,
        &transform,
    )
    .ok_or_else(|| {
        "Camera alignment points could not produce a stable perspective correction".to_string()
    })?;

    let mut best = (0.0, zero_homography, zero_rmse);
    if points.points.len() >= 6 {
        let mut coefficient = -0.38;
        while coefficient <= 0.800_001 {
            if let Some((homography, rmse)) = fit_warp(
                &src,
                &normalized_dst,
                &workspace_dst,
                coefficient,
                output_width_px,
                output_height_px,
                &transform,
            ) && rmse < best.2
            {
                best = (coefficient, homography, rmse);
            }
            coefficient += 0.02;
        }

        let mut low = (best.0 - 0.03).max(-0.38);
        let mut high = (best.0 + 0.03).min(0.8);
        for _ in 0..24 {
            let left = low + (high - low) / 3.0;
            let right = high - (high - low) / 3.0;
            let left_fit = fit_warp(
                &src,
                &normalized_dst,
                &workspace_dst,
                left,
                output_width_px,
                output_height_px,
                &transform,
            );
            let right_fit = fit_warp(
                &src,
                &normalized_dst,
                &workspace_dst,
                right,
                output_width_px,
                output_height_px,
                &transform,
            );
            match (left_fit, right_fit) {
                (Some((left_h, left_rmse)), Some((right_h, right_rmse))) => {
                    if left_rmse <= right_rmse {
                        high = right;
                        if left_rmse < best.2 {
                            best = (left, left_h, left_rmse);
                        }
                    } else {
                        low = left;
                        if right_rmse < best.2 {
                            best = (right, right_h, right_rmse);
                        }
                    }
                }
                (Some((left_h, left_rmse)), None) => {
                    high = right;
                    if left_rmse < best.2 {
                        best = (left, left_h, left_rmse);
                    }
                }
                (None, Some((right_h, right_rmse))) => {
                    low = left;
                    if right_rmse < best.2 {
                        best = (right, right_h, right_rmse);
                    }
                }
                (None, None) => break,
            }
        }

        let meaningful_improvement = zero_rmse - best.2 > zero_rmse.mul_add(0.05, 0.05);
        if !meaningful_improvement {
            best = (0.0, zero_homography, zero_rmse);
        }
    }

    Ok(CameraAlignment {
        transform,
        image_warp: Some(CameraImageWarp {
            output_width_px,
            output_height_px,
            homography: best.1,
            radial_coefficient: best.0,
        }),
        image_width_px: Some(image_width_px),
        image_height_px: Some(image_height_px),
        rmse_mm: best.2,
        quality_score: quality_score(best.2),
        solved_at: Utc::now().to_rfc3339(),
        source: CameraAlignmentSource::SolvedPoints,
    })
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
        image_warp: None,
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
        assert!(alignment.image_warp.is_none());
    }

    #[test]
    fn warped_alignment_corrects_perspective_from_four_points() {
        let alignment = solve_warped_alignment(
            &AlignmentPointSet {
                points: vec![
                    AlignmentPoint {
                        camera_x: 140.0,
                        camera_y: 90.0,
                        workspace_x_mm: 0.0,
                        workspace_y_mm: 0.0,
                    },
                    AlignmentPoint {
                        camera_x: 900.0,
                        camera_y: 130.0,
                        workspace_x_mm: 200.0,
                        workspace_y_mm: 0.0,
                    },
                    AlignmentPoint {
                        camera_x: 820.0,
                        camera_y: 700.0,
                        workspace_x_mm: 200.0,
                        workspace_y_mm: 100.0,
                    },
                    AlignmentPoint {
                        camera_x: 210.0,
                        camera_y: 660.0,
                        workspace_x_mm: 0.0,
                        workspace_y_mm: 100.0,
                    },
                ],
            },
            1000,
            800,
            200.0,
            100.0,
        )
        .unwrap();

        let warp = alignment.image_warp.as_ref().unwrap();
        assert_eq!(warp.radial_coefficient, 0.0);
        assert!(alignment.rmse_mm < 1e-6);
        assert_eq!(warp.output_width_px, 1000);
        assert_eq!(warp.output_height_px, 500);
    }

    #[test]
    fn warped_alignment_estimates_wide_angle_radial_correction() {
        let expected_coefficient = 0.28;
        let mut points = Vec::new();
        for y in [-0.82, 0.0, 0.82] {
            for x in [-0.82, 0.0, 0.82] {
                let corrected = correct_radial((x, y), expected_coefficient).unwrap();
                points.push(AlignmentPoint {
                    camera_x: (x + 1.0) * 500.0,
                    camera_y: (y + 1.0) * 500.0,
                    workspace_x_mm: (corrected.0 + 1.0) * 100.0,
                    workspace_y_mm: (corrected.1 + 1.0) * 100.0,
                });
            }
        }

        let alignment =
            solve_warped_alignment(&AlignmentPointSet { points }, 1000, 1000, 200.0, 200.0)
                .unwrap();
        let warp = alignment.image_warp.as_ref().unwrap();

        assert!((warp.radial_coefficient - expected_coefficient).abs() < 0.01);
        assert!(alignment.rmse_mm < 0.01);
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
