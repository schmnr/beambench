//! This integration binary deliberately compiles the service without cfg(test).
#[test]
fn integration_process_never_uses_real_preferences() {
    use beambench_service::persist;
    let config = persist::config_dir().unwrap();
    let data = persist::data_dir().unwrap();
    assert_ne!(config, dirs::config_dir().unwrap().join("beam-bench"));
    assert_ne!(data, dirs::data_dir().unwrap().join("beam-bench"));
    for (variable, actual) in [
        (persist::CONFIG_DIR_ENV, &config),
        (persist::DATA_DIR_ENV, &data),
    ] {
        if let Some(inherited) = std::env::var_os(variable) {
            assert_ne!(*actual, std::path::PathBuf::from(inherited));
        }
    }
    assert_eq!(config, persist::config_dir().unwrap());
    assert_eq!(data, persist::data_dir().unwrap());
    let context = beambench_service::ServiceContext::new();
    let settings = context.settings.lock().unwrap().clone();
    persist::save_settings(&settings).unwrap();
    assert!(config.join("settings.json").exists());
}

#[test]
fn inherited_custom_app_directories_are_preserved() {
    let sandbox = tempfile::tempdir().unwrap();
    let config = sandbox.path().join("custom-preferences");
    let data = sandbox.path().join("custom-data");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(config.join("settings.json"), "custom-settings-marker").unwrap();
    std::fs::write(data.join("library-marker"), "custom-data-marker").unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "integration_process_never_uses_real_preferences",
            "--nocapture",
        ])
        .env("BEAMBENCH_CONFIG_DIR", &config)
        .env("BEAMBENCH_DATA_DIR", &data)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(config.join("settings.json")).unwrap(),
        "custom-settings-marker"
    );
    assert_eq!(
        std::fs::read_to_string(data.join("library-marker")).unwrap(),
        "custom-data-marker"
    );
    assert_eq!(std::fs::read_dir(&config).unwrap().count(), 1);
    assert_eq!(std::fs::read_dir(&data).unwrap().count(), 1);
}
