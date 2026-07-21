use std::collections::{HashMap, HashSet};

use beambench_common::path::{PathCommand, VecPath};
use beambench_common::{BarcodeOptions, BarcodeType, Bounds, Id, Point2D, Transform2D};
use beambench_core::array_ops::{
    CircularArrayConfig, GridArrayConfig, circular_array, copy_along_path, grid_array_in_project,
    rubber_band_outline as core_rubber_band_outline,
};
use beambench_core::import::{import_image as core_import_image, import_svg as core_import_svg};
use beambench_core::layer::{CutEntryPatch, LayerPatch};
use beambench_core::object::{
    TextAlignment, TextAlignmentV, TextCirclePlacement, TextLayoutMode, TextTransformStyle,
};
use beambench_core::vector::boolean::{path_exclude, path_intersection, path_subtract, path_union};
use beambench_core::vector::convert::object_to_world_vecpath_resolved;
use beambench_core::vector::offset::{CornerStyle, OffsetDirection, offset_path};
use beambench_core::vector::path_ops;
use beambench_core::vector::tabs as tab_ops;
use beambench_core::{
    CutEntry, CutEntryId, Layer, LayerId, ObjectData, ObjectId, OperationType, Project,
    ProjectObject, ShapeKind, apply_path_to_text_with_options,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::context::ServiceContext;
use crate::error::{ServiceError, ServiceErrorCode, ServiceResult};
use crate::ops::planning;
use crate::ops::project::{self as project_ops, UpdateObjectInput};
use crate::ops::vector::{
    BooleanOpInput, BooleanWeldInput, ConvertToPathInput, GroupObjectsInput,
    boolean_binary_op_in_project, boolean_weld_in_project, convert_to_path_in_project,
    group_objects_in_project, ungroup_objects_in_project,
};
use crate::validation::RoutingTarget;

pub const DESIGN_SCHEMA_VERSION: u32 = 1;
const DEFAULT_DESIGN_LAYER_COLOR: &str = beambench_common::PALETTE_COLORS[0].hex;

#[derive(Debug, Clone, Deserialize)]
pub struct DesignPlan {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub operations: Vec<Value>,
    #[serde(default)]
    pub options: DesignOptions,
}

fn default_schema_version() -> u32 {
    DESIGN_SCHEMA_VERSION
}

#[derive(Debug, Clone, Deserialize)]
pub struct DesignOptions {
    #[serde(default = "default_true")]
    pub validate_bounds: bool,
    #[serde(default)]
    pub allow_out_of_bounds: bool,
}

impl Default for DesignOptions {
    fn default() -> Self {
        Self {
            validate_bounds: true,
            allow_out_of_bounds: false,
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionMode {
    Plan,
    Apply,
}

#[derive(Debug, Clone, Serialize)]
pub struct DesignError {
    pub op_index: Option<usize>,
    pub op: Option<String>,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DesignTransactionResult {
    pub schema_version: u32,
    pub transaction_id: Uuid,
    pub applied: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<DesignError>,
    pub warnings: Vec<String>,
    pub summary: DesignTransactionSummary,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct DesignTransactionSummary {
    pub op_count: usize,
    pub created_object_ids: Vec<ObjectId>,
    pub modified_object_ids: Vec<ObjectId>,
    pub deleted_object_ids: Vec<ObjectId>,
    pub touched_layer_ids: Vec<LayerId>,
    pub touched_cut_entry_ids: Vec<CutEntryId>,
    #[serde(default)]
    pub refs: HashMap<String, Value>,
}

#[derive(Debug, Clone)]
enum RefValue {
    Object(ObjectId),
    Objects(Vec<ObjectId>),
    Layer(LayerId),
    Layers(Vec<LayerId>),
    CutEntry {
        layer_id: LayerId,
        entry_id: CutEntryId,
    },
    CutEntries(Vec<(LayerId, CutEntryId)>),
    None,
}

impl RefValue {
    fn to_json(&self) -> Value {
        match self {
            RefValue::Object(id) => json!({ "type": "object", "id": id }),
            RefValue::Objects(ids) => json!({ "type": "objects", "ids": ids }),
            RefValue::Layer(id) => json!({ "type": "layer", "id": id }),
            RefValue::Layers(ids) => json!({ "type": "layers", "ids": ids }),
            RefValue::CutEntry { layer_id, entry_id } => {
                json!({ "type": "cut_entry", "layer_id": layer_id, "entry_id": entry_id })
            }
            RefValue::CutEntries(entries) => {
                let entries: Vec<Value> = entries
                    .iter()
                    .map(|(layer_id, entry_id)| {
                        json!({ "layer_id": layer_id, "entry_id": entry_id })
                    })
                    .collect();
                json!({ "type": "cut_entries", "entries": entries })
            }
            RefValue::None => json!({ "type": "none" }),
        }
    }
}

#[derive(Debug, Default)]
struct RefState {
    refs: HashMap<String, RefValue>,
    last: RefValue,
    created_objects: Vec<ObjectId>,
}

impl Default for RefValue {
    fn default() -> Self {
        RefValue::None
    }
}

#[derive(Debug, Default)]
struct Tracking {
    created: Vec<ObjectId>,
    modified: Vec<ObjectId>,
    deleted: Vec<ObjectId>,
    layers: Vec<LayerId>,
    cut_entries: Vec<CutEntryId>,
}

pub fn describe(ctx: &ServiceContext) -> Value {
    let project = ctx.project.lock().ok().and_then(|guard| guard.clone());
    let settings = ctx.settings.lock().ok().map(|guard| guard.clone());
    let mut warnings = Vec::new();
    if project.is_none() {
        warnings.push("no_active_project".to_string());
    }

    let active_profile = settings.as_ref().and_then(|settings| {
        settings.active_profile_id.and_then(|id| {
            settings
                .machine_profiles
                .iter()
                .find(|p| p.id == id)
                .cloned()
        })
    });
    if active_profile.is_none() {
        warnings.push("missing_machine_profile".to_string());
    }

    if let Some(project) = &project {
        if let Some(snapshot) = &project.machine_profile_snapshot {
            if (snapshot.bed_width_mm - project.workspace.bed_width_mm).abs() > 1e-6
                || (snapshot.bed_height_mm - project.workspace.bed_height_mm).abs() > 1e-6
            {
                warnings.push("profile_project_bed_mismatch".to_string());
            }
        }
        let bed = bed_bounds(project);
        if project
            .objects
            .iter()
            .filter(|object| object.visible)
            .any(|object| !bounds_inside(object.bounds, bed))
        {
            warnings.push("existing_out_of_bed_geometry".to_string());
        }
        let asset_keys: HashSet<String> = project
            .assets
            .iter()
            .map(|asset| asset.id.to_string())
            .collect();
        if project.objects.iter().any(|object| match &object.data {
            ObjectData::RasterImage { asset_key, .. } => !asset_keys.contains(asset_key),
            _ => false,
        }) {
            warnings.push("missing_assets_or_images".to_string());
        }
    }

    json!({
        "schema_version": DESIGN_SCHEMA_VERSION,
        "project": project,
        "active_profile": active_profile,
        "warnings": warnings,
    })
}

pub fn schema() -> Value {
    json!({
        "schema_version": DESIGN_SCHEMA_VERSION,
        "plan_shape": {
            "schema_version": DESIGN_SCHEMA_VERSION,
            "operations": "array",
            "options": { "validate_bounds": true, "allow_out_of_bounds": false }
        },
        "ref_grammar": {
            "existing_id": "plain UUID string",
            "temp_ref": "$name",
            "indexed_ref": "$name[0]",
            "set_ref": "$name[*]",
            "special": ["$last", "$created"],
            "strict_typing": true
        },
        "error_codes": [
            "MISSING_REF", "INVALID_REF_TYPE", "UNKNOWN_OP", "INVALID_FIELD",
            "LAYER_NOT_FOUND", "OBJECT_NOT_FOUND", "OUT_OF_BOUNDS",
            "BOOLEAN_PRECONDITION", "INVALID_BOUNDS", "UNSUPPORTED_SCHEMA_VERSION", "BUSY"
        ],
        "operations": operation_schemas()
    })
}

fn op_schema(
    name: &str,
    output: &str,
    required: &[&str],
    optional: &[&str],
    accepted_refs: &[&str],
    defaults: Value,
    example: Value,
) -> Value {
    json!({
        "name": name,
        "required_fields": required,
        "optional_fields": optional,
        "accepted_ref_types": accepted_refs,
        "output": output,
        "output_type": output,
        "defaults": defaults,
        "example": example,
    })
}

fn operation_schemas() -> Vec<Value> {
    vec![
        op_schema(
            "create_layer",
            "layer",
            &["name"],
            &["operation", "color", "color_tag", "speed", "power", "ref"],
            &[],
            json!({"operation": "line", "color": DEFAULT_DESIGN_LAYER_COLOR}),
            json!({
                "op": "create_layer",
                "name": "Cut",
                "color": DEFAULT_DESIGN_LAYER_COLOR,
                "ref": "cut_layer"
            }),
        ),
        op_schema(
            "update_layer",
            "layer",
            &["layer"],
            &[
                "name",
                "operation",
                "color",
                "color_tag",
                "visible",
                "locked",
                "ref",
            ],
            &["layer"],
            json!({}),
            json!({"op": "update_layer", "layer": "$cut_layer", "name": "Score"}),
        ),
        op_schema(
            "delete_layer",
            "none",
            &["layer"],
            &[],
            &["layer"],
            json!({}),
            json!({"op": "delete_layer", "layer": "$cut_layer"}),
        ),
        op_schema(
            "reorder_layers",
            "layers",
            &["layers"],
            &["ref"],
            &["layer", "layers"],
            json!({}),
            json!({"op": "reorder_layers", "layers": ["$a", "$b"], "ref": "ordered_layers"}),
        ),
        op_schema(
            "add_cut_entry",
            "cut_entry",
            &["layer"],
            &["operation", "settings", "ref"],
            &["layer"],
            json!({"entry": "primary"}),
            json!({"op": "add_cut_entry", "layer": "$cut_layer", "operation": "Line", "ref": "line_entry"}),
        ),
        op_schema(
            "update_cut_entry",
            "cut_entry",
            &["entry"],
            &["operation", "settings", "ref"],
            &["cut_entry", "layer"],
            json!({}),
            json!({"op": "update_cut_entry", "entry": {"layer": "$cut_layer", "entry": "primary"}, "settings": {"speed_mm_min": 3000.0}}),
        ),
        op_schema(
            "delete_cut_entry",
            "none",
            &["entry"],
            &[],
            &["cut_entry", "layer"],
            json!({}),
            json!({"op": "delete_cut_entry", "entry": "$line_entry"}),
        ),
        op_schema(
            "reorder_cut_entries",
            "cut_entries",
            &["layer", "entries"],
            &["ref"],
            &["layer", "cut_entry", "cut_entries"],
            json!({}),
            json!({"op": "reorder_cut_entries", "layer": "$cut_layer", "entries": ["$line_entry"], "ref": "entries"}),
        ),
        op_schema(
            "create_rectangle",
            "object",
            &["x", "y", "width", "height"],
            &["name", "layer", "corner_radius", "ref"],
            &["layer"],
            json!({"name": "Rectangle"}),
            json!({"op": "create_rectangle", "x": 10.0, "y": 10.0, "width": 40.0, "height": 20.0, "ref": "box"}),
        ),
        op_schema(
            "create_ellipse",
            "object",
            &["x", "y", "width", "height"],
            &["name", "layer", "ref"],
            &["layer"],
            json!({"name": "Ellipse"}),
            json!({"op": "create_ellipse", "x": 20.0, "y": 20.0, "width": 25.0, "height": 25.0, "ref": "circle"}),
        ),
        op_schema(
            "create_polygon",
            "object",
            &["x", "y", "radius"],
            &["name", "layer", "sides", "ref"],
            &["layer"],
            json!({"sides": 6}),
            json!({"op": "create_polygon", "x": 50.0, "y": 50.0, "radius": 12.0, "sides": 6, "ref": "hex"}),
        ),
        op_schema(
            "create_star",
            "object",
            &["x", "y", "width", "height"],
            &["name", "layer", "points", "ratio", "bulge", "ref"],
            &["layer"],
            json!({"points": 5, "ratio": 0.5}),
            json!({"op": "create_star", "x": 10.0, "y": 10.0, "width": 30.0, "height": 30.0, "ref": "star"}),
        ),
        op_schema(
            "create_text",
            "object",
            &["text"],
            &[
                "x",
                "y",
                "width",
                "height",
                "font_family",
                "font_size_mm",
                "bold",
                "italic",
                "layer",
                "ref",
            ],
            &["layer"],
            json!({"font_family": "Arial", "font_size_mm": 12.0}),
            json!({"op": "create_text", "text": "Hello", "x": 15.0, "y": 15.0, "ref": "label"}),
        ),
        op_schema(
            "create_barcode",
            "object",
            &["barcode_type", "data"],
            &["x", "y", "width", "height", "layer", "ref"],
            &["layer"],
            json!({"width": 40.0, "height": 20.0}),
            json!({"op": "create_barcode", "barcode_type": "qr_code", "data": "https://example.com", "ref": "qr"}),
        ),
        op_schema(
            "create_vector_path",
            "object",
            &["svg_d or vec_path"],
            &["name", "layer", "ref"],
            &["layer"],
            json!({}),
            json!({"op": "create_vector_path", "svg_d": "M0 0 L10 0", "ref": "line"}),
        ),
        op_schema(
            "create_image",
            "object",
            &["asset_key or path"],
            &[
                "x",
                "y",
                "width",
                "height",
                "original_width_px",
                "original_height_px",
                "layer",
                "ref",
            ],
            &["layer"],
            json!({"original_width_px": 1, "original_height_px": 1}),
            json!({"op": "create_image", "path": "/tmp/photo.png", "x": 0.0, "y": 0.0, "ref": "image"}),
        ),
        op_schema(
            "import_svg",
            "objects",
            &["path"],
            &["layer", "ref"],
            &["layer"],
            json!({}),
            json!({"op": "import_svg", "path": "/tmp/art.svg", "ref": "svg"}),
        ),
        op_schema(
            "update_object",
            "object",
            &["object"],
            &[
                "name",
                "visible",
                "locked",
                "bounds",
                "transform",
                "data",
                "object_data",
                "layer",
                "power_scale",
                "priority",
                "ref",
            ],
            &["object", "layer"],
            json!({}),
            json!({"op": "update_object", "object": "$box", "name": "Panel"}),
        ),
        op_schema(
            "assign_layer",
            "object_or_objects",
            &["object or objects", "layer"],
            &["ref"],
            &["object", "objects", "layer"],
            json!({}),
            json!({"op": "assign_layer", "objects": "$created", "layer": "$cut_layer"}),
        ),
        op_schema(
            "delete_object",
            "none",
            &["object or objects"],
            &[],
            &["object", "objects"],
            json!({}),
            json!({"op": "delete_object", "object": "$box"}),
        ),
        op_schema(
            "duplicate_objects",
            "objects",
            &["object or objects"],
            &["dx", "dy", "ref"],
            &["object", "objects"],
            json!({"dx": 0.0, "dy": 0.0}),
            json!({"op": "duplicate_objects", "objects": "$box", "dx": 5.0, "ref": "copies"}),
        ),
        op_schema(
            "move",
            "object_or_objects",
            &["object or objects"],
            &["dx", "dy", "ref"],
            &["object", "objects"],
            json!({"dx": 0.0, "dy": 0.0}),
            json!({"op": "move", "objects": "$last", "dx": 10.0}),
        ),
        op_schema(
            "resize",
            "object",
            &["object", "width", "height"],
            &["ref"],
            &["object"],
            json!({}),
            json!({"op": "resize", "object": "$box", "width": 50.0, "height": 25.0}),
        ),
        op_schema(
            "rotate",
            "object_or_objects",
            &["object or objects"],
            &["degrees", "ref"],
            &["object", "objects"],
            json!({"degrees": 0.0}),
            json!({"op": "rotate", "objects": "$created", "degrees": 45.0}),
        ),
        op_schema(
            "shear",
            "object_or_objects",
            &["object or objects"],
            &["x", "y", "ref"],
            &["object", "objects"],
            json!({"x": 0.0, "y": 0.0}),
            json!({"op": "shear", "object": "$box", "x": 0.1}),
        ),
        op_schema(
            "flip",
            "object_or_objects",
            &["object or objects"],
            &["axis", "ref"],
            &["object", "objects"],
            json!({"axis": "horizontal"}),
            json!({"op": "flip", "object": "$box", "axis": "vertical"}),
        ),
        op_schema(
            "convert_to_path",
            "object",
            &["object"],
            &["ref"],
            &["object"],
            json!({}),
            json!({"op": "convert_to_path", "object": "$box", "ref": "box_path"}),
        ),
        op_schema(
            "boolean_union",
            "object",
            &["object_a", "object_b"],
            &["ref"],
            &["object"],
            json!({}),
            json!({"op": "boolean_union", "object_a": "$a", "object_b": "$b", "ref": "union"}),
        ),
        op_schema(
            "boolean_subtract",
            "object",
            &["object_a", "object_b"],
            &["ref"],
            &["object"],
            json!({}),
            json!({"op": "boolean_subtract", "object_a": "$a", "object_b": "$b"}),
        ),
        op_schema(
            "boolean_intersection",
            "object",
            &["object_a", "object_b"],
            &["ref"],
            &["object"],
            json!({}),
            json!({"op": "boolean_intersection", "object_a": "$a", "object_b": "$b"}),
        ),
        op_schema(
            "boolean_exclude",
            "object",
            &["object_a", "object_b"],
            &["ref"],
            &["object"],
            json!({}),
            json!({"op": "boolean_exclude", "object_a": "$a", "object_b": "$b"}),
        ),
        op_schema(
            "boolean_weld",
            "object",
            &["objects"],
            &["ref"],
            &["objects"],
            json!({}),
            json!({"op": "boolean_weld", "objects": ["$a", "$b"], "ref": "weld"}),
        ),
        op_schema(
            "group",
            "object",
            &["objects"],
            &["name", "ref"],
            &["objects"],
            json!({"name": "Group"}),
            json!({"op": "group", "objects": "$created", "ref": "group"}),
        ),
        op_schema(
            "ungroup",
            "objects",
            &["object"],
            &["ref"],
            &["object"],
            json!({}),
            json!({"op": "ungroup", "object": "$group", "ref": "members"}),
        ),
        op_schema(
            "break_apart",
            "objects",
            &["object"],
            &["ref"],
            &["object"],
            json!({}),
            json!({"op": "break_apart", "object": "$path", "ref": "parts"}),
        ),
        op_schema(
            "offset",
            "objects",
            &["object or objects"],
            &[
                "distance",
                "direction",
                "corner_style",
                "delete_original",
                "ref",
            ],
            &["object", "objects"],
            json!({"distance": 0.0, "direction": "outward", "corner_style": "miter", "delete_original": false}),
            json!({"op": "offset", "object": "$path", "distance": 2.0, "ref": "offset"}),
        ),
        op_schema(
            "grid_array",
            "objects",
            &[
                "object or objects",
                "rows",
                "cols",
                "h_spacing_mm",
                "v_spacing_mm",
            ],
            &["config", "ref"],
            &["object", "objects"],
            json!({"spacing_mode": "centerToCenter"}),
            json!({"op": "grid_array", "objects": "$box", "rows": 2, "cols": 3, "h_spacing_mm": 30.0, "v_spacing_mm": 20.0, "ref": "grid"}),
        ),
        op_schema(
            "circular_array",
            "objects",
            &["object or objects", "count", "radius_mm", "rotate_copies"],
            &[
                "center_x",
                "center_y",
                "start_angle_deg",
                "end_angle_deg",
                "config",
                "ref",
            ],
            &["object", "objects"],
            json!({"end_angle_deg": 360.0}),
            json!({"op": "circular_array", "objects": "$box", "count": 6, "radius_mm": 40.0, "rotate_copies": true, "ref": "circle_array"}),
        ),
        op_schema(
            "copy_along_path",
            "objects",
            &["object", "guide or path"],
            &[
                "spacing_mm",
                "rotate",
                "scale_copies",
                "final_scale_percent",
                "ref",
            ],
            &["object"],
            json!({"spacing_mm": 10.0, "rotate": false, "scale_copies": false, "final_scale_percent": 100.0}),
            json!({"op": "copy_along_path", "object": "$box", "guide": "$line", "spacing_mm": 5.0, "ref": "path_copies"}),
        ),
        op_schema(
            "mirror_across_line",
            "objects",
            &["object or objects", "axis"],
            &["ref"],
            &["object", "objects"],
            json!({}),
            json!({"op": "mirror_across_line", "objects": "$box", "axis": "$line", "ref": "mirrored"}),
        ),
        op_schema(
            "radius",
            "object",
            &["object"],
            &["radius_mm", "ref"],
            &["object"],
            json!({"radius_mm": 0.0}),
            json!({"op": "radius", "object": "$path", "radius_mm": 2.0}),
        ),
        op_schema(
            "fillet",
            "object",
            &["object"],
            &["radius_mm", "ref"],
            &["object"],
            json!({"radius_mm": 0.0}),
            json!({"op": "fillet", "object": "$path", "radius_mm": 2.0}),
        ),
        op_schema(
            "tabs",
            "object",
            &["object"],
            &["count", "width_mm", "ref"],
            &["object"],
            json!({"count": 0, "width_mm": 0.0}),
            json!({"op": "tabs", "object": "$path", "count": 4, "width_mm": 1.0}),
        ),
        op_schema(
            "start_point",
            "object",
            &["object"],
            &["x", "y", "ref"],
            &["object"],
            json!({"x": 0.0, "y": 0.0}),
            json!({"op": "start_point", "object": "$path", "x": 0.0, "y": 0.0}),
        ),
        op_schema(
            "trim",
            "object",
            &["object"],
            &["t_start", "t_end", "ref"],
            &["object"],
            json!({"t_start": 0.0, "t_end": 1.0}),
            json!({"op": "trim", "object": "$path", "t_start": 0.1, "t_end": 0.9}),
        ),
        op_schema(
            "close_and_join",
            "object",
            &["objects"],
            &["tolerance", "name", "ref"],
            &["objects"],
            json!({"tolerance": 0.1}),
            json!({"op": "close_and_join", "objects": "$parts", "tolerance": 0.2, "ref": "joined"}),
        ),
        op_schema(
            "rubber_band_outline",
            "object",
            &["objects"],
            &["layer", "name", "ref"],
            &["objects", "layer"],
            json!({"name": "Rubber Band Outline"}),
            json!({"op": "rubber_band_outline", "objects": "$created", "ref": "outline"}),
        ),
        op_schema(
            "apply_path_to_text",
            "object",
            &["text", "path"],
            &["ref"],
            &["object"],
            json!({}),
            json!({"op": "apply_path_to_text", "text": "$label", "path": "$line", "ref": "path_text"}),
        ),
        op_schema(
            "align",
            "objects",
            &["objects"],
            &["alignment", "anchor", "ref"],
            &["objects", "object"],
            json!({"alignment": "left"}),
            json!({"op": "align", "objects": "$created", "alignment": "centers_xy"}),
        ),
        op_schema(
            "distribute",
            "objects",
            &["objects"],
            &["direction", "ref"],
            &["objects"],
            json!({"direction": "h_spaced"}),
            json!({"op": "distribute", "objects": "$created", "direction": "h_spaced"}),
        ),
    ]
}

pub fn run_transaction(
    ctx: &ServiceContext,
    plan: DesignPlan,
    mode: TransactionMode,
) -> DesignTransactionResult {
    let transaction_id = Uuid::new_v4();
    let mut result = DesignTransactionResult {
        schema_version: DESIGN_SCHEMA_VERSION,
        transaction_id,
        applied: false,
        error: None,
        warnings: Vec::new(),
        summary: DesignTransactionSummary {
            op_count: plan.operations.len(),
            ..Default::default()
        },
    };

    if plan.schema_version != DESIGN_SCHEMA_VERSION {
        result.error = Some(DesignError {
            op_index: None,
            op: None,
            code: "UNSUPPORTED_SCHEMA_VERSION",
            message: format!("Unsupported design schema version {}", plan.schema_version),
        });
        return result;
    }

    let Ok(_transaction_guard) = ctx.design_transaction_lock.try_lock() else {
        result.error = Some(DesignError {
            op_index: None,
            op: None,
            code: "BUSY",
            message: "A design transaction is already active".to_string(),
        });
        return result;
    };

    let original = match ctx.project.lock() {
        Ok(guard) => match guard.as_ref() {
            Some(project) => project.clone(),
            None => {
                result.error = Some(DesignError {
                    op_index: None,
                    op: None,
                    code: "INVALID_FIELD",
                    message: "No active project".to_string(),
                });
                return result;
            }
        },
        Err(e) => {
            result.error = Some(DesignError {
                op_index: None,
                op: None,
                code: "INVALID_FIELD",
                message: format!("Failed to lock project: {e}"),
            });
            return result;
        }
    };

    let mut working = original.clone();
    let mut refs = RefState::default();
    let mut tracking = Tracking::default();

    for (idx, op) in plan.operations.iter().enumerate() {
        let name = op_name(op);
        match name
            .and_then(|name| execute_op(&mut working, &mut refs, &mut tracking, idx, name, op))
        {
            Ok(()) => {}
            Err(error) => {
                result.error = Some(error);
                result.summary = build_summary(plan.operations.len(), &refs, &tracking);
                return result;
            }
        }
    }

    if plan.options.validate_bounds {
        let bed = bed_bounds(&working);
        let offenders: Vec<ObjectId> = working
            .objects
            .iter()
            .filter(|object| object.visible && !bounds_inside(object.bounds, bed))
            .map(|object| object.id)
            .collect();
        if !offenders.is_empty() {
            let message = format!(
                "{} visible object(s) exceed the active bed bounds",
                offenders.len()
            );
            if plan.options.allow_out_of_bounds {
                result.warnings.push(message);
            } else {
                result.error = Some(DesignError {
                    op_index: None,
                    op: None,
                    code: "OUT_OF_BOUNDS",
                    message,
                });
                result.summary = build_summary(plan.operations.len(), &refs, &tracking);
                return result;
            }
        }
    }

    result.summary = build_summary(plan.operations.len(), &refs, &tracking);
    if mode == TransactionMode::Apply {
        if let Err(err) = commit_design_project(
            ctx,
            original,
            working,
            &result.summary,
            transaction_id,
            &result.warnings,
        ) {
            result.error = Some(DesignError {
                op_index: None,
                op: None,
                code: "INVALID_FIELD",
                message: err.message,
            });
            return result;
        }
        result.applied = true;
    }
    result
}

fn commit_design_project(
    ctx: &ServiceContext,
    original: Project,
    mut project: Project,
    summary: &DesignTransactionSummary,
    transaction_id: Uuid,
    warnings: &[String],
) -> ServiceResult<()> {
    ctx.push_project_undo_snapshot(&original)
        .map_err(ServiceError::internal)?;
    project.dirty = true;
    {
        let mut guard = ctx
            .project
            .lock()
            .map_err(|e| ServiceError::internal(format!("Failed to lock project: {e}")))?;
        *guard = Some(project);
    }
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "project.design.transaction_applied",
        json!({
            "transaction_id": transaction_id,
            "op_count": summary.op_count,
            "created_object_ids": summary.created_object_ids,
            "modified_object_ids": summary.modified_object_ids,
            "deleted_object_ids": summary.deleted_object_ids,
            "touched_layer_ids": summary.touched_layer_ids,
            "touched_cut_entry_ids": summary.touched_cut_entry_ids,
            "warnings": warnings,
        }),
    );
    Ok(())
}

fn build_summary(
    op_count: usize,
    refs: &RefState,
    tracking: &Tracking,
) -> DesignTransactionSummary {
    let refs_json = refs
        .refs
        .iter()
        .map(|(key, value)| (key.clone(), value.to_json()))
        .collect();
    DesignTransactionSummary {
        op_count,
        created_object_ids: unique_ids(&tracking.created),
        modified_object_ids: unique_ids(&tracking.modified),
        deleted_object_ids: unique_ids(&tracking.deleted),
        touched_layer_ids: unique_ids(&tracking.layers),
        touched_cut_entry_ids: unique_ids(&tracking.cut_entries),
        refs: refs_json,
    }
}

fn unique_ids<T: Copy + Eq + std::hash::Hash>(ids: &[T]) -> Vec<T> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for id in ids {
        if seen.insert(*id) {
            out.push(*id);
        }
    }
    out
}

fn op_name(op: &Value) -> Result<&str, DesignError> {
    op.get("op")
        .and_then(Value::as_str)
        .ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: "Operation is missing string field 'op'".to_string(),
        })
}

fn execute_op(
    project: &mut Project,
    refs: &mut RefState,
    tracking: &mut Tracking,
    idx: usize,
    name: &str,
    value: &Value,
) -> Result<(), DesignError> {
    let op_result = match name {
        "create_layer" => create_layer(project, value, tracking),
        "update_layer" => update_layer(project, refs, value, tracking),
        "delete_layer" => delete_layer(project, refs, value, tracking),
        "reorder_layers" => reorder_layers(project, refs, value, tracking),
        "add_cut_entry" => add_cut_entry(project, refs, value, tracking),
        "update_cut_entry" => update_cut_entry(project, refs, value, tracking),
        "delete_cut_entry" => delete_cut_entry(project, refs, value, tracking),
        "reorder_cut_entries" => reorder_cut_entries(project, refs, value, tracking),
        "create_rectangle" => create_shape(project, refs, value, ShapeKind::Rectangle, tracking),
        "create_ellipse" => create_shape(project, refs, value, ShapeKind::Ellipse, tracking),
        "create_polygon" => create_polygon(project, refs, value, tracking),
        "create_star" => create_star(project, refs, value, tracking),
        "create_text" => create_text(project, refs, value, tracking),
        "create_barcode" => create_barcode(project, refs, value, tracking),
        "create_vector_path" => create_vector_path(project, refs, value, tracking),
        "create_image" => create_image(project, refs, value, tracking),
        "import_svg" => import_svg(project, refs, value, tracking),
        "update_object" => update_object(project, refs, value, tracking),
        "assign_layer" => assign_layer(project, refs, value, tracking),
        "delete_object" => delete_objects(project, refs, value, tracking),
        "duplicate_objects" => duplicate_objects(project, refs, value, tracking),
        "move" => move_objects(project, refs, value, tracking),
        "resize" => resize_object(project, refs, value, tracking),
        "rotate" => transform_objects(project, refs, value, tracking, "rotate"),
        "shear" => transform_objects(project, refs, value, tracking, "shear"),
        "flip" => transform_objects(project, refs, value, tracking, "flip"),
        "convert_to_path" => convert_to_path_op(project, refs, value, tracking),
        "boolean_union" => boolean_op(project, refs, value, tracking, "Union", path_union),
        "boolean_subtract" => boolean_op(project, refs, value, tracking, "Subtract", path_subtract),
        "boolean_intersection" => boolean_op(
            project,
            refs,
            value,
            tracking,
            "Intersection",
            path_intersection,
        ),
        "boolean_exclude" => boolean_op(project, refs, value, tracking, "Exclude", path_exclude),
        "boolean_weld" => boolean_weld_op(project, refs, value, tracking),
        "group" => group_op(project, refs, value, tracking),
        "ungroup" => ungroup_op(project, refs, value, tracking),
        "break_apart" => break_apart(project, refs, value, tracking),
        "offset" => offset(project, refs, value, tracking),
        "grid_array" => grid_array_op(project, refs, value, tracking),
        "circular_array" => circular_array_op(project, refs, value, tracking),
        "copy_along_path" => copy_along_path_op(project, refs, value, tracking),
        "mirror_across_line" => mirror_across_line(project, refs, value, tracking),
        "radius" | "fillet" => radius(project, refs, value, tracking),
        "tabs" => tabs(project, refs, value, tracking),
        "start_point" => start_point(project, refs, value, tracking),
        "trim" => trim(project, refs, value, tracking),
        "close_and_join" => close_and_join(project, refs, value, tracking),
        "rubber_band_outline" => rubber_band_outline(project, refs, value, tracking),
        "apply_path_to_text" => apply_path_to_text(project, refs, value, tracking),
        "align" => align(project, refs, value, tracking),
        "distribute" => distribute(project, refs, value, tracking),
        _ => Err(op_error(
            idx,
            name,
            "UNKNOWN_OP",
            format!("Unknown design operation '{name}'"),
        )),
    };

    match op_result {
        Ok(result) => {
            let out = value
                .get("ref")
                .or_else(|| value.get("out"))
                .and_then(Value::as_str);
            if let Some(out) = out {
                refs.refs.insert(out.to_string(), result.clone());
            }
            refs.created_objects = unique_ids(&tracking.created);
            refs.last = result;
            Ok(())
        }
        Err(mut error) => {
            error.op_index = Some(idx);
            error.op = Some(name.to_string());
            Err(error)
        }
    }
}

fn op_error(idx: usize, op: &str, code: &'static str, message: impl Into<String>) -> DesignError {
    DesignError {
        op_index: Some(idx),
        op: Some(op.to_string()),
        code,
        message: message.into(),
    }
}

fn parse<T: DeserializeOwned>(value: &Value) -> Result<T, DesignError> {
    serde_json::from_value(value.clone()).map_err(|e| DesignError {
        op_index: None,
        op: None,
        code: "INVALID_FIELD",
        message: e.to_string(),
    })
}

fn design_error_from_service(error: ServiceError) -> DesignError {
    let message = error.message;
    let message_lc = message.to_ascii_lowercase();
    let code = match error.code {
        ServiceErrorCode::NotFound => {
            if message_lc.contains("layer") {
                "LAYER_NOT_FOUND"
            } else {
                "OBJECT_NOT_FOUND"
            }
        }
        ServiceErrorCode::Busy | ServiceErrorCode::Conflict => "BUSY",
        ServiceErrorCode::InvalidInput | ServiceErrorCode::InvalidState => "INVALID_FIELD",
        _ => "INVALID_FIELD",
    };
    DesignError {
        op_index: None,
        op: None,
        code,
        message,
    }
}

fn get_field<'a>(value: &'a Value, name: &str) -> Result<&'a Value, DesignError> {
    value.get(name).ok_or_else(|| DesignError {
        op_index: None,
        op: None,
        code: "INVALID_FIELD",
        message: format!("Missing required field '{name}'"),
    })
}

fn parse_id<T>(raw: &str, code: &'static str) -> Result<Id<T>, DesignError> {
    let uuid = Uuid::parse_str(raw).map_err(|e| DesignError {
        op_index: None,
        op: None,
        code,
        message: format!("Invalid UUID '{raw}': {e}"),
    })?;
    Ok(Id::from_uuid(uuid))
}

fn resolve_ref_string(raw: &str, refs: &RefState) -> Result<RefValue, DesignError> {
    if raw == "$last" {
        return Ok(refs.last.clone());
    }
    if raw == "$created" {
        return Ok(RefValue::Objects(refs.created_objects.clone()));
    }
    let Some(rest) = raw.strip_prefix('$') else {
        return Err(DesignError {
            op_index: None,
            op: None,
            code: "MISSING_REF",
            message: format!("'{raw}' is not a temporary ref"),
        });
    };
    if let Some(name) = rest.strip_suffix("[*]") {
        return match refs.refs.get(name) {
            Some(value @ RefValue::Objects(_))
            | Some(value @ RefValue::Layers(_))
            | Some(value @ RefValue::CutEntries(_)) => Ok(value.clone()),
            Some(_) => Err(DesignError {
                op_index: None,
                op: None,
                code: "INVALID_REF_TYPE",
                message: format!("Ref '${name}' is not a set"),
            }),
            None => Err(missing_ref(name)),
        };
    }
    if let Some((name, idx_raw)) = rest.strip_suffix(']').and_then(|s| s.split_once('[')) {
        let idx: usize = idx_raw.parse().map_err(|_| DesignError {
            op_index: None,
            op: None,
            code: "INVALID_REF_TYPE",
            message: format!("Invalid ref index '{idx_raw}'"),
        })?;
        return match refs.refs.get(name) {
            Some(RefValue::Objects(ids)) => {
                ids.get(idx)
                    .copied()
                    .map(RefValue::Object)
                    .ok_or_else(|| DesignError {
                        op_index: None,
                        op: None,
                        code: "MISSING_REF",
                        message: format!("Ref '${name}[{idx}]' is out of range"),
                    })
            }
            Some(RefValue::Layers(ids)) => {
                ids.get(idx)
                    .copied()
                    .map(RefValue::Layer)
                    .ok_or_else(|| DesignError {
                        op_index: None,
                        op: None,
                        code: "MISSING_REF",
                        message: format!("Ref '${name}[{idx}]' is out of range"),
                    })
            }
            Some(RefValue::CutEntries(entries)) => entries
                .get(idx)
                .copied()
                .map(|(layer_id, entry_id)| RefValue::CutEntry { layer_id, entry_id })
                .ok_or_else(|| DesignError {
                    op_index: None,
                    op: None,
                    code: "MISSING_REF",
                    message: format!("Ref '${name}[{idx}]' is out of range"),
                }),
            Some(_) => Err(DesignError {
                op_index: None,
                op: None,
                code: "INVALID_REF_TYPE",
                message: format!("Indexed access on single-output ref '${name}' is invalid"),
            }),
            None => Err(missing_ref(name)),
        };
    }
    refs.refs
        .get(rest)
        .cloned()
        .ok_or_else(|| missing_ref(rest))
}

fn missing_ref(name: &str) -> DesignError {
    DesignError {
        op_index: None,
        op: None,
        code: "MISSING_REF",
        message: format!("Unknown ref '${name}'"),
    }
}

fn resolve_object(
    value: &Value,
    refs: &RefState,
    project: &Project,
) -> Result<ObjectId, DesignError> {
    let raw = value.as_str().ok_or_else(|| DesignError {
        op_index: None,
        op: None,
        code: "INVALID_FIELD",
        message: "Expected object id/ref string".to_string(),
    })?;
    let id = if raw.starts_with('$') {
        match resolve_ref_string(raw, refs)? {
            RefValue::Object(id) => id,
            other => {
                return Err(DesignError {
                    op_index: None,
                    op: None,
                    code: "INVALID_REF_TYPE",
                    message: format!("Expected single object ref, got {:?}", other.to_json()),
                });
            }
        }
    } else {
        parse_id::<beambench_common::markers::ObjectMarker>(raw, "OBJECT_NOT_FOUND")?
    };
    if project.find_object(id).is_none() {
        return Err(DesignError {
            op_index: None,
            op: None,
            code: "OBJECT_NOT_FOUND",
            message: format!("Object not found: {id}"),
        });
    }
    Ok(id)
}

fn object_not_found(id: ObjectId) -> DesignError {
    DesignError {
        op_index: None,
        op: None,
        code: "OBJECT_NOT_FOUND",
        message: format!("Object not found: {id}"),
    }
}

fn validate_object_ids(ids: &[ObjectId], project: &Project) -> Result<(), DesignError> {
    for id in ids {
        if project.find_object(*id).is_none() {
            return Err(object_not_found(*id));
        }
    }
    Ok(())
}

fn find_object_checked(project: &Project, id: ObjectId) -> Result<&ProjectObject, DesignError> {
    project.find_object(id).ok_or_else(|| object_not_found(id))
}

fn find_object_mut_checked(
    project: &mut Project,
    id: ObjectId,
) -> Result<&mut ProjectObject, DesignError> {
    project
        .find_object_mut(id)
        .ok_or_else(|| object_not_found(id))
}

fn resolve_objects(
    value: &Value,
    refs: &RefState,
    project: &Project,
) -> Result<Vec<ObjectId>, DesignError> {
    if let Some(raw) = value.as_str() {
        let ids = if raw.starts_with('$') {
            match resolve_ref_string(raw, refs)? {
                RefValue::Object(id) => Ok(vec![id]),
                RefValue::Objects(ids) => Ok(ids),
                _ => Err(DesignError {
                    op_index: None,
                    op: None,
                    code: "INVALID_REF_TYPE",
                    message: "Expected object or object-set ref".to_string(),
                }),
            }?
        } else {
            vec![parse_id::<beambench_common::markers::ObjectMarker>(
                raw,
                "OBJECT_NOT_FOUND",
            )?]
        };
        validate_object_ids(&ids, project)?;
        return Ok(ids);
    }
    let arr = value.as_array().ok_or_else(|| DesignError {
        op_index: None,
        op: None,
        code: "INVALID_FIELD",
        message: "Expected object id/ref or object id/ref array".to_string(),
    })?;
    let mut ids = Vec::new();
    for item in arr {
        ids.extend(resolve_objects(item, refs, project)?);
    }
    validate_object_ids(&ids, project)?;
    Ok(ids)
}

fn resolve_layer(
    value: &Value,
    refs: &RefState,
    project: &Project,
) -> Result<LayerId, DesignError> {
    let raw = value.as_str().ok_or_else(|| DesignError {
        op_index: None,
        op: None,
        code: "INVALID_FIELD",
        message: "Expected layer id/ref string".to_string(),
    })?;
    let id = if raw.starts_with('$') {
        match resolve_ref_string(raw, refs)? {
            RefValue::Layer(id) => id,
            _ => {
                return Err(DesignError {
                    op_index: None,
                    op: None,
                    code: "INVALID_REF_TYPE",
                    message: "Expected single layer ref".to_string(),
                });
            }
        }
    } else {
        parse_id::<beambench_common::markers::LayerMarker>(raw, "LAYER_NOT_FOUND")?
    };
    if project.find_layer(id).is_none() {
        return Err(DesignError {
            op_index: None,
            op: None,
            code: "LAYER_NOT_FOUND",
            message: format!("Layer not found: {id}"),
        });
    }
    Ok(id)
}

fn resolve_optional_layer(
    value: &Value,
    refs: &RefState,
    project: &mut Project,
) -> Result<LayerId, DesignError> {
    if let Some(layer) = value.get("layer") {
        resolve_layer(layer, refs, project)
    } else {
        Ok(project.ensure_default_layer())
    }
}

fn route_layer_for_target(
    project: &mut Project,
    requested: LayerId,
    target: RoutingTarget,
) -> Result<LayerId, DesignError> {
    crate::validation::resolve_layer_for_object(project, requested, target)
        .map(|(layer_id, _)| layer_id)
        .map_err(design_error_from_service)
}

fn resolve_cut_entry(
    value: &Value,
    refs: &RefState,
    project: &Project,
) -> Result<(LayerId, CutEntryId), DesignError> {
    if let Some(raw) = value.as_str() {
        if raw.starts_with('$') {
            return match resolve_ref_string(raw, refs)? {
                RefValue::CutEntry { layer_id, entry_id } => Ok((layer_id, entry_id)),
                _ => Err(DesignError {
                    op_index: None,
                    op: None,
                    code: "INVALID_REF_TYPE",
                    message: "Expected single cut-entry ref".to_string(),
                }),
            };
        }
        let entry_id = parse_id::<beambench_common::markers::CutEntryMarker>(raw, "INVALID_FIELD")?;
        for layer in &project.layers {
            if layer.entries.iter().any(|entry| entry.id == entry_id) {
                return Ok((layer.id, entry_id));
            }
        }
        return Err(DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: format!("Cut entry not found: {entry_id}"),
        });
    }
    let layer_id = resolve_layer(get_field(value, "layer")?, refs, project)?;
    let layer = project.find_layer(layer_id).ok_or_else(|| DesignError {
        op_index: None,
        op: None,
        code: "LAYER_NOT_FOUND",
        message: format!("Layer not found: {layer_id}"),
    })?;
    if value.get("entry").and_then(Value::as_str) == Some("primary") {
        return layer
            .entries
            .first()
            .map(|entry| (layer_id, entry.id))
            .ok_or_else(|| DesignError {
                op_index: None,
                op: None,
                code: "INVALID_FIELD",
                message: "Layer has no cut entries".to_string(),
            });
    }
    if let Some(idx) = value.get("entry_index").and_then(Value::as_u64) {
        return layer
            .entries
            .get(idx as usize)
            .map(|entry| (layer_id, entry.id))
            .ok_or_else(|| DesignError {
                op_index: None,
                op: None,
                code: "INVALID_FIELD",
                message: format!("Cut entry index {idx} is out of range"),
            });
    }
    if let Some(entry_id) = value.get("entry_id").and_then(Value::as_str) {
        let entry_id =
            parse_id::<beambench_common::markers::CutEntryMarker>(entry_id, "INVALID_FIELD")?;
        if layer.entries.iter().any(|entry| entry.id == entry_id) {
            return Ok((layer_id, entry_id));
        }
    }
    Err(DesignError {
        op_index: None,
        op: None,
        code: "INVALID_FIELD",
        message: "Invalid cut entry selector".to_string(),
    })
}

fn cut_entry_selector_from_op(value: &Value) -> Result<&Value, DesignError> {
    if let Some(entry) = value.get("entry") {
        if entry.as_str() == Some("primary") && value.get("layer").is_some() {
            return Ok(value);
        }
        return Ok(entry);
    }
    if value.get("entry_id").is_some() || value.get("entry_index").is_some() {
        return Ok(value);
    }
    if let Some(target) = value.get("target") {
        return Ok(target);
    }
    Err(DesignError {
        op_index: None,
        op: None,
        code: "INVALID_FIELD",
        message: "Missing cut entry selector".to_string(),
    })
}

fn object_bounds_from_xy(value: &Value, width: f64, height: f64) -> Result<Bounds, DesignError> {
    let x = value.get("x").and_then(Value::as_f64).unwrap_or(0.0);
    let y = value.get("y").and_then(Value::as_f64).unwrap_or(0.0);
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err(DesignError {
            op_index: None,
            op: None,
            code: "INVALID_BOUNDS",
            message: "Object dimensions must be positive finite numbers".to_string(),
        });
    }
    Ok(Bounds::new(
        Point2D::new(x, y),
        Point2D::new(x + width, y + height),
    ))
}

fn create_layer(
    project: &mut Project,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    #[derive(Deserialize)]
    struct Input {
        name: String,
        #[serde(default)]
        operation: OperationType,
        #[serde(default, alias = "color")]
        color_tag: Option<String>,
        #[serde(default)]
        entry_patch: Option<CutEntryPatch>,
    }
    let input: Input = parse(value)?;
    let mut layer = Layer::new_single_entry(input.name, input.operation);
    let color_tag = input
        .color_tag
        .unwrap_or_else(|| DEFAULT_DESIGN_LAYER_COLOR.to_string());
    layer.color_tag = beambench_common::ColorTag(
        beambench_common::canonical_palette_color_tag(&color_tag).into(),
    );
    layer.is_tool_layer = beambench_common::is_tool_color(&layer.color_tag.0);
    if layer.is_tool_layer || layer.primary_entry().operation == OperationType::Tool {
        layer.canonicalize_tool_layer();
    } else if let Some(patch) = input.entry_patch {
        layer.entries[0].apply_patch(&patch);
    }
    let id = project.add_layer(layer).id;
    tracking.layers.push(id);
    Ok(RefValue::Layer(id))
}

fn update_layer(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let layer_id = resolve_layer(get_field(value, "layer")?, refs, project)?;
    let patch: LayerPatch = if let Some(patch) = value.get("patch") {
        parse(patch)?
    } else {
        parse(value)?
    };
    let layer = project
        .find_layer_mut(layer_id)
        .ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "LAYER_NOT_FOUND",
            message: format!("Layer not found: {layer_id}"),
        })?;
    if let Some(name) = patch.name {
        layer.name = name;
    }
    if let Some(enabled) = patch.enabled {
        layer.enabled = enabled;
    }
    if let Some(visible) = patch.visible {
        layer.visible = visible;
    }
    if let Some(color_tag) = patch.color_tag {
        layer.color_tag = beambench_common::ColorTag(
            beambench_common::canonical_palette_color_tag(&color_tag).into(),
        );
        layer.is_tool_layer = beambench_common::is_tool_color(&layer.color_tag.0);
        if layer.is_tool_layer {
            layer.canonicalize_tool_layer();
        }
    }
    tracking.layers.push(layer_id);
    Ok(RefValue::Layer(layer_id))
}

fn delete_layer(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let layer_id = resolve_layer(get_field(value, "layer")?, refs, project)?;
    if !project.remove_layer(layer_id) {
        return Err(DesignError {
            op_index: None,
            op: None,
            code: "LAYER_NOT_FOUND",
            message: format!("Layer not found: {layer_id}"),
        });
    }
    tracking.layers.push(layer_id);
    Ok(RefValue::None)
}

fn reorder_layers(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let layers_value = get_field(value, "layers")?;
    let layer_values = layers_value.as_array().ok_or_else(|| DesignError {
        op_index: None,
        op: None,
        code: "INVALID_FIELD",
        message: "'layers' must be an array".to_string(),
    })?;
    let requested: Vec<LayerId> = layer_values
        .iter()
        .map(|value| resolve_layer(value, refs, project))
        .collect::<Result<_, _>>()?;
    let requested_set: HashSet<LayerId> = requested.iter().copied().collect();
    let mut next = Vec::new();
    for id in &requested {
        let layer = project
            .find_layer(*id)
            .cloned()
            .ok_or_else(|| DesignError {
                op_index: None,
                op: None,
                code: "LAYER_NOT_FOUND",
                message: format!("Layer not found: {id}"),
            })?;
        next.push(layer);
    }
    for layer in &project.layers {
        if !requested_set.contains(&layer.id) {
            next.push(layer.clone());
        }
    }
    project.layers = next;
    for (idx, layer) in project.layers.iter_mut().enumerate() {
        layer.order_index = idx as u32;
    }
    let ids: Vec<LayerId> = project.layers.iter().map(|layer| layer.id).collect();
    tracking.layers.extend(ids.iter().copied());
    Ok(RefValue::Layers(ids))
}

fn add_cut_entry(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let layer_id = resolve_layer(get_field(value, "layer")?, refs, project)?;
    let after_entry_id = value
        .get("after")
        .map(|selector| resolve_cut_entry(selector, refs, project).map(|(_, entry_id)| entry_id))
        .transpose()?;
    let entry = project
        .add_cut_entry(layer_id, after_entry_id)
        .ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: "Failed to add cut entry".to_string(),
        })?;
    tracking.layers.push(layer_id);
    tracking.cut_entries.push(entry.id);
    Ok(RefValue::CutEntry {
        layer_id,
        entry_id: entry.id,
    })
}

fn update_cut_entry(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let selector = cut_entry_selector_from_op(value)?;
    let (layer_id, entry_id) = resolve_cut_entry(selector, refs, project)?;
    let patch: CutEntryPatch =
        if let Some(patch) = value.get("patch").or_else(|| value.get("settings")) {
            parse(patch)?
        } else {
            parse(value)?
        };
    project
        .update_cut_entry(layer_id, entry_id, &patch)
        .ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: "Cut entry not found".to_string(),
        })?;
    tracking.layers.push(layer_id);
    tracking.cut_entries.push(entry_id);
    Ok(RefValue::CutEntry { layer_id, entry_id })
}

fn delete_cut_entry(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let selector = cut_entry_selector_from_op(value)?;
    let (layer_id, entry_id) = resolve_cut_entry(selector, refs, project)?;
    if !project.remove_cut_entry(layer_id, entry_id) {
        return Err(DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: "Failed to delete cut entry".to_string(),
        });
    }
    tracking.layers.push(layer_id);
    tracking.cut_entries.push(entry_id);
    Ok(RefValue::None)
}

fn reorder_cut_entries(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let layer_id = resolve_layer(get_field(value, "layer")?, refs, project)?;
    let entries = get_field(value, "entries")?
        .as_array()
        .ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: "'entries' must be an array".to_string(),
        })?;
    let mut ids = Vec::new();
    for item in entries {
        let (_, entry_id) = resolve_cut_entry(item, refs, project)?;
        ids.push(entry_id);
    }
    let layer = project
        .find_layer_mut(layer_id)
        .ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "LAYER_NOT_FOUND",
            message: format!("Layer not found: {layer_id}"),
        })?;
    let by_id: HashMap<CutEntryId, CutEntry> = layer
        .entries
        .iter()
        .cloned()
        .map(|entry| (entry.id, entry))
        .collect();
    let mut next = Vec::new();
    for id in &ids {
        let entry = by_id.get(id).cloned().ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: format!("Cut entry not found: {id}"),
        })?;
        next.push(entry);
    }
    let requested: HashSet<CutEntryId> = ids.iter().copied().collect();
    for entry in &layer.entries {
        if !requested.contains(&entry.id) {
            next.push(entry.clone());
        }
    }
    layer.entries = next;
    let ordered: Vec<(LayerId, CutEntryId)> = layer
        .entries
        .iter()
        .map(|entry| (layer_id, entry.id))
        .collect();
    tracking.layers.push(layer_id);
    tracking
        .cut_entries
        .extend(ordered.iter().map(|(_, entry_id)| *entry_id));
    Ok(RefValue::CutEntries(ordered))
}

fn add_object(project: &mut Project, obj: ProjectObject, tracking: &mut Tracking) -> RefValue {
    let id = obj.id;
    let layer_id = obj.layer_id;
    project.add_object(obj);
    tracking.created.push(id);
    tracking.layers.push(layer_id);
    RefValue::Object(id)
}

fn create_shape(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    kind: ShapeKind,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let width = value
        .get("width")
        .and_then(Value::as_f64)
        .ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "INVALID_BOUNDS",
            message: "Missing width".to_string(),
        })?;
    let height = value
        .get("height")
        .and_then(Value::as_f64)
        .ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "INVALID_BOUNDS",
            message: "Missing height".to_string(),
        })?;
    let layer_id = resolve_optional_layer(value, refs, project)?;
    let bounds = object_bounds_from_xy(value, width, height)?;
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(match kind {
            ShapeKind::Rectangle => "Rectangle",
            ShapeKind::Ellipse => "Ellipse",
        });
    let obj = ProjectObject::new(
        name,
        layer_id,
        bounds,
        ObjectData::Shape {
            kind,
            width,
            height,
            corner_radius: value
                .get("corner_radius")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
        },
    );
    Ok(add_object(project, obj, tracking))
}

fn create_polygon(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let radius = value.get("radius").and_then(Value::as_f64).unwrap_or(10.0);
    let sides = value.get("sides").and_then(Value::as_u64).unwrap_or(6) as u32;
    let layer_id = resolve_optional_layer(value, refs, project)?;
    let x = value.get("x").and_then(Value::as_f64).unwrap_or(0.0);
    let y = value.get("y").and_then(Value::as_f64).unwrap_or(0.0);
    let bounds = Bounds::new(
        Point2D::new(x - radius, y - radius),
        Point2D::new(x + radius, y + radius),
    );
    let obj = ProjectObject::new(
        value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Polygon"),
        layer_id,
        bounds,
        ObjectData::Polygon { sides, radius },
    );
    Ok(add_object(project, obj, tracking))
}

fn create_star(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let width = value.get("width").and_then(Value::as_f64).unwrap_or(20.0);
    let height = value.get("height").and_then(Value::as_f64).unwrap_or(width);
    let bounds = object_bounds_from_xy(value, width, height)?;
    let layer_id = resolve_optional_layer(value, refs, project)?;
    let obj = ProjectObject::new(
        value.get("name").and_then(Value::as_str).unwrap_or("Star"),
        layer_id,
        bounds,
        ObjectData::Star {
            points: value.get("points").and_then(Value::as_u64).unwrap_or(5) as u32,
            bulge: value.get("bulge").and_then(Value::as_f64).unwrap_or(0.0),
            ratio: value.get("ratio").and_then(Value::as_f64).unwrap_or(0.5),
            dual_radius: value
                .get("dual_radius")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            ratio2: value.get("ratio2").and_then(Value::as_f64),
            corner_radius: value
                .get("corner_radius")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            corner_radii: Vec::new(),
        },
    );
    Ok(add_object(project, obj, tracking))
}

fn create_text(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let content = value
        .get("text")
        .or_else(|| value.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: "create_text requires text".to_string(),
        })?;
    let font_size = value
        .get("font_size_mm")
        .and_then(Value::as_f64)
        .unwrap_or(10.0);
    let width = value
        .get("width")
        .and_then(Value::as_f64)
        .unwrap_or((content.chars().count().max(1) as f64) * font_size * 0.6);
    let height = value
        .get("height")
        .and_then(Value::as_f64)
        .unwrap_or(font_size);
    let bounds = object_bounds_from_xy(value, width, height)?;
    let layer_id = resolve_optional_layer(value, refs, project)?;
    let obj = ProjectObject::new(
        value.get("name").and_then(Value::as_str).unwrap_or("Text"),
        layer_id,
        bounds,
        ObjectData::Text {
            content: content.to_string(),
            font_family: value
                .get("font_family")
                .and_then(Value::as_str)
                .unwrap_or("Arial")
                .to_string(),
            font_size_mm: font_size,
            alignment: TextAlignment::Left,
            alignment_v: TextAlignmentV::Top,
            bold: value.get("bold").and_then(Value::as_bool).unwrap_or(false),
            italic: value
                .get("italic")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            upper_case: value
                .get("upper_case")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            welded: value
                .get("welded")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            h_spacing: value
                .get("h_spacing")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            v_spacing: value
                .get("v_spacing")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            on_path: false,
            path_offset: 0.0,
            distort: false,
            layout_mode: TextLayoutMode::Straight,
            rtl: false,
            bend_radius: value
                .get("bend_radius")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            transform_style: value
                .get("transform_style")
                .cloned()
                .and_then(|value| serde_json::from_value(value).ok())
                .unwrap_or(TextTransformStyle::None),
            transform_curve: value
                .get("transform_curve")
                .and_then(Value::as_f64)
                .map(|curve| curve.clamp(-100.0, 100.0))
                .unwrap_or(0.0),
            circle_placement: value
                .get("circle_placement")
                .cloned()
                .and_then(|value| serde_json::from_value(value).ok())
                .unwrap_or(TextCirclePlacement::TopOutside),
            max_width: value.get("max_width").and_then(Value::as_f64),
            squeeze: false,
            ignore_empty_vars: false,
            resolved_font_source: None,
            resolved_font_key: None,
            resolved_path_data: None,
            missing_font: false,
            missing_glyphs: Vec::new(),
            guide_path_id: None,
            variable_text: None,
        },
    );
    Ok(add_object(project, obj, tracking))
}

fn create_barcode(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let barcode_type: BarcodeType = parse(get_field(value, "barcode_type")?)?;
    let data = value
        .get("data")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let width = value.get("width").and_then(Value::as_f64).unwrap_or(40.0);
    let height = value.get("height").and_then(Value::as_f64).unwrap_or(20.0);
    let bounds = object_bounds_from_xy(value, width, height)?;
    let layer_id = resolve_optional_layer(value, refs, project)?;
    let obj = ProjectObject::new(
        value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Barcode"),
        layer_id,
        bounds,
        ObjectData::Barcode {
            barcode_type,
            data,
            width,
            height,
            options: BarcodeOptions::default(),
        },
    );
    Ok(add_object(project, obj, tracking))
}

fn create_vector_path(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let vec_path = if let Some(svg_d) = value.get("svg_d").and_then(Value::as_str) {
        VecPath::parse_svg_d(svg_d)
    } else if let Some(path) = value.get("vec_path") {
        parse::<VecPath>(path)?
    } else {
        return Err(DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: "create_vector_path requires svg_d or vec_path".to_string(),
        });
    };
    let bounds = vec_path
        .visual_bounds()
        .unwrap_or(Bounds::new(Point2D::zero(), Point2D::zero()));
    let layer_id = resolve_optional_layer(value, refs, project)?;
    let closed = vec_path.subpaths.iter().any(|sp| sp.closed);
    let obj = ProjectObject::new(
        value.get("name").and_then(Value::as_str).unwrap_or("Path"),
        layer_id,
        bounds,
        ObjectData::VectorPath {
            path_data: vec_path.to_svg_d(),
            closed,
            ruler_guide_axis: None,
        },
    );
    Ok(add_object(project, obj, tracking))
}

fn create_image(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    if let Some(path_value) = value.get("path") {
        let path = path_value.as_str().ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: "path must be a string".to_string(),
        })?;
        let bytes = std::fs::read(path).map_err(|e| DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: format!("Failed to read image '{path}': {e}"),
        })?;
        let filename = std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Image");
        let requested_layer_id = resolve_optional_layer(value, refs, project)?;
        let layer_id =
            route_layer_for_target(project, requested_layer_id, RoutingTarget::NeedsImage)?;
        let id = core_import_image(&bytes, filename, Some(path.to_string()), project, layer_id)
            .map_err(|e| DesignError {
                op_index: None,
                op: None,
                code: "INVALID_FIELD",
                message: format!("Failed to import image '{path}': {e}"),
            })?;

        {
            let obj = find_object_mut_checked(project, id)?;
            if let Some(name) = value.get("name").and_then(Value::as_str) {
                obj.name = name.to_string();
            }
            if value.get("x").is_some()
                || value.get("y").is_some()
                || value.get("width").is_some()
                || value.get("height").is_some()
            {
                let width = value
                    .get("width")
                    .and_then(Value::as_f64)
                    .unwrap_or_else(|| obj.bounds.width());
                let height = value
                    .get("height")
                    .and_then(Value::as_f64)
                    .unwrap_or_else(|| obj.bounds.height());
                obj.bounds = object_bounds_from_xy(value, width, height)?;
            }
        }

        tracking.created.push(id);
        tracking.layers.push(layer_id);
        return Ok(RefValue::Object(id));
    }

    let asset_key = get_field(value, "asset_key")?
        .as_str()
        .ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: "asset_key must be a string".to_string(),
        })?
        .to_string();
    let original_width_px = value
        .get("original_width_px")
        .and_then(Value::as_u64)
        .unwrap_or(1) as u32;
    let original_height_px = value
        .get("original_height_px")
        .and_then(Value::as_u64)
        .unwrap_or(1) as u32;
    let width = value
        .get("width")
        .and_then(Value::as_f64)
        .unwrap_or(original_width_px as f64);
    let height = value
        .get("height")
        .and_then(Value::as_f64)
        .unwrap_or(original_height_px as f64);
    let bounds = object_bounds_from_xy(value, width, height)?;
    let requested_layer_id = resolve_optional_layer(value, refs, project)?;
    let layer_id = route_layer_for_target(project, requested_layer_id, RoutingTarget::NeedsImage)?;
    let obj = ProjectObject::new(
        value.get("name").and_then(Value::as_str).unwrap_or("Image"),
        layer_id,
        bounds,
        ObjectData::RasterImage {
            asset_key,
            original_width_px,
            original_height_px,
            adjustments: None,
            masks: Vec::new(),
        },
    );
    Ok(add_object(project, obj, tracking))
}

fn import_svg(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let path = get_field(value, "path")?
        .as_str()
        .ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: "path must be a string".to_string(),
        })?;
    let bytes = std::fs::read(path).map_err(|e| DesignError {
        op_index: None,
        op: None,
        code: "INVALID_FIELD",
        message: format!("Failed to read SVG '{path}': {e}"),
    })?;
    let layer_id = resolve_optional_layer(value, refs, project)?;
    let ids = core_import_svg(&bytes, project, layer_id).map_err(|e| DesignError {
        op_index: None,
        op: None,
        code: "INVALID_FIELD",
        message: format!("Failed to import SVG '{path}': {e}"),
    })?;
    tracking.created.extend(ids.iter().copied());
    Ok(RefValue::Objects(ids))
}

fn update_object(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let id = resolve_object(get_field(value, "object")?, refs, project)?;
    let layer_id = value
        .get("layer")
        .map(|v| resolve_layer(v, refs, project))
        .transpose()?;
    let mut input = UpdateObjectInput {
        layer_id,
        ..Default::default()
    };
    if let Some(name) = value.get("name").and_then(Value::as_str) {
        input.name = Some(name.to_string());
    }
    if let Some(visible) = value.get("visible").and_then(Value::as_bool) {
        input.visible = Some(visible);
    }
    if let Some(locked) = value.get("locked").and_then(Value::as_bool) {
        input.locked = Some(locked);
    }
    if let Some(bounds) = value.get("bounds") {
        input.bounds = Some(parse(bounds)?);
    }
    if let Some(transform) = value.get("transform") {
        input.transform = Some(parse(transform)?);
    }
    if let Some(lock_aspect_ratio) = value.get("lock_aspect_ratio").and_then(Value::as_bool) {
        input.lock_aspect_ratio = Some(lock_aspect_ratio);
    }
    if let Some(power_scale) = value.get("power_scale").and_then(Value::as_f64) {
        input.power_scale = Some(power_scale);
    }
    if let Some(priority) = value.get("priority").and_then(Value::as_i64) {
        input.priority = Some(priority as i32);
    }

    let data = value
        .get("data")
        .or_else(|| value.get("object_data"))
        .map(parse::<ObjectData>)
        .transpose()?;
    let updated = project_ops::update_object_patch_in_project(project, id, input, data)
        .map_err(design_error_from_service)?;

    tracking.modified.push(id);
    if let Some(layer_id) = layer_id {
        tracking.layers.push(layer_id);
    }
    Ok(RefValue::Object(updated.id))
}

fn assign_layer(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let layer_id = resolve_layer(get_field(value, "layer")?, refs, project)?;
    let ids = if let Some(object) = value.get("object") {
        vec![resolve_object(object, refs, project)?]
    } else {
        resolve_objects(get_field(value, "objects")?, refs, project)?
    };
    for id in &ids {
        project_ops::update_object_in_project(
            project,
            *id,
            UpdateObjectInput {
                layer_id: Some(layer_id),
                ..Default::default()
            },
        )
        .map_err(design_error_from_service)?;
    }
    tracking.modified.extend(ids.iter().copied());
    tracking.layers.push(layer_id);
    if ids.len() == 1 {
        Ok(RefValue::Object(ids[0]))
    } else {
        Ok(RefValue::Objects(ids))
    }
}

fn delete_objects(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let ids = if let Some(object) = value.get("object") {
        vec![resolve_object(object, refs, project)?]
    } else {
        resolve_objects(get_field(value, "objects")?, refs, project)?
    };
    project.remove_objects(&ids);
    tracking.deleted.extend(ids);
    Ok(RefValue::None)
}

fn duplicate_objects(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let ids = resolve_objects(
        get_field(value, "objects").or_else(|_| get_field(value, "object"))?,
        refs,
        project,
    )?;
    let dx = value.get("dx").and_then(Value::as_f64).unwrap_or(10.0);
    let dy = value.get("dy").and_then(Value::as_f64).unwrap_or(10.0);
    let mut created_objects = Vec::new();
    let mut duplicate_roots = Vec::new();
    for id in ids {
        let duplicate_root = duplicate_subtree(project, id, &mut created_objects)?;
        let mut duplicate_member_ids = Vec::new();
        collect_group_members(project, duplicate_root, &mut duplicate_member_ids);
        for duplicate_id in duplicate_member_ids {
            let object = find_object_mut_checked(project, duplicate_id)?;
            object.bounds = translate_bounds(object.bounds, dx, dy);
        }
        duplicate_roots.push(duplicate_root);
    }
    tracking
        .created
        .extend(created_objects.iter().map(|object| object.id));
    Ok(RefValue::Objects(duplicate_roots))
}

fn translate_bounds(bounds: Bounds, dx: f64, dy: f64) -> Bounds {
    Bounds::new(
        Point2D::new(bounds.min.x + dx, bounds.min.y + dy),
        Point2D::new(bounds.max.x + dx, bounds.max.y + dy),
    )
}

fn bounds_center(bounds: Bounds) -> Point2D {
    Point2D::new(
        (bounds.min.x + bounds.max.x) / 2.0,
        (bounds.min.y + bounds.max.y) / 2.0,
    )
}

fn reflect_point_across_line(point: Point2D, line_start: Point2D, line_end: Point2D) -> Point2D {
    let dx = line_end.x - line_start.x;
    let dy = line_end.y - line_start.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq <= 1e-12 {
        return point;
    }
    let t = ((point.x - line_start.x) * dx + (point.y - line_start.y) * dy) / len_sq;
    let proj = Point2D::new(line_start.x + t * dx, line_start.y + t * dy);
    Point2D::new(2.0 * proj.x - point.x, 2.0 * proj.y - point.y)
}

fn reflection_transform_for_line(line_start: Point2D, line_end: Point2D) -> Transform2D {
    let angle = (line_end.y - line_start.y).atan2(line_end.x - line_start.x);
    let doubled = 2.0 * angle;
    Transform2D {
        a: doubled.cos(),
        b: doubled.sin(),
        c: doubled.sin(),
        d: -doubled.cos(),
        tx: 0.0,
        ty: 0.0,
    }
}

fn point_lies_on_line(point: Point2D, line_start: Point2D, line_end: Point2D) -> bool {
    let dx = line_end.x - line_start.x;
    let dy = line_end.y - line_start.y;
    let length = (dx * dx + dy * dy).sqrt();
    if length <= 1e-9 {
        return false;
    }
    let cross = ((point.x - line_start.x) * dy - (point.y - line_start.y) * dx).abs();
    cross <= 1e-9_f64.max(length * 1e-6)
}

fn extract_design_mirror_axis_points(
    project: &Project,
    axis_object_id: ObjectId,
) -> Result<(Point2D, Point2D), DesignError> {
    let object = project
        .find_object(axis_object_id)
        .ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "OBJECT_NOT_FOUND",
            message: "Mirror axis object not found".to_string(),
        })?;
    if matches!(
        &object.data,
        ObjectData::VectorPath {
            ruler_guide_axis: Some(_),
            ..
        }
    ) {
        return Err(DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: "Mirror axis must be a separate straight line object".to_string(),
        });
    }
    let world = object_to_world_vecpath_resolved(object, project).ok_or_else(|| DesignError {
        op_index: None,
        op: None,
        code: "INVALID_FIELD",
        message: "Mirror axis object has no vector geometry".to_string(),
    })?;
    if world.subpaths.len() != 1 {
        return Err(DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: "Mirror axis must be a single open straight segment".to_string(),
        });
    }
    let subpath = &world.subpaths[0];
    if subpath.closed || subpath.commands.len() != 2 {
        return Err(DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: "Mirror axis must be a single open straight segment".to_string(),
        });
    }
    let start = match subpath.commands[0] {
        PathCommand::MoveTo { x, y } => Point2D::new(x, y),
        _ => {
            return Err(DesignError {
                op_index: None,
                op: None,
                code: "INVALID_FIELD",
                message: "Mirror axis must be a single open straight segment".to_string(),
            });
        }
    };
    let end = match subpath.commands[1] {
        PathCommand::LineTo { x, y } => Point2D::new(x, y),
        PathCommand::CubicTo {
            c1x,
            c1y,
            c2x,
            c2y,
            x,
            y,
        } => {
            let end = Point2D::new(x, y);
            if !point_lies_on_line(Point2D::new(c1x, c1y), start, end)
                || !point_lies_on_line(Point2D::new(c2x, c2y), start, end)
            {
                return Err(DesignError {
                    op_index: None,
                    op: None,
                    code: "INVALID_FIELD",
                    message: "Mirror axis must be a single open straight segment".to_string(),
                });
            }
            end
        }
        _ => {
            return Err(DesignError {
                op_index: None,
                op: None,
                code: "INVALID_FIELD",
                message: "Mirror axis must be a single open straight segment".to_string(),
            });
        }
    };
    if start.distance_to(&end) <= 1e-9 {
        return Err(DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: "Mirror axis must have non-zero length".to_string(),
        });
    }
    Ok((start, end))
}

fn find_parent_group(project: &Project, child_id: ObjectId) -> Option<ObjectId> {
    project
        .objects
        .iter()
        .find_map(|candidate| match &candidate.data {
            ObjectData::Group { children } if children.contains(&child_id) => Some(candidate.id),
            _ => None,
        })
}

fn top_level_group_for_object(project: &Project, object_id: ObjectId) -> ObjectId {
    let mut current = object_id;
    while let Some(parent) = find_parent_group(project, current) {
        current = parent;
    }
    current
}

fn normalize_design_roots(project: &Project, object_ids: &[ObjectId]) -> Vec<ObjectId> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for object_id in object_ids {
        if project.find_object(*object_id).is_none() {
            continue;
        }
        let promoted = top_level_group_for_object(project, *object_id);
        if seen.insert(promoted) {
            normalized.push(promoted);
        }
    }
    normalized
}

fn collect_group_members(project: &Project, object_id: ObjectId, members: &mut Vec<ObjectId>) {
    members.push(object_id);
    if let Some(object) = project.find_object(object_id) {
        if let ObjectData::Group { children } = &object.data {
            for child in children {
                collect_group_members(project, *child, members);
            }
        }
    }
}

fn duplicate_subtree(
    project: &mut Project,
    object_id: ObjectId,
    created: &mut Vec<ProjectObject>,
) -> Result<ObjectId, DesignError> {
    let original = project
        .find_object(object_id)
        .cloned()
        .ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "OBJECT_NOT_FOUND",
            message: format!("Object not found: {object_id}"),
        })?;
    let mut duplicated = original.clone();
    duplicated.id = ObjectId::new();
    duplicated.name = format!("{} copy", original.name);
    if let ObjectData::Group { children } = &original.data {
        let mut duplicated_children = Vec::with_capacity(children.len());
        for child_id in children {
            duplicated_children.push(duplicate_subtree(project, *child_id, created)?);
        }
        duplicated.data = ObjectData::Group {
            children: duplicated_children,
        };
    }
    let added = project.add_object(duplicated).clone();
    created.push(added.clone());
    Ok(added.id)
}

fn move_objects(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let ids = if let Some(object) = value.get("object") {
        vec![resolve_object(object, refs, project)?]
    } else {
        resolve_objects(get_field(value, "objects")?, refs, project)?
    };
    let dx = value.get("dx").and_then(Value::as_f64).unwrap_or(0.0);
    let dy = value.get("dy").and_then(Value::as_f64).unwrap_or(0.0);
    for id in &ids {
        let obj = find_object_mut_checked(project, *id)?;
        obj.bounds = translate_bounds(obj.bounds, dx, dy);
    }
    tracking.modified.extend(ids.iter().copied());
    if ids.len() == 1 {
        Ok(RefValue::Object(ids[0]))
    } else {
        Ok(RefValue::Objects(ids))
    }
}

fn resize_object(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let id = resolve_object(get_field(value, "object")?, refs, project)?;
    let width = value
        .get("width")
        .and_then(Value::as_f64)
        .ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "INVALID_BOUNDS",
            message: "Missing width".to_string(),
        })?;
    let height = value
        .get("height")
        .and_then(Value::as_f64)
        .ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "INVALID_BOUNDS",
            message: "Missing height".to_string(),
        })?;
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err(DesignError {
            op_index: None,
            op: None,
            code: "INVALID_BOUNDS",
            message: "Object dimensions must be positive finite numbers".to_string(),
        });
    }
    let obj = find_object_mut_checked(project, id)?;
    if matches!(obj.data, ObjectData::Group { .. }) {
        return Err(DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: "resize does not support group objects".to_string(),
        });
    }
    obj.bounds.max = Point2D::new(obj.bounds.min.x + width, obj.bounds.min.y + height);
    if let ObjectData::Shape {
        width: w,
        height: h,
        ..
    } = &mut obj.data
    {
        *w = width;
        *h = height;
    } else if let ObjectData::Barcode {
        width: w,
        height: h,
        ..
    } = &mut obj.data
    {
        *w = width;
        *h = height;
    } else if let ObjectData::Polygon { radius, .. } = &mut obj.data {
        *radius = width.min(height) / 2.0;
    }
    tracking.modified.push(id);
    Ok(RefValue::Object(id))
}

fn transform_objects(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
    kind: &str,
) -> Result<RefValue, DesignError> {
    let ids = if let Some(object) = value.get("object") {
        vec![resolve_object(object, refs, project)?]
    } else {
        resolve_objects(get_field(value, "objects")?, refs, project)?
    };
    let transform = match kind {
        "rotate" => Transform2D::rotate(
            value
                .get("degrees")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                .to_radians(),
        ),
        "shear" => Transform2D::shear(
            value
                .get("x")
                .or_else(|| value.get("sx"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            value
                .get("y")
                .or_else(|| value.get("sy"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
        ),
        "flip" => match value
            .get("axis")
            .and_then(Value::as_str)
            .unwrap_or("horizontal")
        {
            "horizontal" => Transform2D::scale(-1.0, 1.0),
            "vertical" => Transform2D::scale(1.0, -1.0),
            other => {
                return Err(DesignError {
                    op_index: None,
                    op: None,
                    code: "INVALID_FIELD",
                    message: format!("Invalid flip axis '{other}'"),
                });
            }
        },
        _ => Transform2D::identity(),
    };
    for id in &ids {
        let obj = find_object_mut_checked(project, *id)?;
        obj.transform = transform.compose(&obj.transform);
    }
    tracking.modified.extend(ids.iter().copied());
    if ids.len() == 1 {
        Ok(RefValue::Object(ids[0]))
    } else {
        Ok(RefValue::Objects(ids))
    }
}

fn convert_to_path_op(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let object_id = resolve_object(get_field(value, "object")?, refs, project)?;
    let object = convert_to_path_in_project(project, ConvertToPathInput { object_id })
        .map_err(service_to_design)?;
    tracking.modified.push(object.id);
    Ok(RefValue::Object(object.id))
}

fn boolean_op(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
    name: &str,
    op: fn(&VecPath, &VecPath) -> VecPath,
) -> Result<RefValue, DesignError> {
    let object_id_a = resolve_object(get_field(value, "object_a")?, refs, project)?;
    let object_id_b = resolve_object(get_field(value, "object_b")?, refs, project)?;
    let object = boolean_binary_op_in_project(
        project,
        BooleanOpInput {
            object_id_a,
            object_id_b,
        },
        name,
        op,
    )
    .map_err(|e| boolean_error(e))?;
    tracking.deleted.extend([object_id_a, object_id_b]);
    tracking.created.push(object.id);
    Ok(RefValue::Object(object.id))
}

fn boolean_weld_op(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let object_ids = resolve_objects(get_field(value, "objects")?, refs, project)?;
    let object = boolean_weld_in_project(
        project,
        BooleanWeldInput {
            object_ids: object_ids.clone(),
        },
    )
    .map_err(|e| boolean_error(e))?;
    tracking.deleted.extend(object_ids);
    tracking.created.push(object.id);
    Ok(RefValue::Object(object.id))
}

fn boolean_error(error: ServiceError) -> DesignError {
    DesignError {
        op_index: None,
        op: None,
        code: "BOOLEAN_PRECONDITION",
        message: error.message,
    }
}

fn service_to_design(error: ServiceError) -> DesignError {
    DesignError {
        op_index: None,
        op: None,
        code: "INVALID_FIELD",
        message: error.message,
    }
}

fn group_op(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let object_ids = resolve_objects(get_field(value, "objects")?, refs, project)?;
    let object = group_objects_in_project(project, GroupObjectsInput { object_ids })
        .map_err(service_to_design)?;
    tracking.created.push(object.id);
    Ok(RefValue::Object(object.id))
}

fn ungroup_op(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let group_id = resolve_object(get_field(value, "object")?, refs, project)?;
    let children = ungroup_objects_in_project(project, group_id).map_err(service_to_design)?;
    tracking.deleted.push(group_id);
    tracking.modified.extend(children.iter().copied());
    Ok(RefValue::Objects(children))
}

fn vector_path_for_object(project: &Project, object_id: ObjectId) -> Result<VecPath, DesignError> {
    let object = project.find_object(object_id).ok_or_else(|| DesignError {
        op_index: None,
        op: None,
        code: "OBJECT_NOT_FOUND",
        message: format!("Object not found: {object_id}"),
    })?;
    match &object.data {
        ObjectData::VectorPath { path_data, .. } => Ok(VecPath::parse_svg_d(path_data)),
        _ => Err(DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: "Object is not a vector path".to_string(),
        }),
    }
}

fn world_path_for_object(project: &Project, object_id: ObjectId) -> Result<VecPath, DesignError> {
    let object = project.find_object(object_id).ok_or_else(|| DesignError {
        op_index: None,
        op: None,
        code: "OBJECT_NOT_FOUND",
        message: format!("Object not found: {object_id}"),
    })?;
    object_to_world_vecpath_resolved(object, project).ok_or_else(|| DesignError {
        op_index: None,
        op: None,
        code: "INVALID_FIELD",
        message: "Object has no vector geometry".to_string(),
    })
}

fn source_objects(project: &Project, ids: &[ObjectId]) -> Result<Vec<ProjectObject>, DesignError> {
    ids.iter()
        .map(|id| {
            project
                .find_object(*id)
                .cloned()
                .ok_or_else(|| DesignError {
                    op_index: None,
                    op: None,
                    code: "OBJECT_NOT_FOUND",
                    message: format!("Object not found: {id}"),
                })
        })
        .collect()
}

fn insert_generated_objects(
    project: &mut Project,
    generated: Vec<ProjectObject>,
    tracking: &mut Tracking,
) -> Result<Vec<ObjectId>, DesignError> {
    let mut ids = Vec::with_capacity(generated.len());
    for object in generated {
        let id = object.id;
        if project.find_object(id).is_some() {
            let target = project.find_object_mut(id).ok_or_else(|| DesignError {
                op_index: None,
                op: None,
                code: "OBJECT_NOT_FOUND",
                message: format!("Object not found: {id}"),
            })?;
            *target = object;
            tracking.modified.push(id);
        } else {
            project.add_object(object);
            tracking.created.push(id);
        }
        ids.push(id);
    }
    Ok(ids)
}

fn write_path(
    project: &mut Project,
    object_id: ObjectId,
    path: VecPath,
) -> Result<(), DesignError> {
    let bounds = path
        .visual_bounds()
        .unwrap_or(Bounds::new(Point2D::zero(), Point2D::zero()));
    let closed = path.subpaths.iter().any(|sp| sp.closed);
    let obj = project
        .find_object_mut(object_id)
        .ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "OBJECT_NOT_FOUND",
            message: format!("Object not found: {object_id}"),
        })?;
    obj.data = ObjectData::VectorPath {
        path_data: path.to_svg_d(),
        closed,
        ruler_guide_axis: None,
    };
    obj.bounds = bounds;
    obj.tabs.clear();
    Ok(())
}

fn break_apart(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let object_id = resolve_object(get_field(value, "object")?, refs, project)?;
    let source = project
        .find_object(object_id)
        .cloned()
        .ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "OBJECT_NOT_FOUND",
            message: format!("Object not found: {object_id}"),
        })?;
    let ObjectData::VectorPath { path_data, .. } = &source.data else {
        return Err(DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: "break_apart requires a vector path".to_string(),
        });
    };
    let paths = path_ops::break_apart(path_data);
    project.remove_object(object_id);
    tracking.deleted.push(object_id);
    let mut created = Vec::new();
    for (idx, path_data) in paths.into_iter().enumerate() {
        let path = VecPath::parse_svg_d(&path_data);
        let bounds = path.visual_bounds().unwrap_or(source.bounds);
        let closed = path.subpaths.iter().any(|sp| sp.closed);
        let obj = ProjectObject::new(
            format!("{} {}", source.name, idx + 1),
            source.layer_id,
            bounds,
            ObjectData::VectorPath {
                path_data,
                closed,
                ruler_guide_axis: None,
            },
        );
        let id = obj.id;
        project.add_object(obj);
        tracking.created.push(id);
        created.push(id);
    }
    Ok(RefValue::Objects(created))
}

fn offset(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let ids = resolve_objects(
        get_field(value, "objects").or_else(|_| get_field(value, "object"))?,
        refs,
        project,
    )?;
    let distance = value.get("distance").and_then(Value::as_f64).unwrap_or(0.0);
    let direction = match value
        .get("direction")
        .and_then(Value::as_str)
        .unwrap_or("outward")
    {
        "inward" => OffsetDirection::Inward,
        "outward" => OffsetDirection::Outward,
        "both" => OffsetDirection::Both,
        other => {
            return Err(DesignError {
                op_index: None,
                op: None,
                code: "INVALID_FIELD",
                message: format!("Invalid offset direction '{other}'"),
            });
        }
    };
    let corner_style = match value
        .get("corner_style")
        .and_then(Value::as_str)
        .unwrap_or("miter")
    {
        "miter" => CornerStyle::Miter,
        "round" => CornerStyle::Round,
        "bevel" => CornerStyle::Bevel,
        other => {
            return Err(DesignError {
                op_index: None,
                op: None,
                code: "INVALID_FIELD",
                message: format!("Invalid corner style '{other}'"),
            });
        }
    };
    let delete_original = value
        .get("delete_original")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut created = Vec::new();
    for id in ids {
        let source = project
            .find_object(id)
            .cloned()
            .ok_or_else(|| DesignError {
                op_index: None,
                op: None,
                code: "OBJECT_NOT_FOUND",
                message: format!("Object not found: {id}"),
            })?;
        let path = vector_path_for_object(project, id)?;
        for offset_path in offset_path(&path, distance, direction, corner_style) {
            let bounds = offset_path.visual_bounds().unwrap_or(source.bounds);
            let closed = offset_path.subpaths.iter().any(|sp| sp.closed);
            let obj = ProjectObject::new(
                format!("{} Offset", source.name),
                source.layer_id,
                bounds,
                ObjectData::VectorPath {
                    path_data: offset_path.to_svg_d(),
                    closed,
                    ruler_guide_axis: None,
                },
            );
            let new_id = obj.id;
            project.add_object(obj);
            tracking.created.push(new_id);
            created.push(new_id);
        }
        if delete_original {
            project.remove_object(id);
            tracking.deleted.push(id);
        }
    }
    Ok(RefValue::Objects(created))
}

fn grid_array_op(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let ids = resolve_objects(
        get_field(value, "objects").or_else(|_| get_field(value, "object"))?,
        refs,
        project,
    )?;
    let sources = source_objects(project, &ids)?;
    let config: GridArrayConfig = if let Some(config) = value.get("config") {
        parse(config)?
    } else {
        parse(value)?
    };
    let generated = grid_array_in_project(project, &sources, &config);
    Ok(RefValue::Objects(insert_generated_objects(
        project, generated, tracking,
    )?))
}

fn circular_array_op(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let ids = resolve_objects(
        get_field(value, "objects").or_else(|_| get_field(value, "object"))?,
        refs,
        project,
    )?;
    let sources = source_objects(project, &ids)?;
    let config: CircularArrayConfig = if let Some(config) = value.get("config") {
        parse(config)?
    } else {
        parse(value)?
    };
    let generated = circular_array(&sources, &config);
    Ok(RefValue::Objects(insert_generated_objects(
        project, generated, tracking,
    )?))
}

fn copy_along_path_op(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let object_id = resolve_object(get_field(value, "object")?, refs, project)?;
    let guide_id = resolve_object(
        get_field(value, "guide")
            .or_else(|_| get_field(value, "path"))
            .or_else(|_| get_field(value, "guide_path"))?,
        refs,
        project,
    )?;
    let source = project
        .find_object(object_id)
        .cloned()
        .ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "OBJECT_NOT_FOUND",
            message: format!("Object not found: {object_id}"),
        })?;
    let guide = world_path_for_object(project, guide_id)?;
    let spacing = value
        .get("spacing_mm")
        .or_else(|| value.get("spacing"))
        .and_then(Value::as_f64)
        .unwrap_or(10.0);
    let rotate = value
        .get("rotate")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let scale_copies = value
        .get("scale_copies")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let final_scale_percent = value
        .get("final_scale_percent")
        .or_else(|| value.get("final_scale"))
        .and_then(Value::as_f64)
        .unwrap_or(100.0);
    let generated = copy_along_path(
        &source,
        &guide,
        spacing,
        rotate,
        scale_copies,
        final_scale_percent,
    );
    Ok(RefValue::Objects(insert_generated_objects(
        project, generated, tracking,
    )?))
}

fn mirror_across_line(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let axis_id = resolve_object(
        get_field(value, "axis")
            .or_else(|_| get_field(value, "line"))
            .or_else(|_| get_field(value, "axis_object"))?,
        refs,
        project,
    )?;
    let object_ids = resolve_objects(
        get_field(value, "objects").or_else(|_| get_field(value, "object"))?,
        refs,
        project,
    )?;
    let (line_start, line_end) = extract_design_mirror_axis_points(project, axis_id)?;
    let source_ids: Vec<ObjectId> = object_ids.into_iter().filter(|id| *id != axis_id).collect();
    let roots = normalize_design_roots(project, &source_ids);
    if roots.is_empty() {
        return Err(DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: "mirror_across_line requires at least one source object".to_string(),
        });
    }

    let reflection = reflection_transform_for_line(line_start, line_end);
    let mut created = Vec::new();
    let mut duplicate_member_ids = Vec::new();
    for root_id in roots {
        let duplicated_root = duplicate_subtree(project, root_id, &mut created)?;
        collect_group_members(project, duplicated_root, &mut duplicate_member_ids);
    }
    for duplicate_id in &duplicate_member_ids {
        let object = project
            .find_object_mut(*duplicate_id)
            .ok_or_else(|| DesignError {
                op_index: None,
                op: None,
                code: "OBJECT_NOT_FOUND",
                message: "Mirrored duplicate not found".to_string(),
            })?;
        let center = bounds_center(object.bounds);
        let reflected_center = reflect_point_across_line(center, line_start, line_end);
        object.bounds = translate_bounds(
            object.bounds,
            reflected_center.x - center.x,
            reflected_center.y - center.y,
        );
        object.transform = reflection.compose(&object.transform);
    }
    let ids: Vec<ObjectId> = created.iter().map(|object| object.id).collect();
    tracking.created.extend(ids.iter().copied());
    Ok(RefValue::Objects(ids))
}

fn radius(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let object_id = resolve_object(get_field(value, "object")?, refs, project)?;
    let radius = value
        .get("radius_mm")
        .or_else(|| value.get("radius"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let path = vector_path_for_object(project, object_id)?;
    let next = path_ops::apply_radius(&path, radius);
    write_path(project, object_id, next)?;
    tracking.modified.push(object_id);
    Ok(RefValue::Object(object_id))
}

fn tabs(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let object_id = resolve_object(get_field(value, "object")?, refs, project)?;
    let count = value.get("count").and_then(Value::as_u64).unwrap_or(0) as u32;
    let width = value
        .get("width_mm")
        .or_else(|| value.get("width"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let path = vector_path_for_object(project, object_id)?;
    let next = tab_ops::add_tabs(&path, count, width);
    write_path(project, object_id, next)?;
    tracking.modified.push(object_id);
    Ok(RefValue::Object(object_id))
}

fn start_point(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let object_id = resolve_object(get_field(value, "object")?, refs, project)?;
    let x = value.get("x").and_then(Value::as_f64).unwrap_or(0.0);
    let y = value.get("y").and_then(Value::as_f64).unwrap_or(0.0);
    let path = vector_path_for_object(project, object_id)?;
    let next = path_ops::set_start_point(&path, x, y);
    write_path(project, object_id, next)?;
    tracking.modified.push(object_id);
    Ok(RefValue::Object(object_id))
}

fn trim(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let object_id = resolve_object(get_field(value, "object")?, refs, project)?;
    let t_start = value.get("t_start").and_then(Value::as_f64).unwrap_or(0.0);
    let t_end = value.get("t_end").and_then(Value::as_f64).unwrap_or(1.0);
    let path = vector_path_for_object(project, object_id)?;
    let mut parts = beambench_core::vector::trim::trim_at_points(&path, t_start, t_end);
    let next = parts.pop().ok_or_else(|| DesignError {
        op_index: None,
        op: None,
        code: "INVALID_FIELD",
        message: "Trim produced no path".to_string(),
    })?;
    write_path(project, object_id, next)?;
    tracking.modified.push(object_id);
    Ok(RefValue::Object(object_id))
}

fn close_and_join(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let ids = resolve_objects(get_field(value, "objects")?, refs, project)?;
    let tolerance = value
        .get("tolerance")
        .and_then(Value::as_f64)
        .unwrap_or(0.1);
    let mut paths = Vec::new();
    let mut layer_id = None;
    for id in &ids {
        let obj = project.find_object(*id).ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "OBJECT_NOT_FOUND",
            message: format!("Object not found: {id}"),
        })?;
        layer_id.get_or_insert(obj.layer_id);
        paths.push(vector_path_for_object(project, *id)?);
    }
    let result = path_ops::close_and_join(&paths, tolerance).path;
    let bounds = result
        .visual_bounds()
        .unwrap_or(Bounds::new(Point2D::zero(), Point2D::zero()));
    let closed = result.subpaths.iter().any(|sp| sp.closed);
    project.remove_objects(&ids);
    tracking.deleted.extend(ids.iter().copied());
    let obj = ProjectObject::new(
        value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Closed Path"),
        layer_id.unwrap_or_else(|| project.ensure_default_layer()),
        bounds,
        ObjectData::VectorPath {
            path_data: result.to_svg_d(),
            closed,
            ruler_guide_axis: None,
        },
    );
    Ok(add_object(project, obj, tracking))
}

fn rubber_band_outline(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let ids = resolve_objects(
        get_field(value, "objects").or_else(|_| get_field(value, "object"))?,
        refs,
        project,
    )?;
    let sources = source_objects(project, &ids)?;
    let path = core_rubber_band_outline(&sources);
    let bounds = path
        .visual_bounds()
        .unwrap_or(Bounds::new(Point2D::zero(), Point2D::zero()));
    let layer_id = value
        .get("layer")
        .map(|layer| resolve_layer(layer, refs, project))
        .transpose()?
        .unwrap_or_else(|| {
            sources
                .first()
                .map(|object| object.layer_id)
                .unwrap_or_else(|| project.ensure_default_layer())
        });
    let obj = ProjectObject::new(
        value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Rubber Band Outline"),
        layer_id,
        bounds,
        ObjectData::VectorPath {
            path_data: path.to_svg_d(),
            closed: path.subpaths.iter().any(|sp| sp.closed),
            ruler_guide_axis: None,
        },
    );
    Ok(add_object(project, obj, tracking))
}

fn apply_path_to_text(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let text_id = resolve_object(
        get_field(value, "text").or_else(|_| get_field(value, "object"))?,
        refs,
        project,
    )?;
    let guide_id = resolve_object(
        get_field(value, "path")
            .or_else(|_| get_field(value, "guide"))
            .or_else(|_| get_field(value, "guide_path"))?,
        refs,
        project,
    )?;
    let guide = world_path_for_object(project, guide_id)?;
    let text = project
        .find_object(text_id)
        .cloned()
        .ok_or_else(|| DesignError {
            op_index: None,
            op: None,
            code: "OBJECT_NOT_FOUND",
            message: format!("Text object not found: {text_id}"),
        })?;
    let ObjectData::Text {
        content,
        font_family,
        font_size_mm,
        bold,
        italic,
        ..
    } = &text.data
    else {
        return Err(DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: "apply_path_to_text requires a text object".to_string(),
        });
    };
    let path = apply_path_to_text_with_options(
        content,
        font_family,
        *font_size_mm,
        *bold,
        *italic,
        &guide,
    );
    write_path(project, text_id, path)?;
    tracking.modified.push(text_id);
    Ok(RefValue::Object(text_id))
}

fn align(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let ids = resolve_objects(get_field(value, "objects")?, refs, project)?;
    if ids.len() < 2 {
        return Ok(RefValue::Objects(Vec::new()));
    }
    let mode = value
        .get("alignment")
        .or_else(|| value.get("alignment_type"))
        .and_then(Value::as_str)
        .unwrap_or("left");
    let anchor_id = value
        .get("anchor")
        .map(|v| resolve_object(v, refs, project))
        .transpose()?
        .unwrap_or(*ids.last().unwrap());
    let anchor = find_object_checked(project, anchor_id)?.bounds;
    for id in &ids {
        if *id == anchor_id {
            continue;
        }
        let obj = find_object_mut_checked(project, *id)?;
        let dx = match mode {
            "left" => anchor.min.x - obj.bounds.min.x,
            "right" => anchor.max.x - obj.bounds.max.x,
            "centers_h" | "centers_x" => {
                ((anchor.min.x + anchor.max.x) - (obj.bounds.min.x + obj.bounds.max.x)) / 2.0
            }
            _ => 0.0,
        };
        let dy = match mode {
            "top" => anchor.min.y - obj.bounds.min.y,
            "bottom" => anchor.max.y - obj.bounds.max.y,
            "centers_v" | "centers_y" => {
                ((anchor.min.y + anchor.max.y) - (obj.bounds.min.y + obj.bounds.max.y)) / 2.0
            }
            "centers_xy" => {
                ((anchor.min.y + anchor.max.y) - (obj.bounds.min.y + obj.bounds.max.y)) / 2.0
            }
            _ => 0.0,
        };
        let dx = if mode == "centers_xy" {
            ((anchor.min.x + anchor.max.x) - (obj.bounds.min.x + obj.bounds.max.x)) / 2.0
        } else {
            dx
        };
        obj.bounds = translate_bounds(obj.bounds, dx, dy);
    }
    tracking.modified.extend(ids.iter().copied());
    Ok(RefValue::Objects(ids))
}

fn distribute(
    project: &mut Project,
    refs: &RefState,
    value: &Value,
    tracking: &mut Tracking,
) -> Result<RefValue, DesignError> {
    let mut ids = resolve_objects(get_field(value, "objects")?, refs, project)?;
    if ids.len() < 3 {
        return Err(DesignError {
            op_index: None,
            op: None,
            code: "INVALID_FIELD",
            message: "Distribution requires at least three objects".to_string(),
        });
    }
    let direction = value
        .get("direction")
        .and_then(Value::as_str)
        .unwrap_or("h_spaced");
    ids.sort_by(|a, b| {
        let ab = project.find_object(*a).map(|object| object.bounds);
        let bb = project.find_object(*b).map(|object| object.bounds);
        let (Some(ab), Some(bb)) = (ab, bb) else {
            return std::cmp::Ordering::Equal;
        };
        let av = if direction.starts_with('h') {
            ab.min.x
        } else {
            ab.min.y
        };
        let bv = if direction.starts_with('h') {
            bb.min.x
        } else {
            bb.min.y
        };
        av.partial_cmp(&bv).unwrap_or(std::cmp::Ordering::Equal)
    });
    let first = find_object_checked(project, ids[0])?.bounds;
    let last = find_object_checked(project, *ids.last().unwrap())?.bounds;
    let total_span = if direction.starts_with('h') {
        last.min.x - first.min.x
    } else {
        last.min.y - first.min.y
    };
    let step = total_span / (ids.len() - 1) as f64;
    for (idx, id) in ids.iter().enumerate().skip(1).take(ids.len() - 2) {
        let obj = find_object_mut_checked(project, *id)?;
        if direction.starts_with('h') {
            let target = first.min.x + step * idx as f64;
            obj.bounds = translate_bounds(obj.bounds, target - obj.bounds.min.x, 0.0);
        } else {
            let target = first.min.y + step * idx as f64;
            obj.bounds = translate_bounds(obj.bounds, 0.0, target - obj.bounds.min.y);
        }
    }
    tracking.modified.extend(ids.iter().copied());
    Ok(RefValue::Objects(ids))
}

fn bed_bounds(project: &Project) -> Bounds {
    let (width, height) = project
        .machine_profile_snapshot
        .as_ref()
        .map(|snapshot| (snapshot.bed_width_mm, snapshot.bed_height_mm))
        .unwrap_or((
            project.workspace.bed_width_mm,
            project.workspace.bed_height_mm,
        ));
    Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(width, height))
}

fn bounds_inside(bounds: Bounds, bed: Bounds) -> bool {
    bounds.min.x >= bed.min.x
        && bounds.min.y >= bed.min.y
        && bounds.max.x <= bed.max.x
        && bounds.max.y <= bed.max.y
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::project::undo_project;

    fn ctx_with_project() -> ServiceContext {
        let ctx = ServiceContext::new();
        *ctx.project.lock().unwrap() = Some(Project::new("Design Test"));
        ctx.clear_project_history().unwrap();
        ctx
    }

    fn plan(operations: Value) -> DesignPlan {
        serde_json::from_value(json!({
            "operations": operations,
            "options": {
                "validate_bounds": true,
                "allow_out_of_bounds": false
            }
        }))
        .unwrap()
    }

    fn rectangle_op(name: &str) -> Value {
        json!({
            "op": "create_rectangle",
            "ref": name,
            "x": 10.0,
            "y": 10.0,
            "width": 20.0,
            "height": 15.0
        })
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-6,
            "expected {expected}, got {actual}"
        );
    }

    fn assert_bounds_close(actual: Bounds, expected: Bounds) {
        assert_close(actual.min.x, expected.min.x);
        assert_close(actual.min.y, expected.min.y);
        assert_close(actual.max.x, expected.max.x);
        assert_close(actual.max.y, expected.max.y);
    }

    fn visual_bounds(project: &Project, id: ObjectId) -> Bounds {
        let object = project.find_object(id).unwrap();
        object_to_world_vecpath_resolved(object, project)
            .and_then(|path| path.visual_bounds())
            .unwrap()
    }

    #[test]
    fn schema_lists_documented_v1_operations() {
        let schema = schema();
        let ops: HashSet<String> = schema["operations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|op| op["name"].as_str().unwrap().to_string())
            .collect();
        for name in [
            "create_layer",
            "update_layer",
            "delete_layer",
            "reorder_layers",
            "add_cut_entry",
            "update_cut_entry",
            "delete_cut_entry",
            "reorder_cut_entries",
            "create_rectangle",
            "create_ellipse",
            "create_polygon",
            "create_star",
            "create_text",
            "create_barcode",
            "create_vector_path",
            "create_image",
            "import_svg",
            "update_object",
            "assign_layer",
            "delete_object",
            "duplicate_objects",
            "move",
            "resize",
            "rotate",
            "shear",
            "flip",
            "convert_to_path",
            "boolean_union",
            "boolean_subtract",
            "boolean_intersection",
            "boolean_exclude",
            "boolean_weld",
            "group",
            "ungroup",
            "break_apart",
            "offset",
            "grid_array",
            "circular_array",
            "copy_along_path",
            "mirror_across_line",
            "radius",
            "fillet",
            "tabs",
            "start_point",
            "trim",
            "close_and_join",
            "rubber_band_outline",
            "apply_path_to_text",
            "align",
            "distribute",
        ] {
            assert!(ops.contains(name), "missing design op schema for {name}");
        }
    }

    #[test]
    fn create_layer_defaults_to_palette_black() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([
                {
                    "op": "create_layer",
                    "name": "CLI Layer"
                }
            ])),
            TransactionMode::Apply,
        );
        assert!(result.error.is_none(), "{:?}", result.error);
        let project = ctx.project.lock().unwrap();
        let layer = project.as_ref().unwrap().layers.first().unwrap();
        assert_eq!(layer.color_tag.0, DEFAULT_DESIGN_LAYER_COLOR);
        assert!(!layer.is_tool_layer);
    }

    #[test]
    fn create_layer_accepts_documented_color_field() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([
                {
                    "op": "create_layer",
                    "name": "Red Layer",
                    "color": "#FF0000"
                }
            ])),
            TransactionMode::Apply,
        );
        assert!(result.error.is_none(), "{:?}", result.error);
        let project = ctx.project.lock().unwrap();
        let layer = project.as_ref().unwrap().layers.first().unwrap();
        assert_eq!(layer.color_tag.0, "#FF0000");
    }

    #[test]
    fn create_layer_accepts_palette_name_and_snaps_unknown_hex() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([
                {
                    "op": "create_layer",
                    "name": "Named Green",
                    "color": "green"
                },
                {
                    "op": "create_layer",
                    "name": "Approx Gray",
                    "color": "#666666"
                }
            ])),
            TransactionMode::Apply,
        );
        assert!(result.error.is_none(), "{:?}", result.error);
        let project = ctx.project.lock().unwrap();
        let layers = &project.as_ref().unwrap().layers;
        assert_eq!(layers[0].color_tag.0, "#00FF00");
        assert_eq!(layers[1].color_tag.0, "#808080");
    }

    #[test]
    fn update_layer_accepts_documented_color_field() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([
                {
                    "op": "create_layer",
                    "name": "Layer",
                    "ref": "layer"
                },
                {
                    "op": "update_layer",
                    "layer": "$layer",
                    "color": "#0000FF"
                }
            ])),
            TransactionMode::Apply,
        );
        assert!(result.error.is_none(), "{:?}", result.error);
        let project = ctx.project.lock().unwrap();
        let layer = project.as_ref().unwrap().layers.first().unwrap();
        assert_eq!(layer.color_tag.0, "#0000FF");
    }

    #[test]
    fn dry_run_executes_without_mutating_project_or_undo() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([
                rectangle_op("box"),
                {
                    "op": "duplicate_objects",
                    "ref": "dupes",
                    "objects": "$box",
                    "dx": 5.0,
                    "dy": 0.0
                }
            ])),
            TransactionMode::Plan,
        );
        assert!(result.error.is_none(), "{:?}", result.error);
        assert!(!result.applied);
        assert_eq!(result.summary.created_object_ids.len(), 2);
        let project = ctx.project.lock().unwrap();
        assert_eq!(project.as_ref().unwrap().objects.len(), 0);
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn apply_commits_once_sets_dirty_and_undo_restores_original_project() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([rectangle_op("box")])),
            TransactionMode::Apply,
        );
        assert!(result.error.is_none(), "{:?}", result.error);
        assert!(result.applied);
        {
            let project = ctx.project.lock().unwrap();
            let project = project.as_ref().unwrap();
            assert_eq!(project.objects.len(), 1);
            assert!(project.dirty);
        }
        assert!(ctx.undo_state().unwrap().can_undo);
        undo_project(&ctx).unwrap();
        assert_eq!(
            ctx.project.lock().unwrap().as_ref().unwrap().objects.len(),
            0
        );
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn apply_failure_rolls_back_and_does_not_push_undo() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([
                rectangle_op("box"),
                {
                    "op": "update_object",
                    "object": "00000000-0000-0000-0000-000000000000",
                    "name": "missing"
                }
            ])),
            TransactionMode::Apply,
        );
        let error = result.error.unwrap();
        assert_eq!(error.op_index, Some(1));
        assert_eq!(error.code, "OBJECT_NOT_FOUND");
        assert_eq!(
            ctx.project.lock().unwrap().as_ref().unwrap().objects.len(),
            0
        );
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn strict_ref_typing_rejects_multi_output_ref_in_single_object_slot() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([
                rectangle_op("box"),
                {
                    "op": "duplicate_objects",
                    "ref": "dupes",
                    "objects": "$box",
                    "dx": 5.0,
                    "dy": 0.0
                },
                {
                    "op": "update_object",
                    "object": "$dupes",
                    "name": "bad"
                }
            ])),
            TransactionMode::Plan,
        );
        let error = result.error.unwrap();
        assert_eq!(error.op_index, Some(2));
        assert_eq!(error.code, "INVALID_REF_TYPE");
    }

    #[test]
    fn create_text_uses_documented_text_field() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([{
                "op": "create_text",
                "ref": "label",
                "text": "Hello",
                "x": 5.0,
                "y": 6.0
            }])),
            TransactionMode::Apply,
        );
        assert!(result.error.is_none(), "{:?}", result.error);
        let project = ctx.project.lock().unwrap();
        let object = project.as_ref().unwrap().objects.first().unwrap();
        match &object.data {
            ObjectData::Text { content, .. } => assert_eq!(content, "Hello"),
            other => panic!("expected text object, got {other:?}"),
        }
    }

    #[test]
    fn create_barcode_schema_example_uses_runtime_value() {
        let ctx = ctx_with_project();
        let schema = schema();
        let example = schema["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|op| op["name"] == "create_barcode")
            .unwrap()["example"]
            .clone();
        let result = run_transaction(&ctx, plan(json!([example])), TransactionMode::Apply);
        assert!(result.error.is_none(), "{:?}", result.error);

        let project = ctx.project.lock().unwrap();
        let object = project.as_ref().unwrap().objects.first().unwrap();
        match &object.data {
            ObjectData::Barcode { barcode_type, .. } => {
                assert_eq!(barcode_type, &BarcodeType::QrCode)
            }
            other => panic!("expected barcode object, got {other:?}"),
        }
    }

    #[test]
    fn grid_array_accepts_documented_snake_case_fields() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([
                rectangle_op("box"),
                {
                    "op": "grid_array",
                    "objects": "$box",
                    "rows": 2,
                    "cols": 2,
                    "h_spacing_mm": 15.0,
                    "v_spacing_mm": 20.0,
                    "spacing_mode": "centerToCenter",
                    "ref": "grid"
                }
            ])),
            TransactionMode::Apply,
        );
        assert!(result.error.is_none(), "{:?}", result.error);
        let project = ctx.project.lock().unwrap();
        assert_eq!(project.as_ref().unwrap().objects.len(), 4);
    }

    #[test]
    fn create_image_accepts_local_file_path() {
        let ctx = ctx_with_project();
        let image_path =
            std::env::temp_dir().join(format!("beambench-design-{}.png", Uuid::new_v4()));
        let mut png = Vec::new();
        image::ImageEncoder::write_image(
            image::codecs::png::PngEncoder::new(std::io::Cursor::new(&mut png)),
            &[255, 0, 0, 255],
            1,
            1,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
        std::fs::write(&image_path, png).unwrap();

        let result = run_transaction(
            &ctx,
            plan(json!([{
                "op": "create_image",
                "path": image_path.to_string_lossy().to_string(),
                "x": 3.0,
                "y": 4.0,
                "width": 7.0,
                "height": 8.0,
                "ref": "image"
            }])),
            TransactionMode::Apply,
        );
        let _ = std::fs::remove_file(&image_path);
        assert!(result.error.is_none(), "{:?}", result.error);

        let project = ctx.project.lock().unwrap();
        let project = project.as_ref().unwrap();
        assert_eq!(project.assets.len(), 1);
        let object = project.objects.first().unwrap();
        assert_eq!(object.bounds.min, Point2D::new(3.0, 4.0));
        assert_eq!(object.bounds.max, Point2D::new(10.0, 12.0));
        let layer = project.find_layer(object.layer_id).unwrap();
        assert_eq!(layer.primary_entry().operation, OperationType::Image);
        match &object.data {
            ObjectData::RasterImage {
                asset_key,
                original_width_px,
                original_height_px,
                ..
            } => {
                assert_eq!(asset_key, &project.assets[0].id.to_string());
                assert_eq!((*original_width_px, *original_height_px), (1, 1));
            }
            other => panic!("expected raster image object, got {other:?}"),
        }
    }

    #[test]
    fn update_object_uses_project_text_bounds_update_behavior() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([
                {
                    "op": "create_text",
                    "ref": "label",
                    "text": "Hello",
                    "x": 0.0,
                    "y": 0.0,
                    "width": 100.0,
                    "height": 20.0,
                    "font_size_mm": 10.0,
                    "h_spacing": 2.0,
                    "v_spacing": 1.0,
                    "max_width": 50.0
                },
                {
                    "op": "update_object",
                    "object": "$label",
                    "bounds": {
                        "min": { "x": 0.0, "y": 0.0 },
                        "max": { "x": 200.0, "y": 40.0 }
                    }
                }
            ])),
            TransactionMode::Apply,
        );
        assert!(result.error.is_none(), "{:?}", result.error);

        let project = ctx.project.lock().unwrap();
        let object = project.as_ref().unwrap().objects.first().unwrap();
        match &object.data {
            ObjectData::Text {
                font_size_mm,
                h_spacing,
                v_spacing,
                max_width,
                ..
            } => {
                assert!((font_size_mm - 20.0).abs() < 1e-6);
                assert!((h_spacing - 4.0).abs() < 1e-6);
                assert!((v_spacing - 2.0).abs() < 1e-6);
                assert!((max_width.unwrap() - 100.0).abs() < 1e-6);
            }
            other => panic!("expected text object, got {other:?}"),
        }
    }

    #[test]
    fn shear_uses_documented_x_y_fields() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([
                rectangle_op("box"),
                {
                    "op": "shear",
                    "object": "$box",
                    "x": 0.25,
                    "y": 0.5
                }
            ])),
            TransactionMode::Apply,
        );
        assert!(result.error.is_none(), "{:?}", result.error);
        let project = ctx.project.lock().unwrap();
        let object = project.as_ref().unwrap().objects.first().unwrap();
        assert_eq!(object.transform.c, 0.25);
        assert_eq!(object.transform.b, 0.5);
    }

    #[test]
    fn update_cut_entry_uses_documented_entry_selector() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([
                {
                    "op": "create_layer",
                    "ref": "layer",
                    "name": "Cut"
                },
                {
                    "op": "update_cut_entry",
                    "entry": { "layer": "$layer", "entry": "primary" },
                    "speed_mm_min": 2222.0
                }
            ])),
            TransactionMode::Apply,
        );
        assert!(result.error.is_none(), "{:?}", result.error);
        let project = ctx.project.lock().unwrap();
        let layer = project
            .as_ref()
            .unwrap()
            .layers
            .iter()
            .find(|layer| layer.name == "Cut")
            .unwrap();
        assert_eq!(layer.entries[0].speed_mm_min, 2222.0);
    }

    #[test]
    fn update_cut_entry_uses_documented_settings_field() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([
                {
                    "op": "create_layer",
                    "ref": "layer",
                    "name": "Cut"
                },
                {
                    "op": "update_cut_entry",
                    "entry": { "layer": "$layer", "entry": "primary" },
                    "settings": {
                        "speed_mm_min": 6000.0,
                        "power_percent": 1.0
                    }
                }
            ])),
            TransactionMode::Apply,
        );
        assert!(result.error.is_none(), "{:?}", result.error);
        let project = ctx.project.lock().unwrap();
        let layer = project
            .as_ref()
            .unwrap()
            .layers
            .iter()
            .find(|layer| layer.name == "Cut")
            .unwrap();
        assert_eq!(layer.entries[0].speed_mm_min, 6000.0);
        assert_eq!(layer.entries[0].power_percent, 1.0);
    }

    #[test]
    fn delete_cut_entry_uses_documented_entry_selector() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([
                {
                    "op": "create_layer",
                    "ref": "layer",
                    "name": "Cut"
                },
                {
                    "op": "add_cut_entry",
                    "layer": "$layer",
                    "ref": "extra"
                },
                {
                    "op": "delete_cut_entry",
                    "entry": { "layer": "$layer", "entry_index": 1 }
                }
            ])),
            TransactionMode::Apply,
        );
        assert!(result.error.is_none(), "{:?}", result.error);
        let project = ctx.project.lock().unwrap();
        let layer = project
            .as_ref()
            .unwrap()
            .layers
            .iter()
            .find(|layer| layer.name == "Cut")
            .unwrap();
        assert_eq!(layer.entries.len(), 1);
    }

    #[test]
    fn created_ref_after_delete_returns_structured_error() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([
                rectangle_op("box"),
                {
                    "op": "delete_object",
                    "object": "$box"
                },
                {
                    "op": "move",
                    "objects": "$created",
                    "dx": 1.0,
                    "dy": 1.0
                }
            ])),
            TransactionMode::Apply,
        );
        let error = result.error.unwrap();
        assert_eq!(error.op_index, Some(2));
        assert_eq!(error.code, "OBJECT_NOT_FOUND");
        assert_eq!(
            ctx.project.lock().unwrap().as_ref().unwrap().objects.len(),
            0
        );
    }

    #[test]
    fn last_ref_can_address_multi_output_previous_op() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([
                rectangle_op("box"),
                {
                    "op": "duplicate_objects",
                    "objects": "$box",
                    "dx": 5.0,
                    "dy": 0.0
                },
                {
                    "op": "move",
                    "objects": "$last",
                    "dx": 1.0,
                    "dy": 1.0
                }
            ])),
            TransactionMode::Plan,
        );
        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(result.summary.created_object_ids.len(), 2);
        assert_eq!(result.summary.modified_object_ids.len(), 1);
    }

    #[test]
    fn indexed_ref_can_select_one_multi_output_object() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([
                rectangle_op("box"),
                {
                    "op": "duplicate_objects",
                    "ref": "dupes",
                    "objects": "$box",
                    "dx": 5.0,
                    "dy": 0.0
                },
                {
                    "op": "update_object",
                    "object": "$dupes[0]",
                    "name": "First copy"
                }
            ])),
            TransactionMode::Apply,
        );
        assert!(result.error.is_none(), "{:?}", result.error);
        let project = ctx.project.lock().unwrap();
        assert!(
            project
                .as_ref()
                .unwrap()
                .objects
                .iter()
                .any(|object| object.name == "First copy")
        );
    }

    #[test]
    fn set_ref_can_feed_multi_object_slot() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([
                rectangle_op("box"),
                {
                    "op": "duplicate_objects",
                    "ref": "dupes",
                    "objects": "$box",
                    "dx": 5.0,
                    "dy": 0.0
                },
                {
                    "op": "move",
                    "objects": "$dupes[*]",
                    "dx": 1.0,
                    "dy": 1.0
                }
            ])),
            TransactionMode::Plan,
        );
        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(result.summary.modified_object_ids.len(), 1);
    }

    #[test]
    fn move_with_transform_offset_moves_visual_geometry_once() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([
                {
                    "op": "create_rectangle",
                    "ref": "box",
                    "x": 0.0,
                    "y": 0.0,
                    "width": 10.0,
                    "height": 10.0
                },
                {
                    "op": "update_object",
                    "object": "$box",
                    "transform": { "a": 1.0, "b": 0.0, "c": 0.0, "d": 1.0, "tx": 3.0, "ty": 4.0 }
                },
                {
                    "op": "move",
                    "object": "$box",
                    "dx": 10.0,
                    "dy": 20.0
                }
            ])),
            TransactionMode::Apply,
        );
        assert!(result.error.is_none(), "{:?}", result.error);

        let project = ctx.project.lock().unwrap();
        let project = project.as_ref().unwrap();
        let object = project.objects.first().unwrap();
        assert_close(object.transform.tx, 3.0);
        assert_close(object.transform.ty, 4.0);
        assert_bounds_close(
            visual_bounds(project, object.id),
            Bounds::new(Point2D::new(13.0, 24.0), Point2D::new(23.0, 34.0)),
        );
    }

    #[test]
    fn duplicate_with_transform_offset_deep_copies_group_members() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([
                {
                    "op": "create_rectangle",
                    "ref": "a",
                    "x": 0.0,
                    "y": 0.0,
                    "width": 10.0,
                    "height": 10.0
                },
                {
                    "op": "create_rectangle",
                    "ref": "b",
                    "x": 20.0,
                    "y": 0.0,
                    "width": 10.0,
                    "height": 10.0
                },
                {
                    "op": "update_object",
                    "object": "$a",
                    "transform": { "a": 1.0, "b": 0.0, "c": 0.0, "d": 1.0, "tx": 3.0, "ty": 4.0 }
                },
                {
                    "op": "group",
                    "objects": "$created",
                    "ref": "group"
                },
                {
                    "op": "duplicate_objects",
                    "object": "$group",
                    "dx": 100.0,
                    "dy": 50.0
                }
            ])),
            TransactionMode::Apply,
        );
        assert!(result.error.is_none(), "{:?}", result.error);

        let project = ctx.project.lock().unwrap();
        let project = project.as_ref().unwrap();
        let original_group = project
            .objects
            .iter()
            .find(|object| object.name == "Group")
            .unwrap();
        let duplicate_group = project
            .objects
            .iter()
            .find(|object| object.name == "Group copy")
            .unwrap();
        let ObjectData::Group {
            children: original_children,
        } = &original_group.data
        else {
            panic!("expected original group");
        };
        let ObjectData::Group {
            children: duplicate_children,
        } = &duplicate_group.data
        else {
            panic!("expected duplicate group");
        };
        assert_eq!(original_children.len(), 2);
        assert_eq!(duplicate_children.len(), 2);
        for id in duplicate_children {
            assert!(!original_children.contains(id));
            assert!(project.find_object(*id).is_some());
        }

        let moved_transformed_child = duplicate_children
            .iter()
            .filter_map(|id| project.find_object(*id))
            .find(|object| object.transform.tx.abs() > 0.0)
            .unwrap();
        assert_close(moved_transformed_child.transform.tx, 3.0);
        assert_close(moved_transformed_child.transform.ty, 4.0);
        assert_bounds_close(
            visual_bounds(project, moved_transformed_child.id),
            Bounds::new(Point2D::new(103.0, 54.0), Point2D::new(113.0, 64.0)),
        );
    }

    #[test]
    fn mirror_across_line_reflects_transformed_visual_geometry() {
        let ctx = ctx_with_project();
        let result = run_transaction(
            &ctx,
            plan(json!([
                {
                    "op": "create_rectangle",
                    "ref": "box",
                    "x": 10.0,
                    "y": 0.0,
                    "width": 10.0,
                    "height": 10.0
                },
                {
                    "op": "update_object",
                    "object": "$box",
                    "transform": { "a": 1.0, "b": 0.0, "c": 0.0, "d": 1.0, "tx": 3.0, "ty": 4.0 }
                },
                {
                    "op": "create_vector_path",
                    "ref": "axis",
                    "svg_d": "M100 0 L100 50"
                },
                {
                    "op": "mirror_across_line",
                    "objects": "$box",
                    "axis": "$axis"
                }
            ])),
            TransactionMode::Apply,
        );
        assert!(result.error.is_none(), "{:?}", result.error);

        let project = ctx.project.lock().unwrap();
        let project = project.as_ref().unwrap();
        let mirrored = project
            .objects
            .iter()
            .find(|object| object.name == "Rectangle copy")
            .unwrap();
        assert_close(mirrored.transform.tx, -3.0);
        assert_close(mirrored.transform.ty, 4.0);
        assert_bounds_close(
            visual_bounds(project, mirrored.id),
            Bounds::new(Point2D::new(177.0, 4.0), Point2D::new(187.0, 14.0)),
        );
    }
}
