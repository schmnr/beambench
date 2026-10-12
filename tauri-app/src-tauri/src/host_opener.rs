use std::path::Path;
use tauri_plugin_opener::OpenerExt;

pub fn open_url(app: &tauri::AppHandle, url: &str) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    if std::env::var_os("BEAMBENCH_APPIMAGE_FONT_POLICY").is_some() {
        return open_with_host_fonts(url.as_ref());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

pub fn open_path(app: &tauri::AppHandle, path: &Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    if std::env::var_os("BEAMBENCH_APPIMAGE_FONT_POLICY").is_some() {
        return open_with_host_fonts(path.as_os_str());
    }
    app.opener()
        .open_path(path.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(any(target_os = "linux", test))]
fn remove_app_font_policy(command: &mut std::process::Command) {
    command
        .env_remove("FONTCONFIG_FILE")
        .env_remove("BEAMBENCH_APPIMAGE_FONT_POLICY");
}

#[cfg(target_os = "linux")]
fn open_with_host_fonts(target: &std::ffi::OsStr) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;
    let mut last_error = None;
    for mut command in open::commands(target) {
        // The application-specific policy must not affect a browser or file
        // manager, or outlive the AppImage's temporary mount in their environment.
        remove_app_font_policy(&mut command);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);
        match command.spawn() {
            Ok(mut child) => {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                return Ok(());
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error
        .map(|e| e.to_string())
        .unwrap_or_else(|| "No desktop opener available".to_owned()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn external_launches_remove_only_the_app_font_policy() {
        let mut command = std::process::Command::new("desktop-opener");
        command
            .env("FONTCONFIG_FILE", "/appimage/fonts.conf")
            .env("BEAMBENCH_APPIMAGE_FONT_POLICY", "1")
            .env("DISPLAY", ":1");
        super::remove_app_font_policy(&mut command);
        let entries: Vec<_> = command.get_envs().collect();
        assert!(
            entries
                .iter()
                .any(|(key, value)| *key == "FONTCONFIG_FILE" && value.is_none())
        );
        assert!(
            entries
                .iter()
                .any(|(key, value)| *key == "BEAMBENCH_APPIMAGE_FONT_POLICY" && value.is_none())
        );
        assert!(entries.iter().any(|(key, value)| *key == "DISPLAY" && *value == Some(std::ffi::OsStr::new(":1"))));
    }
}
