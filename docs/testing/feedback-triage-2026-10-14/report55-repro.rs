// Temporarily include this module from src-tauri/src/state.rs to run the probe.
// Run only this test with BEAMBENCH_API_TOKEN empty. No network listener starts.
#[cfg(test)]
mod feedback55_investigation {
    use super::*;
    use beambench_service::ops::app::{UpdateAppSettingsInput, update_app_settings};

    #[test]
    fn invalid_network_api_blocks_layout_save() {
        assert_eq!(std::env::var("BEAMBENCH_API_TOKEN").as_deref(), Ok(""));
        let settings = AppSettings {
            api_enabled: true,
            api_localhost_only: false,
            panel_layout: None,
            ..Default::default()
        };
        let ctx = Arc::new(ServiceContext::with_settings(settings.clone()));
        let runtime = ApiRuntime::default();
        let startup_error = runtime
            .sync_from_settings(ctx.clone(), &settings)
            .unwrap_err();
        assert!(startup_error.contains("Network API requires BEAMBENCH_API_TOKEN"));
        assert!(runtime.server.lock().unwrap().is_none());
        ctx.set_settings_applier({
            let ctx = ctx.clone();
            let runtime = runtime.clone();
            move |settings| runtime.sync_from_settings(ctx.clone(), settings)
        })
        .unwrap();
        let layout = serde_json::from_value(serde_json::json!({
            "layout_version": 3, "zones": {}, "hidden_panel_ids": [],
            "upper_split_ratio": 0.65, "right_panel_width": 410.0
        }))
        .unwrap();
        let error = update_app_settings(
            &ctx,
            UpdateAppSettingsInput {
                panel_layout: Some(layout),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains(&startup_error));
        assert!(ctx.settings.lock().unwrap().panel_layout.is_none());
        assert!(runtime.server.lock().unwrap().is_none());
        // Disabling the API in the same update recovers settings persistence.
        let recovered = update_app_settings(
            &ctx,
            UpdateAppSettingsInput {
                api_enabled: Some(false),
                panel_layout: Some(
                    serde_json::from_value(serde_json::json!({
                        "layout_version": 3, "zones": {}, "hidden_panel_ids": [],
                        "upper_split_ratio": 0.65, "right_panel_width": 410.0
                    }))
                    .unwrap(),
                ),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(recovered.panel_layout.unwrap().right_panel_width, 410.0);
        assert!(runtime.server.lock().unwrap().is_none());
        eprintln!("Confirmed: invalid network API blocks layout save; disabling API recovers it.");
    }
}
