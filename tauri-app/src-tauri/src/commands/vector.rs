use std::sync::Arc;
use beambench_common::{BarcodeOptions, BarcodeType, Bounds, Point2D};
use beambench_core::vector::node_edit::EditablePath;
use beambench_core::vector::normalize::NormalizedVector;
use beambench_core::vector::path_ops::PathVertex;
use beambench_core::{ImageMaskPolarity, ProjectObject};
use beambench_service::ServiceContext;
use beambench_service::ops::vector;
use tauri::State;
pub use beambench_service::ops::workflows::vector::*;
#[tauri::command]
pub fn convert_to_path(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::convert_to_path(&svc, object_id)
}
#[tauri::command]
pub fn boolean_union(
    svc: State<'_, Arc<ServiceContext>>,
    object_id_a: String,
    object_id_b: String,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::boolean_union(
        &svc,
        object_id_a,
        object_id_b,
    )
}
#[tauri::command]
pub fn boolean_subtract(
    svc: State<'_, Arc<ServiceContext>>,
    object_id_a: String,
    object_id_b: String,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::boolean_subtract(
        &svc,
        object_id_a,
        object_id_b,
    )
}
#[tauri::command]
pub fn boolean_exclude(
    svc: State<'_, Arc<ServiceContext>>,
    object_id_a: String,
    object_id_b: String,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::boolean_exclude(
        &svc,
        object_id_a,
        object_id_b,
    )
}
#[tauri::command]
pub fn group_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::group_objects(&svc, object_ids)
}
#[tauri::command]
pub fn auto_group_objects(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::vector::auto_group_objects(&svc, object_ids)
}
#[tauri::command]
pub fn ungroup_objects(
    svc: State<'_, Arc<ServiceContext>>,
    group_id: String,
) -> Result<Vec<String>, String> {
    beambench_service::ops::workflows::vector::ungroup_objects(&svc, group_id)
}
#[tauri::command]
pub fn get_editable_path(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<Vec<EditablePath>, String> {
    beambench_service::ops::workflows::vector::get_editable_path(&svc, object_id)
}
#[tauri::command]
pub fn copy_nodes(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    node_ids: Vec<serde_json::Value>,
) -> Result<vector::NodeClipboardCopy, String> {
    beambench_service::ops::workflows::vector::copy_nodes(&svc, object_id, node_ids)
}
#[tauri::command]
pub fn paste_nodes(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    copied_path_json: String,
    offset_mm: Option<f64>,
) -> Result<vector::NodePasteResult, String> {
    beambench_service::ops::workflows::vector::paste_nodes(
        &svc,
        object_id,
        copied_path_json,
        offset_mm,
    )
}
#[tauri::command]
pub fn extract_nodes_to_path(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    node_ids: Vec<serde_json::Value>,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::extract_nodes_to_path(
        &svc,
        object_id,
        node_ids,
    )
}
#[tauri::command]
pub fn update_node(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
    x: f64,
    y: f64,
    handle_type: Option<String>,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::update_node(
        &svc,
        object_id,
        subpath_idx,
        command_idx,
        x,
        y,
        handle_type,
    )
}
#[tauri::command]
pub fn update_nodes_batch(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    updates: Vec<serde_json::Value>,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::update_nodes_batch(
        &svc,
        object_id,
        updates,
    )
}
#[tauri::command]
pub fn set_node_type(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
    node_type: String,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::set_node_type(
        &svc,
        object_id,
        subpath_idx,
        command_idx,
        node_type,
    )
}
#[tauri::command]
pub fn delete_node(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::delete_node(
        &svc,
        object_id,
        subpath_idx,
        command_idx,
    )
}
#[tauri::command]
pub fn delete_nodes(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    node_ids: Vec<serde_json::Value>,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::delete_nodes(&svc, object_id, node_ids)
}
#[tauri::command]
pub fn insert_node(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
    t: f64,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::insert_node(
        &svc,
        object_id,
        subpath_idx,
        command_idx,
        t,
    )
}
#[tauri::command]
pub fn convert_segment_to_line(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::convert_segment_to_line(
        &svc,
        object_id,
        subpath_idx,
        command_idx,
    )
}
#[tauri::command]
pub fn convert_segment_to_curve(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::convert_segment_to_curve(
        &svc,
        object_id,
        subpath_idx,
        command_idx,
    )
}
#[tauri::command]
pub fn align_segment_to_angle(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::align_segment_to_angle(
        &svc,
        object_id,
        subpath_idx,
        command_idx,
    )
}
#[tauri::command]
pub fn trim_segment_to_intersection(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
    click_x: f64,
    click_y: f64,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::trim_segment_to_intersection(
        &svc,
        object_id,
        subpath_idx,
        command_idx,
        click_x,
        click_y,
    )
}
#[tauri::command]
pub fn extend_endpoint_to_intersection(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::extend_endpoint_to_intersection(
        &svc,
        object_id,
        subpath_idx,
        command_idx,
    )
}
#[tauri::command]
pub fn join_subpaths(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    src_node_id: serde_json::Value,
    dst_node_id: serde_json::Value,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::join_subpaths(
        &svc,
        object_id,
        src_node_id,
        dst_node_id,
    )
}
#[tauri::command]
pub fn delete_segment_cmd(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::delete_segment_cmd(
        &svc,
        object_id,
        subpath_idx,
        command_idx,
    )
}
#[tauri::command]
pub fn break_path_at_node(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
    command_idx: usize,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::break_path_at_node(
        &svc,
        object_id,
        subpath_idx,
        command_idx,
    )
}
#[tauri::command]
pub fn toggle_path_closed(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_idx: usize,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::toggle_path_closed(
        &svc,
        object_id,
        subpath_idx,
    )
}
#[tauri::command]
pub fn scale_path_to_bounds(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    new_min_x: f64,
    new_min_y: f64,
    new_max_x: f64,
    new_max_y: f64,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::scale_path_to_bounds(
        &svc,
        object_id,
        new_min_x,
        new_min_y,
        new_max_x,
        new_max_y,
    )
}
#[tauri::command]
pub async fn mesh_deform_selection(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    source_bounds: Bounds,
    handles: Vec<Point2D>,
    grid_size: usize,
    perspective: bool,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::vector::mesh_deform_selection(
            &svc,
            object_ids,
            source_bounds,
            handles,
            grid_size,
            perspective,
        )
        .await
}
#[tauri::command]
pub fn normalize_for_planner(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<Vec<NormalizedVector>, String> {
    beambench_service::ops::workflows::vector::normalize_for_planner(&svc, object_ids)
}
#[tauri::command]
pub fn boolean_assistant_preview(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    operation: String,
) -> Result<BooleanAssistantPreview, String> {
    beambench_service::ops::workflows::vector::boolean_assistant_preview(
        &svc,
        object_ids,
        operation,
    )
}
#[tauri::command]
pub fn boolean_intersection(
    svc: State<'_, Arc<ServiceContext>>,
    object_id_a: String,
    object_id_b: String,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::boolean_intersection(
        &svc,
        object_id_a,
        object_id_b,
    )
}
#[tauri::command]
pub fn boolean_weld(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::boolean_weld(&svc, object_ids)
}
#[tauri::command]
pub fn boolean_intersection_many(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::boolean_intersection_many(
        &svc,
        object_ids,
    )
}
#[tauri::command]
pub fn boolean_union_many(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::boolean_union_many(&svc, object_ids)
}
#[tauri::command]
pub fn boolean_exclude_many(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::boolean_exclude_many(&svc, object_ids)
}
#[tauri::command]
pub fn boolean_subtract_many(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::boolean_subtract_many(&svc, object_ids)
}
#[tauri::command]
pub fn offset_shapes(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    distance: f64,
    direction: String,
    corner_style: Option<String>,
    delete_original: Option<bool>,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::vector::offset_shapes(
        &svc,
        object_ids,
        distance,
        direction,
        corner_style,
        delete_original,
    )
}
#[tauri::command]
pub async fn preview_offset_shapes(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    distance: f64,
    direction: String,
    corner_style: Option<String>,
) -> Result<OffsetPreview, String> {
    beambench_service::ops::workflows::vector::preview_offset_shapes(
            &svc,
            object_ids,
            distance,
            direction,
            corner_style,
        )
        .await
}
#[tauri::command]
pub fn close_path(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::close_path(&svc, object_id)
}
#[tauri::command]
pub fn close_paths_with_tolerance(
    _svc: State<'_, Arc<ServiceContext>>,
    paths: Vec<String>,
    tolerance: f64,
) -> Result<Vec<String>, String> {
    beambench_service::ops::workflows::vector::close_paths_with_tolerance(
        &_svc,
        paths,
        tolerance,
    )
}
#[tauri::command]
pub fn close_selected_paths_with_tolerance(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    tolerance: f64,
    mode: String,
) -> Result<CloseSelectedPathsWithToleranceResult, String> {
    beambench_service::ops::workflows::vector::close_selected_paths_with_tolerance(
        &svc,
        object_ids,
        tolerance,
        mode,
    )
}
#[tauri::command]
pub fn count_open_paths_with_tolerance(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    tolerance: f64,
    mode: String,
) -> Result<CloseSelectedPathsWithToleranceResult, String> {
    beambench_service::ops::workflows::vector::count_open_paths_with_tolerance(
        &svc,
        object_ids,
        tolerance,
        mode,
    )
}
#[tauri::command]
pub fn break_apart(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::vector::break_apart(&svc, object_id)
}
#[tauri::command]
pub fn set_start_point(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    x: f64,
    y: f64,
    mode: Option<String>,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::set_start_point(
        &svc,
        object_id,
        x,
        y,
        mode,
    )
}
#[tauri::command]
pub fn get_path_vertices(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<Vec<PathVertex>, String> {
    beambench_service::ops::workflows::vector::get_path_vertices(&svc, object_id)
}
#[tauri::command]
pub fn apply_radius(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    radius_mm: f64,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::apply_radius(&svc, object_id, radius_mm)
}
#[tauri::command]
pub fn get_fillet_candidates(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<Vec<beambench_core::vector::path_ops::FilletCandidate>, String> {
    beambench_service::ops::workflows::vector::get_fillet_candidates(&svc, object_id)
}
#[tauri::command]
pub fn apply_corner_radius(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    subpath_index: usize,
    vertex_index: usize,
    radius_mm: f64,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::apply_corner_radius(
        &svc,
        object_id,
        subpath_index,
        vertex_index,
        radius_mm,
    )
}
#[tauri::command]
pub fn grid_array(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    rows: u32,
    cols: u32,
    sizing_mode_x: Option<String>,
    sizing_mode_y: Option<String>,
    total_width_mm: Option<f64>,
    total_height_mm: Option<f64>,
    h_spacing_mm: f64,
    v_spacing_mm: f64,
    #[allow(unused_variables)]
    spacing_mode: Option<String>,
    mirror_alternate_cols: Option<bool>,
    mirror_alternate_rows: Option<bool>,
    x_col_shift_mm: Option<f64>,
    y_row_shift_mm: Option<f64>,
    half_shift: Option<bool>,
    reverse_h: Option<bool>,
    reverse_v: Option<bool>,
    random_orientation: Option<bool>,
    random_seed: Option<u64>,
    group_results: Option<bool>,
    create_virtual: Option<bool>,
    auto_increment_text: Option<bool>,
    text_increment: Option<i64>,
) -> Result<ArrayResult, String> {
    beambench_service::ops::workflows::vector::grid_array(
        &svc,
        object_ids,
        rows,
        cols,
        sizing_mode_x,
        sizing_mode_y,
        total_width_mm,
        total_height_mm,
        h_spacing_mm,
        v_spacing_mm,
        spacing_mode,
        mirror_alternate_cols,
        mirror_alternate_rows,
        x_col_shift_mm,
        y_row_shift_mm,
        half_shift,
        reverse_h,
        reverse_v,
        random_orientation,
        random_seed,
        group_results,
        create_virtual,
        auto_increment_text,
        text_increment,
    )
}
#[tauri::command]
pub fn circular_array(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    count: u32,
    radius_mm: f64,
    rotate_copies: Option<bool>,
    center_x: Option<f64>,
    center_y: Option<f64>,
    center_object_id: Option<String>,
    start_angle_deg: Option<f64>,
    end_angle_deg: Option<f64>,
    group_results: Option<bool>,
    create_virtual: Option<bool>,
    auto_increment_text: Option<bool>,
    text_increment: Option<i64>,
) -> Result<ArrayResult, String> {
    beambench_service::ops::workflows::vector::circular_array(
        &svc,
        object_ids,
        count,
        radius_mm,
        rotate_copies,
        center_x,
        center_y,
        center_object_id,
        start_angle_deg,
        end_angle_deg,
        group_results,
        create_virtual,
        auto_increment_text,
        text_increment,
    )
}
#[tauri::command]
pub fn copy_along_path(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    path_object_id: String,
    count: u32,
    rotate: bool,
    scale_copies: Option<bool>,
    final_scale_percent: Option<f64>,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::vector::copy_along_path(
        &svc,
        object_id,
        path_object_id,
        count,
        rotate,
        scale_copies,
        final_scale_percent,
    )
}
#[tauri::command]
pub fn copy_along_path_batch(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    path_object_id: String,
    count: u32,
    rotate: bool,
    scale_copies: Option<bool>,
    final_scale_percent: Option<f64>,
) -> Result<Vec<ProjectObject>, String> {
    beambench_service::ops::workflows::vector::copy_along_path_batch(
        &svc,
        object_ids,
        path_object_id,
        count,
        rotate,
        scale_copies,
        final_scale_percent,
    )
}
#[tauri::command]
pub fn rubber_band_outline(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::rubber_band_outline(&svc, object_ids)
}
#[tauri::command]
pub fn apply_path_to_text(
    svc: State<'_, Arc<ServiceContext>>,
    text_object_id: String,
    path_object_id: String,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::apply_path_to_text(
        &svc,
        text_object_id,
        path_object_id,
    )
}
#[tauri::command]
pub fn crop_image(
    svc: State<'_, Arc<ServiceContext>>,
    image_object_id: String,
    mask_object_id: String,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::crop_image(
        &svc,
        image_object_id,
        mask_object_id,
    )
}
#[tauri::command]
pub fn apply_mask_to_image(
    svc: State<'_, Arc<ServiceContext>>,
    image_object_id: String,
    mask_object_id: String,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::apply_mask_to_image(
        &svc,
        image_object_id,
        mask_object_id,
    )
}
#[tauri::command]
pub fn assign_image_mask(
    svc: State<'_, Arc<ServiceContext>>,
    image_object_id: String,
    mask_object_ids: Vec<String>,
    polarity: ImageMaskPolarity,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::assign_image_mask(
        &svc,
        image_object_id,
        mask_object_ids,
        polarity,
    )
}
#[tauri::command]
pub fn set_image_mask_polarity(
    svc: State<'_, Arc<ServiceContext>>,
    image_object_id: String,
    mask_object_id: String,
    polarity: ImageMaskPolarity,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::set_image_mask_polarity(
        &svc,
        image_object_id,
        mask_object_id,
        polarity,
    )
}
#[tauri::command]
pub fn remove_image_mask(
    svc: State<'_, Arc<ServiceContext>>,
    image_object_id: String,
    mask_object_id: Option<String>,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::remove_image_mask(
        &svc,
        image_object_id,
        mask_object_id,
    )
}
#[tauri::command]
pub fn convert_to_bitmap(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    dpi: f64,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::convert_to_bitmap(&svc, object_id, dpi)
}
#[tauri::command]
pub fn add_tabs(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    count: u32,
    width_mm: f64,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::add_tabs(&svc, object_id, count, width_mm)
}
#[tauri::command]
pub fn trim_shape(
    svc: State<'_, Arc<ServiceContext>>,
    click_x: f64,
    click_y: f64,
    edge_threshold_mm: f64,
    heal: Option<bool>,
) -> Result<vector::TrimShapeResult, String> {
    beambench_service::ops::workflows::vector::trim_shape(
        &svc,
        click_x,
        click_y,
        edge_threshold_mm,
        heal,
    )
}
#[tauri::command]
pub fn preview_trim_segment(
    svc: State<'_, Arc<ServiceContext>>,
    click_x: f64,
    click_y: f64,
    edge_threshold_mm: f64,
) -> Result<Option<vector::TrimPreview>, String> {
    beambench_service::ops::workflows::vector::preview_trim_segment(
        &svc,
        click_x,
        click_y,
        edge_threshold_mm,
    )
}
#[tauri::command]
pub fn close_and_join(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
    tolerance: Option<f64>,
) -> Result<CloseAndJoinResult, String> {
    beambench_service::ops::workflows::vector::close_and_join(
        &svc,
        object_ids,
        tolerance,
    )
}
#[tauri::command]
pub fn cut_shapes_apply(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<CutShapesApplyResult, String> {
    beambench_service::ops::workflows::vector::cut_shapes_apply(&svc, object_ids)
}
#[tauri::command]
pub fn cut_shapes(
    svc: State<'_, Arc<ServiceContext>>,
    object_ids: Vec<String>,
) -> Result<Vec<String>, String> {
    beambench_service::ops::workflows::vector::cut_shapes(&svc, object_ids)
}
#[tauri::command]
pub fn generate_barcode(
    _svc: State<'_, Arc<ServiceContext>>,
    barcode_type: BarcodeType,
    data: String,
    width: f64,
    height: f64,
    options: Option<BarcodeOptions>,
) -> Result<String, String> {
    beambench_service::ops::workflows::vector::generate_barcode(
        &_svc,
        barcode_type,
        data,
        width,
        height,
        options,
    )
}
#[tauri::command]
pub fn place_tab(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    world_x: f64,
    world_y: f64,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::place_tab(
        &svc,
        object_id,
        world_x,
        world_y,
    )
}
#[tauri::command]
pub fn remove_tab(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
    world_x: f64,
    world_y: f64,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::remove_tab(
        &svc,
        object_id,
        world_x,
        world_y,
    )
}
#[tauri::command]
pub fn clear_tabs(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::clear_tabs(&svc, object_id)
}
#[tauri::command]
pub fn resolve_tab_markers(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<Vec<TabMarkerDto>, String> {
    beambench_service::ops::workflows::vector::resolve_tab_markers(&svc, object_id)
}
#[tauri::command]
pub fn unlink_virtual_clone(
    svc: State<'_, Arc<ServiceContext>>,
    object_id: String,
) -> Result<ProjectObject, String> {
    beambench_service::ops::workflows::vector::unlink_virtual_clone(&svc, object_id)
}
