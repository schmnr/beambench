//! This integration binary deliberately compiles the service without cfg(test).
#[test]
fn integration_process_never_uses_real_preferences() {
    use beambench_service::persist;
    let config = persist::config_dir().unwrap();
    let data = persist::data_dir().unwrap();
    assert_ne!(config, dirs::config_dir().unwrap().join("beam-bench"));
    assert_ne!(data, dirs::data_dir().unwrap().join("beam-bench"));
    assert_eq!(config, persist::config_dir().unwrap());
    assert_eq!(data, persist::data_dir().unwrap());
    let context = beambench_service::ServiceContext::new();
    let settings = context.settings.lock().unwrap().clone();
    persist::save_settings(&settings).unwrap();
    assert!(config.join("settings.json").exists());
}
