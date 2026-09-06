use super::*;

#[derive(Args)]
pub(super) struct SelectionArgs {
    /// Use only these object IDs (independent of GUI selection)
    #[arg(long, value_delimiter = ',')]
    selected_ids: Vec<String>,
    /// Compute placement from the explicit selection's origin
    #[arg(long, requires = "selected_ids")]
    use_selection_origin: bool,
}

impl SelectionArgs {
    pub(super) fn body(self) -> Value {
        serde_json::json!({
            "cut_selected_graphics": !self.selected_ids.is_empty(),
            "selected_object_ids": self.selected_ids,
            "use_selection_origin": self.use_selection_origin,
        })
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub(super) enum OverrideAction {
    Reset,
    Increase10,
    Decrease10,
    Increase1,
    Decrease1,
}
impl OverrideAction {
    pub(super) fn api_name(self) -> &'static str {
        match self {
            Self::Reset => "reset",
            Self::Increase10 => "increase_10",
            Self::Decrease10 => "decrease_10",
            Self::Increase1 => "increase_1",
            Self::Decrease1 => "decrease_1",
        }
    }
}

#[derive(Subcommand)]
pub(super) enum SettingsCmd {
    Show,
    Update { input: String },
}
impl SettingsCmd {
    pub(super) fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            Self::Show => request(Method::GET, "/api/v1/app/settings", None),
            Self::Update { input } => request_file(Method::POST, "/api/v1/app/settings", &input),
        }
    }
}

pub(super) fn no_confirmations() -> ConfirmFlags {
    ConfirmFlags {
        confirm_motion: false,
        confirm_laser_on: false,
        confirm_raw_gcode: false,
        confirm_air_assist: false,
    }
}

pub(super) fn request(
    method: Method,
    path: &str,
    body: Option<Value>,
) -> Result<(), Box<dyn std::error::Error>> {
    let result = local_api_json_request(method, path, body)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

pub(super) fn request_file(
    method: Method,
    path: &str,
    input: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    request(method, path, Some(load_json_file::<Value>(input)?))
}

pub(super) fn export_library(
    library: &str,
    output: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let result = local_api_json_request(Method::GET, &format!("/api/v1/{library}/export"), None)?;
    std::fs::write(output, serde_json::to_vec_pretty(&result)?)?;
    println!(
        "{}",
        serde_json::json!({"path": api_file_path(output)?, "exported": true})
    );
    Ok(())
}

fn native_request(
    group: &str,
    operation: &str,
    input: Option<String>,
    confirmations: ConfirmFlags,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut body = match input {
        Some(path) => load_json_file::<Value>(&path)?,
        None => serde_json::json!({}),
    };
    let object = body.as_object_mut().ok_or("Input must be a JSON object")?;
    if object.contains_key("command")
        || object.contains_key("confirm_motion")
        || object.contains_key("confirm_laser_on")
    {
        return Err("Choose the operation and hardware confirmations through CLI arguments".into());
    }
    // Relative file paths in input JSON are relative to the caller, just like CLI paths.
    for key in ["path", "file_path"] {
        if let Some(Value::String(path)) = object.get_mut(key) {
            *path = api_file_path(path)?;
        }
    }
    object.insert("command".into(), operation.into());
    object.insert("confirm_motion".into(), confirmations.confirm_motion.into());
    object.insert(
        "confirm_laser_on".into(),
        confirmations.confirm_laser_on.into(),
    );
    request(
        Method::POST,
        &format!("/api/v1/workflows/{group}"),
        Some(body),
    )
}
struct NativeCall {
    operation: String,
    input: Option<String>,
}

fn command_name(operation: &str) -> String {
    match operation {
        "get_art_libraries" => "list",
        "create_art_library" => "create",
        "load_art_library" => "load",
        "unload_art_library" => "unload",
        "save_art_library_as" => "save-as",
        "rename_art_library" => "rename",
        "delete_art_library" => "delete",
        "add_art_library_item" => "add-file",
        "add_selection_to_art_library" => "add-selection",
        "rename_art_library_item" => "rename-item",
        "remove_art_library_item" => "remove-item",
        "commit_art_library_thumbnail" => "thumbnail",
        "move_art_library_item" => "move-item",
        "insert_art_library_item_to_project" => "insert",
        "quality_test_preview" | "generate_batch_preview" => "preview",
        "quality_test_export_gcode" => "export",
        "quality_test_frame" => "frame",
        "quality_test_start" => "start",
        "quality_test_create_material_on_canvas" => "create-on-canvas",
        "export_material_test_recipes" => "export-recipes",
        "import_material_test_recipes" => "import-recipes",
        "parse_merge_fields" => "parse",
        "load_csv_file" => "load-csv",
        "resolve_variable_text" => "resolve",
        "generate_variable_text_batch" => "batch",
        other => return other.replace('_', "-"),
    }
    .into()
}

fn catalog(group: &str) -> Value {
    use beambench_service::ops::workflows;
    match group {
        "project" => workflows::project::schema(),
        "vector" => workflows::vector::schema(),
        "art_library" => workflows::art_library::schema(),
        "variable_text" => workflows::variable_text::schema(),
        "quality_test" => workflows::quality_test::schema(),
        _ => unreachable!(),
    }
}

fn augment_native(group: &str, mut command: clap::Command) -> clap::Command {
    command = command.subcommand(
        clap::Command::new("schema").about("List native commands and typed JSON input fields"),
    );
    for spec in catalog(group).as_array().expect("workflow catalog") {
        let operation = spec["operation"].as_str().unwrap();
        let name = command_name(operation);
        let alias = operation.replace('_', "-");
        let fields = spec["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|field| {
                format!(
                    "{}: {}{}",
                    field["name"].as_str().unwrap(),
                    field["type"].as_str().unwrap(),
                    if field["required"].as_bool().unwrap() {
                        " (required)"
                    } else {
                        ""
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        let mut subcommand = clap::Command::new(name.clone())
            .about(format!(
                "{operation}. JSON input: {}",
                if fields.is_empty() { "{}" } else { &fields }
            ))
            .arg(
                clap::Arg::new("input")
                    .long("input")
                    .value_name("JSON_FILE")
                    .help("JSON object containing the command's input fields"),
            );
        if name != alias {
            subcommand = subcommand.alias(alias);
        }
        command = command.subcommand(subcommand);
    }
    command
}

fn parse_native(group: &str, matches: &clap::ArgMatches) -> Result<NativeCall, clap::Error> {
    let Some((name, args)) = matches.subcommand() else {
        return Err(clap::Error::raw(
            clap::error::ErrorKind::MissingSubcommand,
            "Choose a workflow command or schema",
        ));
    };
    if name == "schema" {
        return Ok(NativeCall {
            operation: name.into(),
            input: None,
        });
    }
    let operation = catalog(group)
        .as_array()
        .unwrap()
        .iter()
        .find_map(|spec| {
            let operation = spec["operation"].as_str().unwrap();
            (command_name(operation) == name || operation.replace('_', "-") == name)
                .then(|| operation.to_owned())
        })
        .ok_or_else(|| {
            clap::Error::raw(
                clap::error::ErrorKind::InvalidSubcommand,
                "Unknown native workflow",
            )
        })?;
    Ok(NativeCall {
        operation,
        input: args.get_one::<String>("input").cloned(),
    })
}

macro_rules! native_group {
    ($name:ident, $group:literal) => {
        pub(super) struct $name(NativeCall);
        impl Subcommand for $name {
            fn augment_subcommands(command: clap::Command) -> clap::Command {
                augment_native($group, command)
            }
            fn augment_subcommands_for_update(command: clap::Command) -> clap::Command {
                augment_native($group, command)
            }
            fn has_subcommand(name: &str) -> bool {
                name == "schema"
                    || catalog($group).as_array().unwrap().iter().any(|s| {
                        let op = s["operation"].as_str().unwrap();
                        command_name(op) == name || op.replace('_', "-") == name
                    })
            }
        }
        impl clap::FromArgMatches for $name {
            fn from_arg_matches(matches: &clap::ArgMatches) -> Result<Self, clap::Error> {
                parse_native($group, matches).map(Self)
            }
            fn update_from_arg_matches(
                &mut self,
                matches: &clap::ArgMatches,
            ) -> Result<(), clap::Error> {
                *self = Self::from_arg_matches(matches)?;
                Ok(())
            }
        }
        impl $name {
            pub(super) fn run(
                self,
                confirmations: ConfirmFlags,
            ) -> Result<(), Box<dyn std::error::Error>> {
                if self.0.operation == "schema" {
                    println!("{}", serde_json::to_string_pretty(&catalog($group))?);
                    Ok(())
                } else {
                    native_request($group, &self.0.operation, self.0.input, confirmations)
                }
            }
        }
    };
}
native_group!(ProjectCmd, "project");
native_group!(VectorCmd, "vector");
native_group!(ArtLibraryCmd, "art_library");
native_group!(VariableTextCmd, "variable_text");
native_group!(QualityTestCmd, "quality_test");
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_native_operation_is_a_real_cli_command() {
        let schema = beambench_service::ops::workflows::schema();
        for (group, prefix) in [
            ("project", vec!["project", "edit"]),
            ("vector", vec!["vector", "edit"]),
            ("art_library", vec!["art-library"]),
            ("variable_text", vec!["variable-text"]),
            ("quality_test", vec!["quality-test"]),
        ] {
            for command in schema[group].as_array().unwrap() {
                let name = command["operation"].as_str().unwrap().replace('_', "-");
                let mut args = vec!["beambench-cli"];
                args.extend(prefix.clone());
                args.extend([&name, "--input", "request.json", "--json"]);
                Cli::try_parse_from(args)
                    .unwrap_or_else(|e| panic!("Missing CLI {group}.{name}: {e}"));
            }
        }
    }
}
