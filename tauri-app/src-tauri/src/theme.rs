use beambench_core::UiTheme;
use tauri::utils::config::{Color, WindowConfig};
use tauri::{AppHandle, Manager, Theme};

pub(crate) const DARK_BACKGROUND: Color = Color(0x0c, 0x0e, 0x12, 0xff);
pub(crate) const LIGHT_BACKGROUND: Color = Color(0xf3, 0xf5, 0xf7, 0xff);

/// Map the persisted preference to Tauri's native theme override.
/// `None` tells Tauri to follow the operating system.
pub(crate) fn native_theme(ui_theme: UiTheme) -> Option<Theme> {
    match ui_theme {
        UiTheme::System => None,
        UiTheme::Light => Some(Theme::Light),
        UiTheme::Dark => Some(Theme::Dark),
    }
}

/// Resolve the native window/webview background. System mode uses the native
/// theme when available and the dark startup color as a fail-safe otherwise.
pub(crate) fn background_color(ui_theme: UiTheme, system_theme: Option<Theme>) -> Color {
    match ui_theme {
        UiTheme::Light => LIGHT_BACKGROUND,
        UiTheme::Dark => DARK_BACKGROUND,
        UiTheme::System => match system_theme {
            Some(Theme::Light) => LIGHT_BACKGROUND,
            Some(Theme::Dark) | None => DARK_BACKGROUND,
            _ => DARK_BACKGROUND,
        },
    }
}

/// Configure a generated-context window before Tauri creates it. This avoids
/// an OS-default flash for explicit Light/Dark preferences.
pub(crate) fn configure_window(config: &mut WindowConfig, ui_theme: UiTheme) {
    config.theme = native_theme(ui_theme);
    config.background_color = Some(background_color(ui_theme, None));
}

fn set_window_background(window: &tauri::WebviewWindow, color: Color) {
    if let Err(error) = window.set_background_color(Some(color)) {
        tracing::warn!(
            window = window.label(),
            error = %error,
            "Failed to update native window background"
        );
    }
}

/// Apply a persisted appearance to native chrome and all existing windows.
/// Native appearance failures are deliberately non-fatal; CSS remains the
/// authoritative rendering layer.
pub(crate) fn apply_to_app(app: &AppHandle, ui_theme: UiTheme) {
    // Tauri exposes this app-wide override as an infallible operation.
    app.set_theme(native_theme(ui_theme));

    for window in app.webview_windows().into_values() {
        let system_theme = if ui_theme == UiTheme::System {
            match window.theme() {
                Ok(theme) => Some(theme),
                Err(error) => {
                    tracing::warn!(
                        window = window.label(),
                        error = %error,
                        "Failed to resolve native system theme"
                    );
                    None
                }
            }
        } else {
            None
        };
        set_window_background(&window, background_color(ui_theme, system_theme));
    }
}

/// Keep System-mode startup backgrounds aligned when the OS reports a theme
/// change. Tauri does not emit this event on Linux, where native chrome is
/// best-effort.
pub(crate) fn apply_system_background(app: &AppHandle, system_theme: Theme) {
    let color = background_color(UiTheme::System, Some(system_theme));
    for window in app.webview_windows().into_values() {
        set_window_background(&window, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_theme_mapping_preserves_system_mode() {
        assert_eq!(native_theme(UiTheme::System), None);
        assert_eq!(native_theme(UiTheme::Light), Some(Theme::Light));
        assert_eq!(native_theme(UiTheme::Dark), Some(Theme::Dark));
    }

    #[test]
    fn background_mapping_uses_dark_fail_safe_for_unresolved_system() {
        assert_eq!(background_color(UiTheme::Light, None), LIGHT_BACKGROUND);
        assert_eq!(background_color(UiTheme::Dark, None), DARK_BACKGROUND);
        assert_eq!(
            background_color(UiTheme::System, Some(Theme::Light)),
            LIGHT_BACKGROUND
        );
        assert_eq!(
            background_color(UiTheme::System, Some(Theme::Dark)),
            DARK_BACKGROUND
        );
        assert_eq!(background_color(UiTheme::System, None), DARK_BACKGROUND);
    }

    #[test]
    fn generated_window_config_gets_theme_and_background() {
        let mut config = WindowConfig::default();
        configure_window(&mut config, UiTheme::Light);
        assert_eq!(config.theme, Some(Theme::Light));
        assert_eq!(config.background_color, Some(LIGHT_BACKGROUND));

        configure_window(&mut config, UiTheme::System);
        assert_eq!(config.theme, None);
        assert_eq!(config.background_color, Some(DARK_BACKGROUND));
    }
}
