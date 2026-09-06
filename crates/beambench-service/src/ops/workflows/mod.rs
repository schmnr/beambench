//! Native project workflows shared by desktop commands and programmatic clients.

// One definition owns deserialization, dispatch and discovery. The CLI builds
// its subcommands from this catalog, so adding an operation exposes it consistently.
macro_rules! define_commands {
    ($ctx:ident; $( $name:ident { $( $field:ident: $ty:ty ),* } => $call:expr; )*) => {
        #[allow(non_camel_case_types, clippy::large_enum_variant)]
        #[derive(serde::Deserialize)]
        #[serde(tag = "command", deny_unknown_fields)]
        pub enum Command { $( $name { $( $field: $ty ),* }, )* }

        impl Command {
            pub async fn execute(self, $ctx: &std::sync::Arc<crate::ServiceContext>) -> Result<serde_json::Value, super::WorkflowError> {
                match self {
                    $( Self::$name { $( $field ),* } => {
                        serde_json::to_value($call.map_err(super::WorkflowError::from)?).map_err(|e| super::WorkflowError::from(e.to_string()))
                    }, )*
                }
            }
        }

        pub fn schema() -> serde_json::Value {
            serde_json::Value::Array(vec![ $( serde_json::json!({
                "operation": stringify!($name),
                "fields": [ $( {
                    "name": stringify!($field), "type": stringify!($ty),
                    "required": !stringify!($ty).starts_with("Option"),
                }, )* ],
            }), )* ])
        }
    };
}
pub(crate) use define_commands;
pub mod art_library;
pub mod project;
pub mod quality_test;
pub mod variable_text;
pub mod vector;

#[derive(Debug)]
pub struct WorkflowError {
    pub message: String,
    pub details: serde_json::Value,
}

impl From<String> for WorkflowError {
    fn from(message: String) -> Self {
        Self {
            message,
            details: serde_json::Value::Null,
        }
    }
}

impl From<beambench_core::QualityTestError> for WorkflowError {
    fn from(error: beambench_core::QualityTestError) -> Self {
        Self {
            message: format!("{error:?}"),
            details: serde_json::to_value(error).unwrap_or_default(),
        }
    }
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "project": project::schema(),
        "vector": vector::schema(),
        "art_library": art_library::schema(),
        "variable_text": variable_text::schema(),
        "quality_test": quality_test::schema(),
        "examples": {
            "material_test_request": beambench_core::QualityTestRequest::Material(Default::default()),
            "focus_test_request": beambench_core::QualityTestRequest::Focus(Default::default()),
            "interval_test_request": beambench_core::QualityTestRequest::Interval(Default::default()),
            "variable_text_config": beambench_core::variable_text::VariableTextConfig {
                template: "SN-{Serial:1,1,3}".into(), mode: None, offset: None,
                source: beambench_core::variable_text::VariableTextSource::default(),
            },
            "nest_options": crate::ops::nesting::NestOptions::default(),
            "resize_slots_options": crate::ops::project::ResizeSlotsOptions {
                current_thickness_mm: 3.0, new_thickness_mm: 4.0, tolerance_mm: 0.05,
            },
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_desktop_workflow_is_in_the_public_catalog_or_has_a_dedicated_endpoint() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tauri-app/src-tauri/src/commands");
        let schema = schema();
        for group in [
            "project",
            "vector",
            "art_library",
            "variable_text",
            "quality_test",
        ] {
            let source = std::fs::read_to_string(root.join(format!("{group}.rs"))).unwrap();
            let catalog = schema[group].as_array().unwrap();
            for command in source.split("#[tauri::command]").skip(1) {
                let signature = command
                    .split("pub ")
                    .nth(1)
                    .unwrap()
                    .split('{')
                    .next()
                    .unwrap();
                let name = signature
                    .split("fn ")
                    .nth(1)
                    .unwrap()
                    .split('(')
                    .next()
                    .unwrap();
                if ["replace_project", "create_project", "close_project"].contains(&name) {
                    continue;
                }
                let entry = catalog
                    .iter()
                    .find(|entry| entry["operation"] == name)
                    .unwrap_or_else(|| {
                        panic!("Desktop command {group}.{name} needs CLI/API coverage")
                    });
                let args = signature
                    .split_once('(')
                    .unwrap()
                    .1
                    .split(") ->")
                    .next()
                    .unwrap();
                for argument in args.split(',') {
                    let Some((field, _)) = argument.trim().split_once(':') else {
                        continue;
                    };
                    if !field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                        || ["svc", "ctx", "_svc"].contains(&field)
                    {
                        continue;
                    }
                    assert!(
                        entry["fields"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|f| f["name"] == field),
                        "New desktop argument {group}.{name}.{field} needs public coverage"
                    );
                }
                for field in entry["fields"].as_array().unwrap() {
                    let field = field["name"].as_str().unwrap();
                    assert!(
                        signature.contains(&format!("{field}:")),
                        "Stale field {group}.{name}.{field}"
                    );
                }
            }
        }
    }
}
