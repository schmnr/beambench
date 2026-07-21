import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import i18n from '../i18n';
import type { AppStatus, AppSettings, BuildInfo } from '../types/commands';
import type { PanelLayoutState } from '../panels';
import { useNotificationStore } from '../stores/notificationStore';

export interface AppSettingsUpdate {
  display_unit?: AppSettings['display_unit'];
  speed_time_unit?: AppSettings['speed_time_unit'];
  autosave_enabled?: boolean;
  autosave_interval_secs?: number;
  api_enabled?: boolean;
  api_port?: number;
  api_localhost_only?: boolean;
  ui_theme?: AppSettings['ui_theme'];
  dark_mode?: boolean;
  antialiasing?: boolean;
  filled_rendering?: boolean;
  reduce_motion?: boolean;
  show_palette_labels?: boolean;
  cursor_size?: 'small' | 'normal' | 'large';
  toolbar_icon_size?: 'small' | 'normal' | 'large';
  click_tolerance_px?: number;
  snap_threshold_px?: number;
  grid_spacing_mm?: number;
  nudge_step_mm?: number;
  nudge_step_fine_mm?: number;
  nudge_step_coarse_mm?: number;
  scroll_zoom?: boolean;
  debug_log_enabled?: boolean;
  last_radius_mm?: number;
  export_settings?: AppSettings['export_settings'];
  allow_importing_to_tool_layers?: boolean;
  include_tool_layers_in_job_bounds?: boolean;
  check_for_updates_on_startup?: boolean;
  update_snoozed_until?: string;
  skipped_update_version?: string;
  custom_hotkeys?: Record<string, string>;
  display_language?: string;
}

/** Convert top-level snake_case keys to camelCase for Tauri invoke. */
function snakeToCamel(obj: Record<string, unknown>): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(obj)) {
    if (value === undefined) continue;
    const camelKey = key.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase());
    result[camelKey] = value;
  }
  return result;
}

function serializePanelLayout(layout: PanelLayoutState): Record<string, unknown> {
  return {
    zones: Object.fromEntries(
      Object.entries(layout.zones).map(([zoneId, zone]) => [
        zoneId,
        {
          panel_ids: zone.panelIds,
          active_tab: zone.activeTab,
        },
      ]),
    ),
    hidden_panel_ids: layout.hiddenPanelIds,
    upper_split_ratio: layout.upperSplitRatio,
    right_panel_width: layout.rightPanelWidth,
    left_panel_width: layout.leftPanelWidth,
    bottom_panel_height: layout.bottomPanelHeight,
    side_panels_visible: layout.sidePanelsVisible,
    toolbar_visibility: layout.toolbarVisibility,
    floating_panels: layout.floatingPanels.map((fp) => ({
      panel_id: fp.panelId,
      x: fp.x,
      y: fp.y,
      width: fp.width,
      height: fp.height,
      z_index: fp.zIndex,
      origin_zone: fp.originZone ?? null,
      origin_index: fp.originIndex ?? null,
    })),
  };
}

export const appService = {
  async getStatus(): Promise<AppStatus> {
    return invoke<AppStatus>('get_app_status');
  },

  async getBuildInfo(): Promise<BuildInfo> {
    return invoke<BuildInfo>('get_build_info');
  },

  async openExternalUrl(url: string): Promise<void> {
    return invoke<void>('open_external_url', { url });
  },

  async getSettings(): Promise<AppSettings> {
    return invoke<AppSettings>('get_app_settings');
  },

  async openNewWindow(): Promise<string> {
    return invoke<string>('open_new_window');
  },

  /**
   * Set the unsaved-changes confirmation flag and close the window from the
   * backend. Used after the user picks Save or Don't Save.
   */
  async confirmWindowClose(): Promise<void> {
    return invoke<void>('confirm_window_close');
  },

  /**
   * Request a window close that runs the normal unsaved-changes check
   * (used by Cmd/Ctrl+Q). The backend owns the close request so older
   * webviews do not depend on the JS window-close wrapper.
   */
  async requestWindowClose(): Promise<void> {
    return invoke<void>('request_window_close');
  },

  async updateSettings(updates: AppSettingsUpdate): Promise<AppSettings> {
    return invoke<AppSettings>('update_app_settings', snakeToCamel(updates as Record<string, unknown>));
  },

  async pickPreferencesExportPath(): Promise<string> {
    const selected = await save({
      title: i18n.t('file_dialogs.export_preferences_title'),
      defaultPath: 'beam-bench.bbprefs',
      filters: [{
        name: i18n.t('file_dialogs.filter_preferences'),
        extensions: ['bbprefs'],
      }],
    });

    if (selected === null) {
      throw new Error('Export cancelled');
    }

    return selected;
  },

  async pickPreferencesImportPath(): Promise<string> {
    const selected = await open({
      title: i18n.t('file_dialogs.import_preferences_title'),
      multiple: false,
      filters: [{
        name: i18n.t('file_dialogs.filter_preferences'),
        extensions: ['bbprefs'],
      }],
    });

    if (selected === null || Array.isArray(selected)) {
      throw new Error('Import cancelled');
    }

    return selected;
  },

  async exportPreferences(path: string): Promise<string> {
    return invoke<string>('export_preferences', { path });
  },

  async importPreferences(path: string): Promise<AppSettings> {
    return invoke<AppSettings>('import_preferences', { path });
  },

  async resetPreferences(): Promise<AppSettings> {
    return invoke<AppSettings>('reset_preferences');
  },

  async openPreferencesFolder(): Promise<void> {
    return invoke<void>('open_preferences_folder');
  },

  async updateDisplaySettings(updates: AppSettingsUpdate): Promise<AppSettings> {
    return invoke<AppSettings>('update_display_settings', snakeToCamel(updates as Record<string, unknown>));
  },

  async getSystemFonts(): Promise<string[]> {
    return invoke<string[]>('get_system_fonts');
  },

  persistLayout(layout: PanelLayoutState): void {
    if (persistLayoutTimer !== null) {
      clearTimeout(persistLayoutTimer);
    }
    persistLayoutTimer = window.setTimeout(() => {
      persistLayoutTimer = null;
      void (async () => {
        try {
          await invoke('update_app_settings', {
            panelLayout: serializePanelLayout(layout),
          });
          layoutPersistFailureNotified = false;
        } catch (error) {
          console.error('[Beam Bench] Failed to save panel layout changes', error);
          if (!layoutPersistFailureNotified) {
            layoutPersistFailureNotified = true;
            useNotificationStore.getState().push(
              'Failed to save panel layout changes. They may be lost on restart.',
              'error',
            );
          }
        }
      })();
    }, 500);
  },
};

let persistLayoutTimer: number | null = null;
let layoutPersistFailureNotified = false;
