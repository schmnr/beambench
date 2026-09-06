use super::*;

#[test]
fn offline_service_errors_keep_their_code_and_details() {
    let error = beambench_service::ServiceError::invalid_state("Plan failed")
        .with_details(serde_json::json!({"failed_entries":["mask"]}));
    let output = format_cli_error_json(&error);
    assert_eq!(output["error"]["code"], "invalid_state");
    assert_eq!(output["error"]["details"]["failed_entries"][0], "mask");
}

#[test]
fn signed_coordinates_work_without_equals_syntax() {
    for args in [
        vec![
            "beambench",
            "vector",
            "update-node",
            "object",
            "--subpath",
            "0",
            "--command",
            "1",
            "--x",
            "-12.5",
            "--y",
            "-7",
        ],
        vec![
            "beambench",
            "camera",
            "overlay",
            "nudge",
            "--dx",
            "-5",
            "--dy",
            "2",
        ],
        vec!["beambench", "camera", "overlay", "rotate", "--deg", "-90"],
        vec![
            "beambench",
            "profile",
            "update",
            "machine",
            "--laser-offset-x",
            "-2.5",
        ],
    ] {
        Cli::try_parse_from(&args).unwrap_or_else(|error| panic!("{args:?}: {error}"));
    }
}

#[test]
fn nonfinite_numbers_are_rejected_before_json_can_turn_them_into_null() {
    for number in ["NaN", "inf", "-inf"] {
        for args in [
            vec![
                "beambench",
                "profile",
                "update",
                "machine",
                "--bed-width-mm",
                number,
            ],
            vec!["beambench", "machine", "jog", number, "0"],
            vec!["beambench", "camera", "overlay", "opacity", number],
        ] {
            assert!(Cli::try_parse_from(&args).is_err(), "accepted {args:?}");
        }
        assert!(parse_scanning_offset_entry(&format!("1000:{number}")).is_err());
    }
}

#[test]
fn default_true_profile_option_can_be_disabled_at_creation() {
    let cli = Cli::try_parse_from([
        "beambench",
        "profile",
        "create",
        "--name",
        "test",
        "--use-g0-for-overscan",
        "false",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Commands::Profile {
            command: ProfileCmd::Create {
                use_g0_for_overscan: false,
                ..
            }
        }
    ));
    Cli::try_parse_from([
        "beambench",
        "profile",
        "create",
        "--name",
        "test",
        "--use-g0-for-overscan",
    ])
    .unwrap();
}

#[test]
fn kept_camera_renders_survive_cleanup_and_renders_have_unique_paths() {
    let dir = tempfile::tempdir().unwrap();
    let kept = camera_render_path(dir.path(), true).unwrap();
    let old = camera_render_path(dir.path(), false).unwrap();
    let recent = camera_render_path(dir.path(), false).unwrap();
    assert_ne!(old, recent);
    for path in [&kept, &old, &recent] {
        std::fs::write(path, b"png").unwrap();
    }
    for path in [&kept, &old] {
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(SystemTime::UNIX_EPOCH))
            .unwrap();
    }
    assert_eq!(cleanup_camera_render_artifacts(dir.path()).unwrap(), 1);
    assert!(kept.exists());
    assert!(recent.exists());
    assert!(!old.exists());
}
