import { useEffect, useId, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '../../stores/appStore';
import { NumberStepper } from '../shared/NumberStepper';
import { mmToDisplay, displayToMm, roundDisplayLength, lengthStep, lengthUnitLabel, labelWithUnit } from '../../utils/lengthUnits';
import type { AppSettings, UiTheme } from '../../types/commands';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';

type SettingsDraft = {
  displayUnit: 'mm' | 'inches';
  speedTimeUnit: 'minutes' | 'seconds';
  autosaveEnabled: boolean;
  autosaveInterval: number;
  apiEnabled: boolean;
  apiPort: number;
  apiLocalhostOnly: boolean;
  uiTheme: UiTheme;
  darkMode: boolean;
  antialiasing: boolean;
  filledRendering: boolean;
  reduceMotion: boolean;
  clickTolerance: number;
  snapThreshold: number;
  gridSpacing: number;
  nudgeStep: number;
  nudgeStepFine: number;
  nudgeStepCoarse: number;
  scrollZoom: boolean;
  checkForUpdatesOnStartup: boolean;
  allowImportingToToolLayers: boolean;
  includeToolLayersInJobBounds: boolean;
};

const SETTINGS_DRAFT_KEYS = [
  'displayUnit',
  'speedTimeUnit',
  'autosaveEnabled',
  'autosaveInterval',
  'apiEnabled',
  'apiPort',
  'apiLocalhostOnly',
  'uiTheme',
  'darkMode',
  'antialiasing',
  'filledRendering',
  'reduceMotion',
  'clickTolerance',
  'snapThreshold',
  'gridSpacing',
  'nudgeStep',
  'nudgeStepFine',
  'nudgeStepCoarse',
  'scrollZoom',
  'checkForUpdatesOnStartup',
  'allowImportingToToolLayers',
  'includeToolLayersInJobBounds',
] as const;

type SettingsDraftKey = (typeof SETTINGS_DRAFT_KEYS)[number];
type TabId = 'general' | 'units_grid' | 'display' | 'file_import';

const TAB_IDS: TabId[] = ['general', 'units_grid', 'display', 'file_import'];

function createDraft(settings: AppSettings): SettingsDraft {
  return {
    displayUnit: settings.display_unit,
    speedTimeUnit: settings.speed_time_unit ?? 'minutes',
    autosaveEnabled: settings.autosave_enabled,
    autosaveInterval: settings.autosave_interval_secs,
    apiEnabled: settings.api_enabled,
    apiPort: settings.api_port,
    apiLocalhostOnly: settings.api_localhost_only,
    uiTheme: settings.ui_theme ?? 'dark',
    darkMode: settings.dark_mode ?? false,
    antialiasing: settings.antialiasing ?? false,
    filledRendering: settings.filled_rendering ?? false,
    reduceMotion: settings.reduce_motion ?? false,
    clickTolerance: settings.click_tolerance_px ?? 5,
    snapThreshold: settings.snap_threshold_px ?? 5,
    gridSpacing: settings.grid_spacing_mm,
    nudgeStep: settings.nudge_step_mm,
    nudgeStepFine: settings.nudge_step_fine_mm,
    nudgeStepCoarse: settings.nudge_step_coarse_mm,
    scrollZoom: settings.scroll_zoom ?? true,
    checkForUpdatesOnStartup: settings.check_for_updates_on_startup ?? true,
    allowImportingToToolLayers: settings.allow_importing_to_tool_layers ?? false,
    includeToolLayersInJobBounds: settings.include_tool_layers_in_job_bounds ?? true,
  };
}

const FALLBACK_DRAFT: SettingsDraft = {
  displayUnit: 'mm',
  speedTimeUnit: 'minutes',
  autosaveEnabled: true,
  autosaveInterval: 120,
  apiEnabled: true,
  apiPort: 5900,
  apiLocalhostOnly: false,
  uiTheme: 'dark',
  darkMode: false,
  antialiasing: false,
  filledRendering: false,
  reduceMotion: false,
  clickTolerance: 5,
  snapThreshold: 5,
  gridSpacing: 10,
  nudgeStep: 5,
  nudgeStepFine: 1,
  nudgeStepCoarse: 20,
  scrollZoom: true,
  checkForUpdatesOnStartup: true,
  allowImportingToToolLayers: false,
  includeToolLayersInJobBounds: true,
};

function SwitchRow(props: {
  label: string;
  description?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  testId?: string;
}) {
  const generatedId = useId();
  const controlId = props.testId ? `${props.testId}-control` : generatedId;
  const descriptionId = props.description ? `${controlId}-description` : undefined;
  return (
    <div className="flex items-center justify-between gap-4">
      <div>
        <label htmlFor={controlId} className="text-sm text-bb-text">{props.label}</label>
        {props.description && (
          <div id={descriptionId} className="mt-0.5 max-w-md text-xs leading-5 text-bb-text-muted">
            {props.description}
          </div>
        )}
      </div>
      <button
        id={controlId}
        data-testid={props.testId}
        className={`relative h-5 w-10 shrink-0 rounded-full transition-colors ${
          props.checked ? 'bg-bb-accent' : 'bg-bb-border'
        }`}
        onClick={() => props.onChange(!props.checked)}
        role="switch"
        aria-checked={props.checked}
        aria-describedby={descriptionId}
      >
        <span
          className={`absolute left-0.5 top-0.5 h-4 w-4 rounded-full bg-white transition-transform ${
            props.checked ? 'translate-x-5' : 'translate-x-0'
          }`}
        />
      </button>
    </div>
  );
}

function NumberRow(props: {
  label: string;
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  testId?: string;
}) {
  return (
    <div className="flex items-center justify-between gap-4">
      <label className="text-sm text-bb-text">{props.label}</label>
      <NumberStepper
        data-testid={props.testId}
        value={props.value}
        onChange={(e) => props.onChange(Number(e.target.value))}
        min={props.min}
        max={props.max}
        step={props.step}
        className="w-24 rounded border border-bb-control-border bg-bb-surface px-2 py-1 text-right text-sm text-bb-text"
      />
    </div>
  );
}

export function SettingsDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);

  // Display-unit and speed-unit picker values. Internal enum strings
  // ('mm', 'inches', 'minutes', 'seconds') are not user-facing; the
  // visible labels are derived from i18n keys.
  const displayUnits: ReadonlyArray<'mm' | 'inches'> = ['mm', 'inches'];
  const speedUnits: ReadonlyArray<[ 'minutes' | 'seconds', string ]> = [
    ['minutes', t('dialog.settings.speed_minutes_short')],
    ['seconds', t('dialog.settings.speed_seconds_short')],
  ];
  const [draft, setDraft] = useState<SettingsDraft | null>(() => (settings ? createDraft(settings) : null));
  const [activeTab, setActiveTab] = useState<TabId>('general');
  const [saveError, setSaveError] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [conflictFields, setConflictFields] = useState<SettingsDraftKey[]>([]);
  const dirtyFieldsRef = useRef<Set<SettingsDraftKey>>(new Set());
  const baseDraftRef = useRef<SettingsDraft | null>(draft);

  useEffect(() => {
    if (!settings) return;
    const latest = createDraft(settings);

    if (!draft) {
      setDraft(latest);
      dirtyFieldsRef.current.clear();
      baseDraftRef.current = latest;
      setConflictFields([]);
      return;
    }

    const previousBase = baseDraftRef.current ?? latest;
    const nextDraft = { ...draft };
    const nextConflicts: SettingsDraftKey[] = [];
    let draftChanged = false;

    for (const key of SETTINGS_DRAFT_KEYS) {
      if (!dirtyFieldsRef.current.has(key)) {
        if (nextDraft[key] !== latest[key]) {
          (nextDraft as Record<SettingsDraftKey, SettingsDraft[SettingsDraftKey]>)[key] = latest[key];
          draftChanged = true;
        }
        continue;
      }

      if (previousBase[key] !== latest[key] && draft[key] !== latest[key]) {
        nextConflicts.push(key);
      }
    }

    baseDraftRef.current = latest;
    setConflictFields(nextConflicts);
    if (draftChanged) setDraft(nextDraft);
  }, [draft, settings]);

  const ready = draft !== null;
  const content = useMemo(() => draft ?? FALLBACK_DRAFT, [draft]);

  const updateDraft = <K extends keyof SettingsDraft>(key: K, value: SettingsDraft[K]) => {
    setDraft((current) => (current ? { ...current, [key]: value } : current));
    dirtyFieldsRef.current.add(key);
    setConflictFields((current) => current.filter((field) => field !== key));
    setSaveError(null);
  };

  const resolveConflicts = (mode: 'latest' | 'mine') => {
    if (mode === 'latest' && draft && baseDraftRef.current) {
      const latest = baseDraftRef.current;
      setDraft((current) => {
        if (!current) return current;
        const next = { ...current };
        for (const key of conflictFields) {
          (next as Record<SettingsDraftKey, SettingsDraft[SettingsDraftKey]>)[key] = latest[key];
          dirtyFieldsRef.current.delete(key);
        }
        return next;
      });
    }
    setConflictFields([]);
  };

  const handleSave = async () => {
    if (!draft) return;

    try {
      setIsSaving(true);
      setSaveError(null);
      await updateSettings({
        display_unit: draft.displayUnit,
        speed_time_unit: draft.speedTimeUnit,
        autosave_enabled: draft.autosaveEnabled,
        autosave_interval_secs: draft.autosaveInterval,
        api_enabled: draft.apiEnabled,
        api_port: draft.apiPort,
        api_localhost_only: draft.apiLocalhostOnly,
        ui_theme: draft.uiTheme,
        dark_mode: draft.darkMode,
        antialiasing: draft.antialiasing,
        filled_rendering: draft.filledRendering,
        reduce_motion: draft.reduceMotion,
        click_tolerance_px: draft.clickTolerance,
        snap_threshold_px: draft.snapThreshold,
        grid_spacing_mm: draft.gridSpacing,
        nudge_step_mm: draft.nudgeStep,
        nudge_step_fine_mm: draft.nudgeStepFine,
        nudge_step_coarse_mm: draft.nudgeStepCoarse,
        scroll_zoom: draft.scrollZoom,
        check_for_updates_on_startup: draft.checkForUpdatesOnStartup,
        allow_importing_to_tool_layers: draft.allowImportingToToolLayers,
        include_tool_layers_in_job_bounds: draft.includeToolLayersInJobBounds,
      });
      onClose();
    } catch (e) {
      setSaveError(e instanceof Error ? e.message : String(e));
    } finally {
      setIsSaving(false);
    }
  };

  return createPortal(
    <>
      <MovableResizableDialogFrame
        title={t('dialog.settings.title')}
        titleId="dialog-title"
        testId="settings-dialog"
        initialWidth={760}
        initialHeight={620}
        minWidth={640}
        minHeight={520}
        onRequestClose={onClose}
        closeOnBackdropClick
        footer={
          <div className="space-y-3 px-5 py-4">
            {saveError && (
              <div role="alert" className="text-sm text-bb-error-fg">
                {saveError}
              </div>
            )}

            {conflictFields.length > 0 && (
              <div
                role="alert"
                data-testid="settings-sync-warning"
                className="space-y-2 rounded border border-bb-warning-border bg-bb-warning-bg p-2 text-sm text-bb-warning-fg"
              >
                <div>{t('dialog.settings.conflict_warning')}</div>
                <div className="flex justify-end gap-2">
                  <button
                    className="rounded border border-bb-warning-border px-2 py-1 text-xs hover:bg-bb-hover"
                    onClick={() => resolveConflicts('latest')}
                  >
                    {t('dialog.settings.use_latest')}
                  </button>
                  <button
                    className="rounded bg-bb-warning px-2 py-1 text-xs font-medium text-bb-on-warning hover:bg-bb-warning-hover"
                    onClick={() => resolveConflicts('mine')}
                  >
                    {t('dialog.settings.keep_mine')}
                  </button>
                </div>
              </div>
            )}

            <div className="flex justify-end gap-2">
              <button
                onClick={onClose}
                className="rounded border border-bb-border bg-bb-surface px-4 py-1.5 text-sm text-bb-text-muted hover:bg-bb-hover"
              >
                {t('common.cancel')}
              </button>
              <button
                onClick={() => void handleSave()}
                disabled={!ready || isSaving || conflictFields.length > 0}
                className="rounded bg-bb-accent px-4 py-1.5 text-sm text-bb-on-accent hover:bg-bb-accent-hover disabled:opacity-50"
              >
                {t('common.save')}
              </button>
            </div>
          </div>
        }
      >
        {!ready ? (
          <div className="px-5 py-8 text-sm text-bb-text-muted">{t('dialog.settings.loading')}</div>
        ) : (
          <div className="flex min-h-0 flex-1">
            <div className="w-44 shrink-0 border-r border-bb-border p-2">
              {TAB_IDS.map((id) => (
                <button
                  key={id}
                  className={`mb-1 w-full rounded px-3 py-2 text-left text-sm ${
                    activeTab === id
                      ? 'bg-bb-accent text-bb-on-accent'
                      : 'text-bb-text-muted hover:bg-bb-hover hover:text-bb-text'
                  }`}
                  onClick={() => setActiveTab(id)}
                >
                  {t(`dialog.settings.tab.${id}`)}
                </button>
              ))}
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto p-5">
              {activeTab === 'general' && (
                <div className="space-y-4">
                  <SwitchRow
                    label={t('dialog.settings.autosave')}
                    checked={content.autosaveEnabled}
                    onChange={(v) => updateDraft('autosaveEnabled', v)}
                  />
                  {content.autosaveEnabled && (
                    <NumberRow
                      label={t('dialog.settings.autosave_interval')}
                      value={content.autosaveInterval}
                      onChange={(v) => updateDraft('autosaveInterval', Math.max(30, Math.min(3600, v)))}
                      min={30}
                      max={3600}
                    />
                  )}
                  <SwitchRow
                    label={t('dialog.settings.local_api')}
                    checked={content.apiEnabled}
                    onChange={(v) => updateDraft('apiEnabled', v)}
                  />
                  {content.apiEnabled && (
                    <>
                      <NumberRow
                        label={t('dialog.settings.api_port')}
                        value={content.apiPort}
                        onChange={(v) => updateDraft('apiPort', Math.max(1, Math.min(65535, Math.round(v))))}
                        min={1}
                        max={65535}
                      />
                      <SwitchRow
                        label={t('dialog.settings.allow_network_devices')}
                        description={t('dialog.settings.allow_network_devices_help')}
                        checked={!content.apiLocalhostOnly}
                        onChange={(allowRemoteAccess) => updateDraft('apiLocalhostOnly', !allowRemoteAccess)}
                        testId="toggle-network-api-access"
                      />
                    </>
                  )}
                  <SwitchRow
                    label={t('dialog.settings.check_for_updates')}
                    checked={content.checkForUpdatesOnStartup}
                    onChange={(v) => updateDraft('checkForUpdatesOnStartup', v)}
                    testId="toggle-startup-update-checks"
                  />
                </div>
              )}

              {activeTab === 'units_grid' && (
                <div className="space-y-4">
                  <div className="flex items-center justify-between gap-4">
                    <label className="text-sm text-bb-text">{t('dialog.settings.display_unit')}</label>
                    <div className="flex gap-1">
                      {displayUnits.map((unit) => (
                        <button
                          key={unit}
                          className={`rounded px-3 py-1 text-xs ${
                            content.displayUnit === unit
                              ? 'bg-bb-accent text-bb-on-accent'
                              : 'border border-bb-border bg-bb-surface text-bb-text-muted hover:bg-bb-hover'
                          }`}
                          onClick={() => updateDraft('displayUnit', unit)}
                        >
                          {unit}
                        </button>
                      ))}
                    </div>
                  </div>
                  <div className="flex items-center justify-between gap-4">
                    <label className="text-sm text-bb-text">{t('dialog.settings.speed_time_unit')}</label>
                    <div className="flex gap-1">
                      {speedUnits.map(([unit, label]) => (
                        <button
                          key={unit}
                          className={`rounded px-3 py-1 text-xs ${
                            content.speedTimeUnit === unit
                              ? 'bg-bb-accent text-bb-on-accent'
                              : 'border border-bb-border bg-bb-surface text-bb-text-muted hover:bg-bb-hover'
                          }`}
                          onClick={() => updateDraft('speedTimeUnit', unit)}
                        >
                          {label}
                        </button>
                      ))}
                    </div>
                  </div>
                  <NumberRow
                    label={labelWithUnit(t('dialog.settings.grid_spacing'), lengthUnitLabel(content.displayUnit))}
                    value={roundDisplayLength(mmToDisplay(content.gridSpacing, content.displayUnit), content.displayUnit)}
                    onChange={(v) => updateDraft('gridSpacing', Math.max(0.1, displayToMm(v, content.displayUnit)))}
                    min={mmToDisplay(0.1, content.displayUnit)}
                    step={lengthStep(content.displayUnit, 0.1, 0.005)}
                    testId="input-grid-spacing"
                  />
                  <NumberRow
                    label={t('dialog.settings.snap_distance')}
                    value={content.snapThreshold}
                    onChange={(v) => updateDraft('snapThreshold', Math.max(1, Math.min(20, v)))}
                    min={1}
                    max={20}
                    testId="input-snap-threshold"
                  />
                  <NumberRow
                    label={t('dialog.settings.click_tolerance')}
                    value={content.clickTolerance}
                    onChange={(v) => updateDraft('clickTolerance', Math.max(1, Math.min(20, v)))}
                    min={1}
                    max={20}
                    testId="input-click-tolerance"
                  />
                  <NumberRow
                    label={labelWithUnit(t('dialog.settings.nudge_step'), lengthUnitLabel(content.displayUnit))}
                    value={roundDisplayLength(mmToDisplay(content.nudgeStep, content.displayUnit), content.displayUnit)}
                    onChange={(v) => updateDraft('nudgeStep', Math.max(0, displayToMm(v, content.displayUnit)))}
                    min={0}
                    step={lengthStep(content.displayUnit, 0.1, 0.005)}
                  />
                  <NumberRow
                    label={labelWithUnit(t('dialog.settings.nudge_fine'), lengthUnitLabel(content.displayUnit))}
                    value={roundDisplayLength(mmToDisplay(content.nudgeStepFine, content.displayUnit), content.displayUnit)}
                    onChange={(v) => updateDraft('nudgeStepFine', Math.max(0, displayToMm(v, content.displayUnit)))}
                    min={0}
                    step={lengthStep(content.displayUnit, 0.1, 0.005)}
                  />
                  <NumberRow
                    label={labelWithUnit(t('dialog.settings.nudge_coarse'), lengthUnitLabel(content.displayUnit))}
                    value={roundDisplayLength(mmToDisplay(content.nudgeStepCoarse, content.displayUnit), content.displayUnit)}
                    onChange={(v) => updateDraft('nudgeStepCoarse', Math.max(0, displayToMm(v, content.displayUnit)))}
                    min={0}
                    step={lengthStep(content.displayUnit, 0.1, 0.005)}
                  />
                </div>
              )}

              {activeTab === 'display' && (
                <div className="space-y-4">
                  <fieldset className="space-y-2">
                    <legend className="text-sm font-semibold text-bb-text">
                      {t('dialog.settings.appearance')}
                    </legend>
                    <div className="flex items-start justify-between gap-4">
                      <div>
                        <label htmlFor="settings-ui-theme" className="text-sm text-bb-text">
                          {t('dialog.settings.app_appearance')}
                        </label>
                        <div
                          id="settings-ui-theme-help"
                          className="mt-0.5 max-w-md text-xs leading-5 text-bb-text-muted"
                        >
                          {t('dialog.settings.app_appearance_help')}
                        </div>
                      </div>
                      <select
                        id="settings-ui-theme"
                        data-testid="select-ui-theme"
                        aria-describedby="settings-ui-theme-help"
                        value={content.uiTheme}
                        onChange={(event) => updateDraft('uiTheme', event.target.value as UiTheme)}
                        className="shrink-0 rounded border border-bb-control-border bg-bb-surface px-2 py-1 text-sm text-bb-text"
                      >
                        <option value="system">{t('dialog.settings.theme_system')}</option>
                        <option value="light">{t('dialog.settings.theme_light')}</option>
                        <option value="dark">{t('dialog.settings.theme_dark')}</option>
                      </select>
                    </div>
                  </fieldset>
                  <SwitchRow
                    label={t('dialog.settings.dark_background')}
                    checked={content.darkMode}
                    onChange={(v) => updateDraft('darkMode', v)}
                    testId="toggle-dark-mode"
                  />
                  <SwitchRow
                    label={t('dialog.settings.antialiasing')}
                    checked={content.antialiasing}
                    onChange={(v) => updateDraft('antialiasing', v)}
                    testId="toggle-antialiasing"
                  />
                  <SwitchRow
                    label={t('dialog.settings.filled_rendering')}
                    checked={content.filledRendering}
                    onChange={(v) => updateDraft('filledRendering', v)}
                    testId="toggle-filled-rendering"
                  />
                  <SwitchRow
                    label={t('dialog.settings.reduce_motion')}
                    checked={content.reduceMotion}
                    onChange={(v) => updateDraft('reduceMotion', v)}
                    testId="toggle-reduce-motion"
                  />
                  <div className="flex items-center justify-between gap-4">
                    <label className="text-sm text-bb-text">{t('dialog.settings.scroll_wheel')}</label>
                    <select
                      data-testid="select-scroll-behavior"
                      value={content.scrollZoom ? 'zoom' : 'pan'}
                      onChange={(e) => updateDraft('scrollZoom', e.target.value === 'zoom')}
                      className="rounded border border-bb-control-border bg-bb-surface px-2 py-1 text-sm text-bb-text"
                    >
                      <option value="zoom">{t('dialog.settings.scroll_zoom')}</option>
                      <option value="pan">{t('dialog.settings.scroll_pan')}</option>
                    </select>
                  </div>
                </div>
              )}

              {activeTab === 'file_import' && (
                <div className="space-y-4">
                  <SwitchRow
                    label={t('dialog.settings.allow_import_tool_layers')}
                    checked={content.allowImportingToToolLayers}
                    onChange={(v) => updateDraft('allowImportingToToolLayers', v)}
                    testId="toggle-allow-import-tool-layers"
                  />
                  <SwitchRow
                    label={t('dialog.settings.include_tool_layers_job_bounds')}
                    checked={content.includeToolLayersInJobBounds}
                    onChange={(v) => updateDraft('includeToolLayersInJobBounds', v)}
                    testId="toggle-tool-layers-job-bounds"
                  />
                </div>
              )}

            </div>
          </div>
        )}
      </MovableResizableDialogFrame>
    </>,
    document.body,
  );
}
