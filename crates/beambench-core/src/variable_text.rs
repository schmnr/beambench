//! Variable text support — merge fields and template resolution for batch production.

use std::collections::HashMap;

use crate::ProjectObject;
use crate::layer::OperationType;
use crate::project::Project;
use serde::{Deserialize, Deserializer, Serialize};

/// A merge field placeholder found in text content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MergeField {
    /// Auto-incrementing serial number: {Serial}
    Serial {
        start: u64,
        increment: u64,
        padding: u8,
    },
    /// Date/time: {Date:format}
    Date { format: String },
    /// CSV column reference: {CSV:column_name}
    CsvColumn { column: String },
    /// Fixed text constant: {Const:name}
    Constant { name: String, value: String },
}

/// A variable text configuration stored on a Text object — template + full source context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableTextConfig {
    /// The original template with merge field placeholders, e.g. "SN-{Serial}".
    pub template: String,
    #[serde(default)]
    pub mode: Option<VariableTextMode>,
    #[serde(default)]
    pub offset: Option<i64>,
    /// Full source context: csv_data, field_defaults, csv_path, etc.
    pub source: VariableTextSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableTextMode {
    Normal,
    SerialNumber,
    DateTime,
    MergeCsv,
    CutSetting,
}

/// A variable text data source for batch production.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableTextSource {
    pub csv_path: Option<String>,
    pub csv_data: Vec<Vec<String>>,
    pub field_defaults: HashMap<String, String>,
    pub current: i64,
    pub start: i64,
    pub end: i64,
    pub advance_by: i64,
    #[serde(default)]
    pub auto_advance: bool,
    pub total_copies: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VariableTextSourceSerde {
    #[serde(default)]
    csv_path: Option<String>,
    #[serde(default)]
    csv_data: Vec<Vec<String>>,
    #[serde(default)]
    field_defaults: HashMap<String, String>,
    #[serde(default)]
    current: Option<i64>,
    #[serde(default)]
    start: Option<i64>,
    #[serde(default)]
    end: Option<i64>,
    #[serde(default, alias = "advanceBy")]
    advance_by: Option<i64>,
    #[serde(default)]
    auto_advance: Option<bool>,
    #[serde(default, alias = "currentRow")]
    current_row: Option<usize>,
    #[serde(default)]
    total_copies: Option<u64>,
}

impl<'de> Deserialize<'de> for VariableTextSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = VariableTextSourceSerde::deserialize(deserializer)?;
        let csv_row_count = raw.csv_data.len().saturating_sub(1) as i64;
        let legacy_start = raw
            .field_defaults
            .get("_serial_start")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(1);
        let legacy_advance = raw
            .field_defaults
            .get("_serial_increment")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(1);
        let start = raw.start.unwrap_or(legacy_start);
        let current = raw.current.unwrap_or_else(|| {
            if let Some(current_row) = raw.current_row {
                current_row as i64
            } else if csv_row_count > 0 {
                0
            } else {
                start
            }
        });
        let end = raw.end.unwrap_or_else(|| {
            if csv_row_count > 0 {
                csv_row_count.saturating_sub(1)
            } else {
                start
            }
        });

        Ok(Self {
            csv_path: raw.csv_path,
            csv_data: raw.csv_data,
            field_defaults: raw.field_defaults,
            current,
            start,
            end,
            advance_by: raw.advance_by.unwrap_or(legacy_advance),
            auto_advance: raw.auto_advance.unwrap_or(false),
            total_copies: raw.total_copies.unwrap_or(1),
        })
    }
}

impl Default for VariableTextSource {
    fn default() -> Self {
        Self {
            csv_path: None,
            csv_data: Vec::new(),
            field_defaults: HashMap::new(),
            current: 1,
            start: 1,
            end: 1,
            advance_by: 1,
            auto_advance: false,
            total_copies: 1,
        }
    }
}

/// Info about a detected merge field in template text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeFieldInfo {
    pub start: usize,
    pub end: usize,
    pub field_name: String,
}

/// Parse merge field placeholders from text content.
/// Recognizes `{Serial}`, `{Date:format}`, `{CSV:column}`, `{Const:name}`.
pub fn parse_merge_fields(text: &str) -> Vec<MergeFieldInfo> {
    let mut fields = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if let Some(close) = text[i..].find('}') {
                let end = i + close + 1;
                let inner = &text[i + 1..end - 1];
                if !inner.is_empty() && !inner.contains('{') {
                    fields.push(MergeFieldInfo {
                        start: i,
                        end,
                        field_name: inner.to_string(),
                    });
                }
                i = end;
            } else {
                break;
            }
        } else {
            i += 1;
        }
    }
    fields
}

/// Resolve all merge fields in text content against a source.
pub fn resolve_text(template: &str, source: &VariableTextSource, row: usize) -> String {
    let fields = parse_merge_fields(template);
    if fields.is_empty() {
        return template.to_string();
    }

    let mut result = String::with_capacity(template.len());
    let mut last_end = 0;

    for field in &fields {
        result.push_str(&template[last_end..field.start]);
        let replacement = resolve_field(&field.field_name, source, row);
        result.push_str(&replacement);
        last_end = field.end;
    }
    result.push_str(&template[last_end..]);
    result
}

fn resolve_field(field_name: &str, source: &VariableTextSource, row: usize) -> String {
    if field_name == "Serial" {
        let padding = source
            .field_defaults
            .get("_serial_padding")
            .and_then(|s| s.parse::<u8>().ok())
            .unwrap_or(1);
        let value = wrap_sequence_value(source.current, source.start, source.end);
        return format_serial(value, padding);
    }

    if let Some(params) = field_name.strip_prefix("Serial:") {
        let parts: Vec<&str> = params.split(',').collect();
        let start = parts
            .first()
            .and_then(|s| s.trim().parse::<i64>().ok())
            .unwrap_or(1);
        let increment = parts
            .get(1)
            .and_then(|s| s.trim().parse::<i64>().ok())
            .unwrap_or(1);
        let padding = parts
            .get(2)
            .and_then(|s| s.trim().parse::<u8>().ok())
            .unwrap_or(1);
        return resolve_serial(start, increment, padding, row);
    }

    if let Some(fmt) = field_name.strip_prefix("Date:") {
        return resolve_date(fmt);
    }

    if field_name == "Date" {
        return resolve_date("YYYY-MM-DD");
    }

    if let Some(col_name) = field_name.strip_prefix("CSV:") {
        return resolve_csv_column(col_name, source);
    }

    if let Some(const_name) = field_name.strip_prefix("Const:")
        && let Some(val) = source.field_defaults.get(const_name)
    {
        return val.clone();
    }

    if let Some(cut_name) = field_name.strip_prefix("Cut:") {
        return resolve_cut_placeholder(cut_name, source)
            .unwrap_or_else(|| format!("{{{field_name}}}"));
    }

    // Unknown field — leave placeholder as-is
    format!("{{{field_name}}}")
}

fn resolve_serial(start: i64, increment: i64, padding: u8, row: usize) -> String {
    let value = start + (row as i64) * increment;
    format_serial(value, padding)
}

fn format_serial(value: i64, padding: u8) -> String {
    if value < 0 {
        format!("-{:0>width$}", value.abs(), width = padding as usize)
    } else {
        format!("{:0>width$}", value, width = padding as usize)
    }
}

fn resolve_date(format_str: &str) -> String {
    let now = chrono::Local::now();
    let mut result = format_str.to_string();
    result = result.replace("YYYY", &now.format("%Y").to_string());
    result = result.replace("YY", &now.format("%y").to_string());
    result = result.replace("MM", &now.format("%m").to_string());
    result = result.replace("DD", &now.format("%d").to_string());
    result = result.replace("HH", &now.format("%H").to_string());
    result = result.replace("mm", &now.format("%M").to_string());
    result = result.replace("ss", &now.format("%S").to_string());
    result
}

/// CSV resolution reads from `source.current`, not the `row` parameter.
/// Callers must set `source.current` to the desired CSV data row index
/// before calling `resolve_text`. The `row` parameter passed to `resolve_text`
/// is the copy index, used only by `{Serial:params}` for progression.
fn resolve_csv_column(column_name: &str, source: &VariableTextSource) -> String {
    if source.csv_data.is_empty() {
        return format!("{{CSV:{column_name}}}");
    }

    let headers = &source.csv_data[0];
    let col_idx = headers.iter().position(|h| h == column_name);

    if let Some(idx) = col_idx {
        let data_row = wrap_csv_row(source.current, source.csv_data.len() - 1) + 1; // +1 to skip header row
        if data_row < source.csv_data.len()
            && let Some(val) = source.csv_data[data_row].get(idx)
        {
            return val.clone();
        }
    }

    format!("{{CSV:{column_name}}}")
}

fn wrap_csv_row(current: i64, row_count: usize) -> usize {
    if row_count == 0 {
        return 0;
    }
    current.rem_euclid(row_count as i64) as usize
}

pub fn wrap_sequence_value(value: i64, start: i64, end: i64) -> i64 {
    if start == end {
        // M8 compatibility: a degenerate range means "unbounded sequence"
        // for legacy/new serial workflows until the user sets a real range.
        return value;
    }

    let (start, end) = if end < start {
        (end, start)
    } else {
        (start, end)
    };
    let range = (end - start + 1).max(1);
    start + (value - start).rem_euclid(range)
}

pub fn advance_sequence_value(current: i64, start: i64, end: i64, delta: i64) -> i64 {
    wrap_sequence_value(current + delta, start, end)
}

pub fn resolve_text_in_project(
    project: &Project,
    object: &ProjectObject,
    config: &VariableTextConfig,
    row: usize,
) -> String {
    if matches!(config.mode, Some(VariableTextMode::Normal)) {
        return config.template.clone();
    }

    let mut source = config.source.clone();
    if source.end < source.start {
        std::mem::swap(&mut source.start, &mut source.end);
    }

    if matches!(
        config.mode,
        None | Some(VariableTextMode::SerialNumber) | Some(VariableTextMode::MergeCsv)
    ) {
        let offset = config.offset.unwrap_or(0);
        source.current = advance_sequence_value(source.current, source.start, source.end, offset);
    }

    if matches!(config.mode, Some(VariableTextMode::CutSetting)) {
        inject_cut_context(project, object, &mut source);
    }
    resolve_text(&config.template, &source, row)
}

fn inject_cut_context(project: &Project, object: &ProjectObject, source: &mut VariableTextSource) {
    let Some(layer) = project
        .layers
        .iter()
        .find(|layer| layer.id == object.layer_id)
    else {
        return;
    };
    let entry = layer.primary_entry();
    let machine_name = project
        .machine_profile_snapshot
        .as_ref()
        .map(|snapshot| snapshot.profile_name.clone())
        .unwrap_or_default();

    insert_cut_default(source, "LayerName", layer.name.clone());
    insert_cut_default(source, "MachineName", machine_name);
    insert_cut_default(source, "Operation", operation_display(entry.operation));
    insert_cut_default(source, "Speed", trim_float(entry.speed_mm_min));
    insert_cut_default(
        source,
        "SpeedWithUnits",
        format!("{} mm/min", trim_float(entry.speed_mm_min)),
    );
    insert_cut_default(source, "PowerMax", trim_float(entry.power_percent));
    insert_cut_default(
        source,
        "PowerMaxPct",
        format!("{}%", trim_float(entry.power_percent)),
    );
    insert_cut_default(source, "PowerMin", trim_float(entry.power_min_percent));
    insert_cut_default(
        source,
        "PowerMinPct",
        format!("{}%", trim_float(entry.power_min_percent)),
    );
    insert_cut_default(source, "Passes", entry.passes_for_operation().to_string());
    insert_cut_default(source, "ZOffset", trim_float(entry.z_offset_mm));
    insert_cut_default(
        source,
        "ZOffsetWithUnits",
        format!("{} mm", trim_float(entry.z_offset_mm)),
    );

    if entry.operation.uses_raster_settings() {
        if let Some(raster) = entry.raster_settings.as_ref() {
            insert_cut_default(source, "DPI", raster.effective_dpi().to_string());
            insert_cut_default(
                source,
                "Interval",
                trim_float(raster.effective_line_interval_mm()),
            );
            insert_cut_default(
                source,
                "IntervalWithUnits",
                format!("{} mm", trim_float(raster.effective_line_interval_mm())),
            );
        }
    } else {
        insert_cut_default(source, "DPI", String::new());
        insert_cut_default(source, "Interval", String::new());
        insert_cut_default(source, "IntervalWithUnits", String::new());
    }
}

fn insert_cut_default(source: &mut VariableTextSource, name: &str, value: String) {
    source.field_defaults.insert(format!("_cut_{name}"), value);
}

fn resolve_cut_placeholder(name: &str, source: &VariableTextSource) -> Option<String> {
    match name {
        "LayerName" | "MachineName" | "Operation" | "Speed" | "SpeedWithUnits" | "PowerMax"
        | "PowerMaxPct" | "PowerMin" | "PowerMinPct" | "Passes" | "DPI" | "Interval"
        | "IntervalWithUnits" | "ZOffset" | "ZOffsetWithUnits" => Some(
            source
                .field_defaults
                .get(&format!("_cut_{name}"))
                .cloned()
                .unwrap_or_default(),
        ),
        _ => None,
    }
}

fn operation_display(operation: OperationType) -> String {
    match operation {
        OperationType::Image => "Image",
        OperationType::Line => "Line",
        OperationType::Fill => "Fill",
        OperationType::Score => "Score",
        OperationType::Cut => "Cut",
        OperationType::OffsetFill => "Offset Fill",
        OperationType::Tool => "Tool",
    }
    .to_string()
}

fn trim_float(value: f64) -> String {
    let mut rendered = format!("{value:.4}");
    while rendered.contains('.') && rendered.ends_with('0') {
        rendered.pop();
    }
    if rendered.ends_with('.') {
        rendered.pop();
    }
    rendered
}

/// Parse a CSV string into rows (header + data).
pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, String> {
    if input.trim().is_empty() {
        return Err("Empty CSV input".to_string());
    }

    let mut rows: Vec<Vec<String>> = Vec::new();

    for line in input.lines() {
        let row = parse_csv_line(line);
        rows.push(row);
    }

    if rows.is_empty() {
        return Err("No rows found in CSV".to_string());
    }

    Ok(rows)
}

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    current.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                current.push(ch);
            }
        } else if ch == '"' {
            in_quotes = true;
        } else if ch == ',' {
            fields.push(current.clone());
            current.clear();
        } else {
            current.push(ch);
        }
    }
    fields.push(current);
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::{Bounds, Point2D};

    use crate::layer::{Layer, OperationType};
    use crate::machine_profile::MachineProfile;
    use crate::object::{ObjectData, ProjectObject, TextAlignment, TextAlignmentV, TextLayoutMode};
    use crate::project::Project;

    // --- Serialization round-trip tests ---

    #[test]
    fn merge_field_serial_serde_roundtrip() {
        let field = MergeField::Serial {
            start: 1,
            increment: 1,
            padding: 4,
        };
        let json = serde_json::to_string(&field).unwrap();
        let parsed: MergeField = serde_json::from_str(&json).unwrap();
        assert_eq!(field, parsed);
    }

    #[test]
    fn merge_field_date_serde_roundtrip() {
        let field = MergeField::Date {
            format: "YYYY-MM-DD".to_string(),
        };
        let json = serde_json::to_string(&field).unwrap();
        let parsed: MergeField = serde_json::from_str(&json).unwrap();
        assert_eq!(field, parsed);
    }

    #[test]
    fn merge_field_csv_column_serde_roundtrip() {
        let field = MergeField::CsvColumn {
            column: "Name".to_string(),
        };
        let json = serde_json::to_string(&field).unwrap();
        let parsed: MergeField = serde_json::from_str(&json).unwrap();
        assert_eq!(field, parsed);
    }

    #[test]
    fn merge_field_constant_serde_roundtrip() {
        let field = MergeField::Constant {
            name: "company".to_string(),
            value: "Acme".to_string(),
        };
        let json = serde_json::to_string(&field).unwrap();
        let parsed: MergeField = serde_json::from_str(&json).unwrap();
        assert_eq!(field, parsed);
    }

    #[test]
    fn variable_text_source_serde_roundtrip() {
        let mut defaults = HashMap::new();
        defaults.insert("company".to_string(), "Acme".to_string());
        let source = VariableTextSource {
            csv_path: Some("/tmp/data.csv".to_string()),
            csv_data: vec![
                vec!["Name".to_string(), "Value".to_string()],
                vec!["Alice".to_string(), "100".to_string()],
            ],
            field_defaults: defaults,
            current: 0,
            start: 0,
            end: 0,
            advance_by: 1,
            auto_advance: false,
            total_copies: 10,
        };
        let json = serde_json::to_string(&source).unwrap();
        let parsed: VariableTextSource = serde_json::from_str(&json).unwrap();
        assert_eq!(source, parsed);
    }

    #[test]
    fn variable_text_source_serde_without_csv() {
        let source = VariableTextSource::default();
        let json = serde_json::to_string(&source).unwrap();
        let parsed: VariableTextSource = serde_json::from_str(&json).unwrap();
        assert_eq!(source, parsed);
    }

    #[test]
    fn variable_text_source_backward_compat() {
        let json =
            r#"{"csvPath":null,"csvData":[],"fieldDefaults":{},"currentRow":0,"totalCopies":1}"#;
        let parsed: VariableTextSource = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.total_copies, 1);
        assert!(parsed.csv_data.is_empty());
        assert_eq!(parsed.current, 0);
    }

    #[test]
    fn variable_text_source_backward_compat_prefers_csv_current_row_when_present() {
        let json = r#"{
            "csvPath":"/tmp/data.csv",
            "csvData":[["Name"],["Alice"],["Bob"]],
            "fieldDefaults":{"_serial_start":"50","_serial_increment":"2"},
            "currentRow":1,
            "totalCopies":2
        }"#;
        let parsed: VariableTextSource = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.start, 50);
        assert_eq!(parsed.advance_by, 2);
        assert_eq!(parsed.current, 1);
        assert_eq!(parsed.end, 1);
    }

    #[test]
    fn wrap_sequence_value_degenerate_range_keeps_serial_unbounded() {
        assert_eq!(wrap_sequence_value(5, 1, 1), 5);
        assert_eq!(advance_sequence_value(5, 1, 1, 2), 7);
    }

    #[test]
    fn advance_sequence_value_wraps_negative_offsets_for_real_ranges() {
        assert_eq!(advance_sequence_value(10, 10, 15, -1), 15);
        assert_eq!(advance_sequence_value(12, 10, 15, 4), 10);
    }

    fn make_variable_text_project(
        operation: OperationType,
    ) -> (Project, ProjectObject, VariableTextConfig) {
        let mut project = Project::new("Variable Text");
        let mut layer = Layer::new("Layer A", operation);
        {
            let entry = layer.primary_entry_mut();
            entry.speed_mm_min = 600.0;
            entry.power_percent = 80.0;
            entry.power_min_percent = 15.0;
            entry.z_offset_mm = 1.5;
            if operation.uses_raster_settings() {
                entry.ensure_raster_settings();
                if let Some(raster) = entry.raster_settings.as_mut() {
                    raster.dpi = 254;
                    raster.line_interval_mm = 0.2;
                    raster.passes = 3;
                }
            } else if let Some(vector) = entry.vector_settings.as_mut() {
                vector.passes = 2;
            }
        }
        let layer_id = layer.id;
        project.add_layer(layer);

        let object = ProjectObject::new(
            "Text",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 5.0)),
            ObjectData::Text {
                content: "literal".to_string(),
                font_family: "Arial".to_string(),
                font_size_mm: 5.0,
                alignment: TextAlignment::Left,
                alignment_v: TextAlignmentV::Top,
                bold: false,
                italic: false,
                upper_case: false,
                welded: false,
                h_spacing: 0.0,
                v_spacing: 0.0,
                on_path: false,
                path_offset: 0.0,
                distort: false,
                layout_mode: TextLayoutMode::Straight,
                rtl: false,
                bend_radius: 0.0,
                transform_style: crate::object::TextTransformStyle::None,
                transform_curve: 0.0,
                circle_placement: crate::object::TextCirclePlacement::TopOutside,
                max_width: None,
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

        let config = VariableTextConfig {
            template: "SN-{Serial}".to_string(),
            mode: Some(VariableTextMode::SerialNumber),
            offset: Some(-1),
            source: VariableTextSource {
                current: 10,
                start: 10,
                end: 15,
                ..Default::default()
            },
        };

        (project, object, config)
    }

    #[test]
    fn resolve_text_in_project_normal_mode_returns_literal_template() {
        let (project, object, mut config) = make_variable_text_project(OperationType::Cut);
        config.mode = Some(VariableTextMode::Normal);
        config.template = "Value {Serial} {Cut:LayerName}".to_string();
        assert_eq!(
            resolve_text_in_project(&project, &object, &config, 0),
            "Value {Serial} {Cut:LayerName}"
        );
    }

    #[test]
    fn resolve_text_in_project_applies_negative_offset_for_serial_mode() {
        let (project, object, config) = make_variable_text_project(OperationType::Cut);
        assert_eq!(
            resolve_text_in_project(&project, &object, &config, 0),
            "SN-15"
        );
    }

    #[test]
    fn resolve_text_in_project_injects_cut_context_and_machine_name() {
        let (mut project, object, _) = make_variable_text_project(OperationType::Fill);
        let mut machine = MachineProfile::default();
        machine.name = "Beam Box".to_string();
        project.machine_profile_snapshot = Some(machine.snapshot());

        let config = VariableTextConfig {
            template: "{Cut:LayerName}|{Cut:MachineName}|{Cut:Operation}|{Cut:SpeedWithUnits}|{Cut:Passes}|{Cut:DPI}|{Cut:IntervalWithUnits}|{Cut:ZOffsetWithUnits}".to_string(),
            mode: Some(VariableTextMode::CutSetting),
            offset: Some(3),
            source: VariableTextSource::default(),
        };

        assert_eq!(
            resolve_text_in_project(&project, &object, &config, 0),
            "Layer A|Beam Box|Fill|600 mm/min|3|127|0.2 mm|1.5 mm"
        );
    }

    #[test]
    fn resolve_text_in_project_leaves_machine_name_empty_without_snapshot() {
        let (project, object, _) = make_variable_text_project(OperationType::Cut);
        let config = VariableTextConfig {
            template: "{Cut:MachineName}|{Cut:DPI}|{Cut:Passes}".to_string(),
            mode: Some(VariableTextMode::CutSetting),
            offset: None,
            source: VariableTextSource::default(),
        };

        assert_eq!(
            resolve_text_in_project(&project, &object, &config, 0),
            "||2"
        );
    }

    // --- Parser tests ---

    #[test]
    fn parse_single_serial_field() {
        let fields = parse_merge_fields("Hello {Serial} world");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_name, "Serial");
        assert_eq!(fields[0].start, 6);
        assert_eq!(fields[0].end, 14);
    }

    #[test]
    fn parse_no_fields() {
        let fields = parse_merge_fields("No fields here");
        assert!(fields.is_empty());
    }

    #[test]
    fn parse_multiple_fields() {
        let fields = parse_merge_fields("{CSV:Name} - {Date:YYYY}");
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].field_name, "CSV:Name");
        assert_eq!(fields[1].field_name, "Date:YYYY");
    }

    #[test]
    fn parse_adjacent_fields() {
        let fields = parse_merge_fields("{Serial}{Date:YYYY}");
        assert_eq!(fields.len(), 2);
    }

    #[test]
    fn parse_empty_braces_ignored() {
        let fields = parse_merge_fields("empty {} braces");
        assert!(fields.is_empty());
    }

    #[test]
    fn resolve_serial_default() {
        let source = VariableTextSource::default();
        let result = resolve_text("SN-{Serial}", &source, 0);
        assert_eq!(result, "SN-1");
    }

    #[test]
    fn resolve_serial_increments_via_current_bump() {
        let mut source = VariableTextSource::default();
        let r0 = resolve_text("SN-{Serial}", &source, 0);
        assert_eq!(r0, "SN-1");
        source.current = 2;
        source.start = 1;
        source.end = 100;
        let r1 = resolve_text("SN-{Serial}", &source, 0);
        assert_eq!(r1, "SN-2");
        source.current = 3;
        let r2 = resolve_text("SN-{Serial}", &source, 0);
        assert_eq!(r2, "SN-3");
    }

    #[test]
    fn resolve_serial_row_does_not_affect_serial_field() {
        // Passing different rows should not change {Serial} output
        let source = VariableTextSource::default();
        let r0 = resolve_text("SN-{Serial}", &source, 0);
        let r5 = resolve_text("SN-{Serial}", &source, 5);
        assert_eq!(r0, "SN-1");
        assert_eq!(r5, "SN-1");
    }

    #[test]
    fn resolve_serial_reads_field_defaults() {
        let mut source = VariableTextSource::default();
        source.current = 100;
        source.start = 100;
        source.end = 9999;
        source
            .field_defaults
            .insert("_serial_padding".to_string(), "4".to_string());
        let r0 = resolve_text("SN-{Serial}", &source, 0);
        assert_eq!(r0, "SN-0100");
        source.current = 105;
        let r1 = resolve_text("SN-{Serial}", &source, 0);
        assert_eq!(r1, "SN-0105");
    }

    #[test]
    fn resolve_serial_with_params() {
        let source = VariableTextSource::default();
        let result = resolve_text("{Serial:100,10,5}", &source, 0);
        assert_eq!(result, "00100");
        let result2 = resolve_text("{Serial:100,10,5}", &source, 1);
        assert_eq!(result2, "00110");
    }

    #[test]
    fn resolve_csv_column() {
        let mut source = VariableTextSource {
            csv_data: vec![
                vec!["Name".to_string(), "City".to_string()],
                vec!["Alice".to_string(), "Boston".to_string()],
                vec!["Bob".to_string(), "Denver".to_string()],
            ],
            ..Default::default()
        };
        // CSV resolution uses source.current, not the row parameter
        source.current = 0;
        let result = resolve_text("Hello {CSV:Name} from {CSV:City}", &source, 0);
        assert_eq!(result, "Hello Alice from Boston");
        source.current = 1;
        let result2 = resolve_text("Hello {CSV:Name} from {CSV:City}", &source, 0);
        assert_eq!(result2, "Hello Bob from Denver");
    }

    #[test]
    fn resolve_date_field() {
        let source = VariableTextSource::default();
        let result = resolve_text("Date: {Date:YYYY-MM-DD}", &source, 0);
        assert!(!result.contains("{Date:"));
        assert!(result.starts_with("Date: "));
        let date_part = &result[6..];
        assert_eq!(date_part.len(), 10);
        assert_eq!(&date_part[4..5], "-");
        assert_eq!(&date_part[7..8], "-");
    }

    #[test]
    fn resolve_constant_field() {
        let mut defaults = HashMap::new();
        defaults.insert("company".to_string(), "Acme Corp".to_string());
        let source = VariableTextSource {
            field_defaults: defaults,
            ..Default::default()
        };
        let result = resolve_text("Made by {Const:company}", &source, 0);
        assert_eq!(result, "Made by Acme Corp");
    }

    #[test]
    fn resolve_unknown_field_left_as_is() {
        let source = VariableTextSource::default();
        let result = resolve_text("Hello {Unknown}", &source, 0);
        assert_eq!(result, "Hello {Unknown}");
    }

    #[test]
    fn resolve_no_fields_returns_original() {
        let source = VariableTextSource::default();
        let result = resolve_text("Plain text", &source, 0);
        assert_eq!(result, "Plain text");
    }

    // --- CSV parser tests ---

    #[test]
    fn parse_csv_basic() {
        let input = "Name,City,Age\nAlice,Boston,30\nBob,Denver,25";
        let rows = parse_csv(input).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], vec!["Name", "City", "Age"]);
        assert_eq!(rows[1], vec!["Alice", "Boston", "30"]);
        assert_eq!(rows[2], vec!["Bob", "Denver", "25"]);
    }

    #[test]
    fn parse_csv_quoted_fields() {
        let input = "Name,Description\nAlice,\"Has a, comma\"\nBob,\"He said \"\"hi\"\"\"";
        let rows = parse_csv(input).unwrap();
        assert_eq!(rows[1][1], "Has a, comma");
        assert_eq!(rows[2][1], "He said \"hi\"");
    }

    #[test]
    fn parse_csv_empty_fields() {
        let input = "A,B,C\n1,,3\n,,";
        let rows = parse_csv(input).unwrap();
        assert_eq!(rows[1], vec!["1", "", "3"]);
        assert_eq!(rows[2], vec!["", "", ""]);
    }

    #[test]
    fn parse_csv_empty_input_errors() {
        let result = parse_csv("");
        assert!(result.is_err());
    }

    #[test]
    fn parse_csv_single_column() {
        let input = "Name\nAlice\nBob";
        let rows = parse_csv(input).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], vec!["Name"]);
    }

    /// Exercises the exact batch resolution pattern: bump current per copy,
    /// then resolve with row=i. Serial must not double-count (1,2,3 not 1,3,5).
    #[test]
    fn batch_serial_no_double_increment() {
        let mut source = VariableTextSource::default();
        source.current = 1;
        source.start = 1;
        source.end = 99;
        source.advance_by = 1;
        source
            .field_defaults
            .insert("_serial_start".to_string(), "1".to_string());
        source
            .field_defaults
            .insert("_serial_increment".to_string(), "1".to_string());
        source
            .field_defaults
            .insert("_serial_padding".to_string(), "1".to_string());

        let template = "SN-{Serial}";
        let mut results = Vec::new();
        for i in 0..5_usize {
            let mut copy_source = source.clone();
            copy_source.current = source.current + i as i64;
            // Batch command passes row=i (for CSV); {Serial} must ignore row
            results.push(resolve_text(template, &copy_source, i));
        }
        assert_eq!(results, vec!["SN-1", "SN-2", "SN-3", "SN-4", "SN-5"]);
    }

    /// Mixed {Serial} + {CSV:...} template: serial uses current,
    /// CSV uses source.current. No cross-contamination.
    #[test]
    fn batch_mixed_serial_csv_no_double_increment() {
        let mut source = VariableTextSource::default();
        source.current = 100;
        source.start = 100;
        source.end = 999;
        source.csv_data = vec![
            vec!["Name".to_string()],
            vec!["Alice".to_string()],
            vec!["Bob".to_string()],
            vec!["Carol".to_string()],
        ];

        let template = "{Serial}-{CSV:Name}";
        let mut results = Vec::new();
        for i in 0..3_usize {
            let mut copy_source = source.clone();
            copy_source.current = source.current + i as i64;
            results.push(resolve_text(template, &copy_source, i));
        }
        assert_eq!(results, vec!["100-Bob", "101-Carol", "102-Alice"]);
    }

    /// {Serial:params} inline variant must progress via row, not _serial_start.
    /// This verifies that array auto-increment passes copy_index as row.
    #[test]
    fn serial_params_variant_progresses_via_row() {
        let source = VariableTextSource::default();
        // {Serial:100,10,5} with different row values
        let r0 = resolve_text("ID-{Serial:100,10,5}", &source, 0);
        let r1 = resolve_text("ID-{Serial:100,10,5}", &source, 1);
        let r2 = resolve_text("ID-{Serial:100,10,5}", &source, 2);
        assert_eq!(r0, "ID-00100");
        assert_eq!(r1, "ID-00110");
        assert_eq!(r2, "ID-00120");
    }

    /// {Serial:params} with CSV must NOT wrap when CSV rows wrap —
    /// serial continues 100, 101, 102, 103, 104 across 5 copies with 3 CSV rows.
    #[test]
    fn serial_params_with_csv_does_not_wrap() {
        let mut source = VariableTextSource::default();
        source.csv_data = vec![
            vec!["Name".to_string()],
            vec!["Alice".to_string()],
            vec!["Bob".to_string()],
            vec!["Carol".to_string()],
        ];
        let csv_row_count = 3;
        let template = "{Serial:100,1,3}-{CSV:Name}";
        let mut results = Vec::new();
        for i in 0..5_usize {
            let mut s = source.clone();
            s.current = (i % csv_row_count) as i64;
            // copy_index drives {Serial:params}; current drives {CSV:}
            results.push(resolve_text(template, &s, i));
        }
        assert_eq!(
            results,
            vec!["100-Alice", "101-Bob", "102-Carol", "103-Alice", "104-Bob"]
        );
    }

    /// Batch preview with CSV should continue from source.current, not 0.
    #[test]
    fn batch_preview_csv_continues_from_current() {
        let mut source = VariableTextSource::default();
        source.csv_data = vec![
            vec!["Name".to_string()],
            vec!["Alice".to_string()],
            vec!["Bob".to_string()],
            vec!["Carol".to_string()],
        ];
        source.current = 10; // 10 % 3 = 1, start from Bob
        source.start = 10;
        source.end = 100;

        let template = "{Serial}-{CSV:Name}";

        // Simulate batch preview loop with current offset
        let mut results = Vec::new();
        for i in 0..3_usize {
            let mut iter_source = source.clone();
            iter_source.current = source.current + i as i64;
            // Pass copy index for {Serial:params}; CSV reads from current
            results.push(resolve_text(template, &iter_source, i));
        }
        // Should start from Bob (row 1), then Carol (row 2), then wrap to Alice (row 0)
        assert_eq!(results, vec!["10-Bob", "11-Carol", "12-Alice"]);
    }
}
