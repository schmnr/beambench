import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { AppShell } from './components/layout/AppShell';
import { ToastContainer } from './components/shared/ToastContainer';
import { RecoveryDialog } from './components/settings/RecoveryDialog';
import { NotesDialog } from './components/dialogs/NotesDialog';
import { PreviewWindow } from './components/dialogs/PreviewWindow';
import { AboutDialog } from './components/settings/AboutDialog';
import { UpdateDialog } from './components/settings/UpdateDialog';
import { WelcomeDialog } from './components/welcome/WelcomeDialog';
import { SettingsDialog } from './components/settings/SettingsDialog';
import { HotkeyEditorDialog } from './components/settings/HotkeyEditorDialog';
import { TraceImageDialog } from './components/dialogs/TraceImageDialog';
import { AdjustImageDialog } from './components/dialogs/AdjustImageDialog';
import { OffsetDialog } from './components/dialogs/OffsetDialog';
import { BooleanAssistantDialog } from './components/dialogs/BooleanAssistantDialog';
import { BarcodeDialog } from './components/dialogs/BarcodeDialog';
import { CloseSelectedPathsWithToleranceDialog } from './components/dialogs/CloseSelectedPathsWithToleranceDialog';
import { DeleteDuplicatesDialog } from './components/dialogs/DeleteDuplicatesDialog';
import { UnsavedChangesDialog } from './components/dialogs/UnsavedChangesDialog';
import { NestDialog } from './components/dialogs/NestDialog';
import { MaterialTestDialog } from './components/dialogs/MaterialTestDialog';
import { FocusTestDialog } from './components/dialogs/FocusTestDialog';
import { IntervalTestDialog } from './components/dialogs/IntervalTestDialog';
import { FeedbackErrorBoundary } from './components/dialogs/FeedbackErrorBoundary';
import { FeedbackReportDialog } from './components/dialogs/FeedbackReportDialog';
import { useAppStore } from './stores/appStore';
import { useProjectStore } from './stores/projectStore';
import { usePreviewStore } from './stores/previewStore';
import { useUiStore } from './stores/uiStore';
import { useUndoStore } from './stores/undoStore';
import { useUnsavedGuardStore } from './stores/unsavedGuardStore';
import i18n from './i18n';
import { useMachineStore } from './stores/machineStore';
import { consumeSuppressedProfileEvent } from './stores/machineStore';
import { useCameraStore } from './stores/cameraStore';
import { useUpdateStore } from './stores/updateStore';
import { useWelcomeStore, shouldShowWelcome } from './stores/welcomeStore';
import { useMacroStore } from './stores/macroStore';
import { useNotificationStore } from './stores/notificationStore';
import { wrapBackendError } from './i18n/errors';
import { useEventListener } from './hooks/useEventListener';
import { useAutosave } from './hooks/useAutosave';
import { useMachinePolling } from './hooks/useMachinePolling';
import { persistenceService } from './services/persistenceService';
import { cameraService } from './services/cameraService';
import { captureBrowserCameraFrame } from './services/browserCameraCapture';
import { exportCanvasScreenshot } from './services/canvasScreenshotExportService';
import { getCanvasViewportSize } from './canvas/canvasViewportRegistry';
import { zoomToFitBounds } from './canvas/ViewportTransform';
import { isTransformLocked, notifyTransformLocked } from './utils/transformLocks';
import { matchesHotkey } from './utils/hotkeyMatch';
import { createSelectionContext, isBooleanCompatible } from './commands/selectionContext';
import {
  BOOLEAN_ASSISTANT_OPEN_EVENT,
  executeAppCommand,
  getAppCommandState,
  isNativeMenuOwnedShortcut,
  QUICK_HELP_DOCS_URL,
  setAppCommandDialogActions,
  type AppCommandDialogActions,
} from './commands/appCommands';
import { APP_COMMANDS } from './commands/appCommandIds';
import { defaultHotkeyIsOverriddenByEvent, findCommandForKeyboardEvent } from './commands/commandRegistry';
import { appService } from './services/appService';
import { feedbackService } from './services/feedbackService';
import type { PhysicalDockZone } from './panels';
import { normalizeToolbarVisibility, getPanelById } from './panels';
import { discardRecoveryBatch } from './utils/recovery';
import { lengthUnitLabel, mmToDisplay, roundDisplayLength } from './utils/lengthUnits';
import {
  clearClipboard,
  hasClipboardData,
  clipboardCut,
  clipboardCopy,
  clipboardPaste,
  clipboardPasteInPlace,
  clipboardDuplicate,
} from './utils/clipboard';
import { pasteClipboardArtworkFromEvent } from './utils/systemClipboard';
import type { RecoveryInfo } from './services/persistenceService';
import type { AppEvent } from './types/events';
import type { AppSettings } from './types/commands';
import type { CameraDeviceInfo } from './types/camera';
import { isNativeMenuActive } from './utils/platform';
import { FEEDBACK_REPORT_OPEN_EVENT, type FeedbackReportOpenDetail } from './feedbackEvents';
import type { JobProgress } from './types/machine';

const EMPTY_LAYERS: import('./types/project').Layer[] = [];
const EMPTY_RECENT_FILES: import('./types/commands').RecentFile[] = [];
const JOB_PROGRESS_EVENT_TYPES = new Set([
  'job.started',
  'job.progress',
  'job.paused',
  'job.resumed',
  'job.completed',
  'job.failed',
  'job.cancelled',
]);
const TERMINAL_JOB_STATES = new Set<JobProgress['state']>(['completed', 'failed', 'cancelled']);

interface CameraCaptureRequestedPayload {
  request_id?: string;
  camera_id?: string;
}

interface CameraOverlayRenderRequestedPayload {
  request_id?: string;
  options?: {
    output_path?: string;
    view?: 'fit' | 'current';
    format?: string;
  };
}

function isExportCancelledError(error: unknown): boolean {
  return String(error).toLowerCase().includes('cancelled');
}

function isJobProgressPayload(payload: unknown): payload is JobProgress {
  return !!payload
    && typeof payload === 'object'
    && typeof (payload as { state?: unknown }).state === 'string';
}

function isTerminalJobProgress(progress: JobProgress | null): progress is JobProgress {
  return !!progress && TERMINAL_JOB_STATES.has(progress.state);
}

function waitForCanvasPaint(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  });
}

async function runShortcutAsync(action: () => Promise<unknown>) {
  try {
    await action();
  } catch (error) {
    if (isExportCancelledError(error)) {
      return;
    }
    useNotificationStore.getState().push(wrapBackendError(String(error)), 'error');
  }
}

function isEditableEventTarget(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  if (!element) return false;
  return element.tagName === 'INPUT' || element.tagName === 'TEXTAREA' || element.isContentEditable;
}

function isJogLaserBrowserNavigationShortcut(event: KeyboardEvent): boolean {
  const ctrl = event.ctrlKey || event.metaKey;
  if (!ctrl) return false;
  const key = event.key;
  if (key !== '[' && key !== ']') return false;
  return (event.altKey && !event.shiftKey) || (event.shiftKey && !event.altKey);
}

function PreviewWindowBridge() {
  const previewWindowOpen = usePreviewStore((s) => s.previewWindowOpen);
  const previewData = usePreviewStore((s) => s.data);
  const previewState = usePreviewStore((s) => s.state);
  const manualRefreshRequired = usePreviewStore((s) => s.manualRefreshRequired);
  const previewGenerationDialogVisible = usePreviewStore((s) => s.previewGenerationDialogVisible);
  const refreshPreview = usePreviewStore((s) => s.refreshPreview);
  const cancelPreviewGeneration = usePreviewStore((s) => s.cancelPreviewGeneration);
  const closePreviewWindow = usePreviewStore((s) => s.closePreviewWindow);
  const layers = useProjectStore((s) => s.project?.layers) ?? EMPTY_LAYERS;
  const workspace = useProjectStore((s) => s.project?.workspace) ?? null;

  if (!previewWindowOpen) return null;

  return (
    <PreviewWindow
      data={previewData}
      previewState={previewState}
      manualRefreshRequired={manualRefreshRequired}
      previewGenerationDialogVisible={previewGenerationDialogVisible}
      layers={layers}
      workspace={workspace}
      onRefresh={() => { void refreshPreview(); }}
      onCancelGeneration={() => { void cancelPreviewGeneration(); }}
      onClose={closePreviewWindow}
    />
  );
}

interface NativeMenuCommandPayload {
  commandId: string;
  filePath?: string;
}

function NativeMenuBridge({ dialogActions }: { dialogActions: AppCommandDialogActions }) {
  const project = useProjectStore((s) => s.project);
  const selectedObjectIds = useProjectStore((s) => s.selectedObjectIds);
  const canUndo = useUndoStore((s) => s.canUndo);
  const canRedo = useUndoStore((s) => s.canRedo);
  const hasClipboard = useUiStore((s) => s.hasClipboard);
  const sidePanelsVisible = useUiStore((s) => s.sidePanelsVisible);
  const viewStyle = useUiStore((s) => s.viewStyle);
  const hiddenPanelIds = useUiStore((s) => s.panelLayout.hiddenPanelIds);
  const toolbarVisibility = useUiStore((s) => s.panelLayout.toolbarVisibility);
  const showNotesDialog = useUiStore((s) => s.showNotesDialog);
  const textEditObjectId = useUiStore((s) => s.textEditObjectId);
  const showPreview = usePreviewStore((s) => s.showPreview);
  const previewWindowOpen = usePreviewStore((s) => s.previewWindowOpen);
  const previewState = usePreviewStore((s) => s.state);
  const recentFiles = useAppStore((s) => s.settings?.recent_files);
  const customHotkeys = useAppStore((s) => s.settings?.custom_hotkeys);
  const displayLanguage = useAppStore((s) => s.settings?.display_language);

  // Tell the backend the webview actually booted. The backend's startup
  // watchdog shows a native "could not start" dialog if this never arrives
  // (old system WebKit that cannot run the bundled JS), and the Quit menu
  // falls back to a native exit until it does.
  useEffect(() => {
    invoke('mark_frontend_ready').catch(() => {
      // Non-fatal: only the watchdog depends on it.
    });
  }, []);

  // Apply persisted display_language to i18next when settings hydrate or
  // change. The pseudo-locale 'en-XA' is set via direct i18n.changeLanguage
  // calls elsewhere (dev only) and intentionally never persisted, so this
  // effect only ever sees the 23 production locales.
  useEffect(() => {
    if (!displayLanguage) return;
    if (i18n.language === displayLanguage) return;
    void i18n.changeLanguage(displayLanguage);
  }, [displayLanguage]);

  // When language changes, rebuild the native menu so section/submenu
  // titles relabel. The update_native_menu_state effect below keeps the
  // language check item state in sync, but it cannot update top-level
  // submenu titles (File, Edit, …) — those require a menu rebuild.
  //
  // Race-safety: use `i18n.getFixedT(displayLanguage)` rather than the
  // global `i18n.t`. The previous effect kicks off `i18n.changeLanguage`
  // asynchronously, and React fires both effects in the same paint —
  // `i18n.t` may still be bound to the old locale here. `getFixedT`
  // returns a locale-bound translator that resolves against the requested
  // bundle regardless of which language is active globally.
  useEffect(() => {
    if (!isNativeMenuActive()) return;
    if (!displayLanguage) return;
    void (async () => {
      const { buildNativeMenuLabels } = await import('./services/nativeMenuLabels');
      const fixedT = i18n.getFixedT(displayLanguage);
      const labels = buildNativeMenuLabels(fixedT);
      try {
        await invoke('rebuild_native_menu', {
          labels,
          state: getAppCommandState(),
        });
      } catch (error) {
        console.error('[Beam Bench] Failed to rebuild native menu for locale', error);
      }
    })();
  }, [displayLanguage]);
  const objectStateKey = useMemo(
    () => project?.objects
      .map((object) => `${object.id}:${object.locked ? '1' : '0'}:${object.data.type}`)
      .join('|') ?? '',
    [project?.objects],
  );
  const recentFilesKey = useMemo(
    () => (recentFiles ?? EMPTY_RECENT_FILES).map((file) => `${file.path}:${file.name}`).join('|'),
    [recentFiles],
  );
  const customHotkeysKey = useMemo(
    () => JSON.stringify(customHotkeys ?? {}),
    [customHotkeys],
  );
  const hiddenPanelIdsKey = useMemo(
    () => hiddenPanelIds.join('|'),
    [hiddenPanelIds],
  );
  const toolbarVisibilityKey = useMemo(
    () => JSON.stringify(toolbarVisibility),
    [toolbarVisibility],
  );

  useEffect(() => {
    if (!isNativeMenuActive()) return undefined;

    let unlisten: UnlistenFn | undefined;
    let cancelled = false;
    listen<NativeMenuCommandPayload>('native-menu-command', (event) => {
      void executeAppCommand(event.payload.commandId, dialogActions, {
        filePath: event.payload.filePath,
        source: 'native-menu',
      });
    }).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        unlisten = fn;
      }
    }).catch((error) => {
      console.error('[Beam Bench] Failed to subscribe to native menu events', error);
      useNotificationStore.getState().push(i18n.t('notifications.menu_subscribe_failed'), 'warning');
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [dialogActions]);

  useEffect(() => {
    if (!isNativeMenuActive()) return;
    void invoke('update_native_menu_state', { state: getAppCommandState() }).catch((error) => {
      console.error('[Beam Bench] Failed to update native menu state', error);
    });
  }, [
    canRedo,
    canUndo,
    customHotkeysKey,
    hasClipboard,
    hiddenPanelIdsKey,
    objectStateKey,
    previewState,
    previewWindowOpen,
    project,
    recentFilesKey,
    selectedObjectIds,
    showPreview,
    showNotesDialog,
    sidePanelsVisible,
    textEditObjectId,
    toolbarVisibilityKey,
    viewStyle,
    displayLanguage,
  ]);

  return null;
}

function AgentSelectionSyncBridge() {
  const projectId = useProjectStore((s) => s.project?.metadata.project_id ?? null);
  const selectedObjectIds = useProjectStore((s) => s.selectedObjectIds);
  const selectedLayerId = useProjectStore((s) => s.selectedLayerId);
  const selectionKey = useMemo(
    () => selectedObjectIds.join('|'),
    [selectedObjectIds],
  );

  useEffect(() => {
    const timer = window.setTimeout(() => {
      const frontendUpdatedAtMs = performance.timeOrigin + performance.now();
      void invoke('agent_sync_selection', {
        selectedObjectIds,
        selectedLayerId,
        projectId,
        frontendUpdatedAtMs,
      }).catch((error) => {
        console.error('[Beam Bench] Failed to sync agent selection state', error);
      });
    }, 75);

    return () => window.clearTimeout(timer);
  }, [projectId, selectedLayerId, selectedObjectIds, selectionKey]);

  return null;
}

function PreviewGenerationDialog() {
  const visible = usePreviewStore((s) => s.previewGenerationDialogVisible);
  const title = usePreviewStore((s) => s.previewGenerationDialogTitle);
  const cancelPreviewGeneration = usePreviewStore((s) => s.cancelPreviewGeneration);

  useEffect(() => {
    if (!visible) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        void cancelPreviewGeneration();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [visible, cancelPreviewGeneration]);

  if (!visible) return null;

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="preview-generation-title"
      className="fixed inset-0 z-[9500] flex items-center justify-center bg-black/20"
    >
      <div className="w-[360px] max-w-[90vw] rounded-xl border border-bb-border bg-bb-panel shadow-2xl">
        <div className="px-6 py-5 flex flex-col gap-4">
          <h2 id="preview-generation-title" className="text-sm font-semibold text-bb-text text-center">
            {title}
          </h2>
          <div
            className="h-2 overflow-hidden rounded-full bg-bb-border"
            aria-label={i18n.t('status.preview_in_progress')}
          >
            <div className="h-full w-1/3 rounded-full bg-bb-accent animate-pulse" />
          </div>
          <div className="flex justify-center">
            <button
              type="button"
              onClick={() => { void cancelPreviewGeneration(); }}
              className="px-4 py-1.5 rounded-md bg-bb-accent text-bb-on-accent text-sm font-medium hover:brightness-110 focus:outline-none focus:ring-2 focus:ring-bb-accent"
            >
              {i18n.t('common.cancel')}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

function NestingOverlay() {
  const visible = useUiStore((s) => s.nestingInProgress);
  if (!visible) return null;

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="nesting-overlay-title"
      className="fixed inset-0 z-[9500] flex items-center justify-center bg-black/20"
    >
      <div className="w-[360px] max-w-[90vw] rounded-xl border border-bb-border bg-bb-panel shadow-2xl">
        <div className="px-6 py-5 flex flex-col gap-4">
          <h2 id="nesting-overlay-title" className="text-sm font-semibold text-bb-text text-center">
            {i18n.t('status.nesting_in_progress')}
          </h2>
          <div className="h-2 overflow-hidden rounded-full bg-bb-border" aria-label={i18n.t('status.nesting_in_progress')}>
            <div className="h-full w-1/3 rounded-full bg-bb-accent animate-pulse" />
          </div>
        </div>
      </div>
    </div>
  );
}

function ResetPreferencesDialog({
  onCancel,
  onConfirm,
}: {
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="reset-preferences-title"
      className="fixed inset-0 z-[9500] flex items-center justify-center bg-black/20"
      onClick={onCancel}
    >
      <div
        className="w-[420px] max-w-[90vw] rounded-xl border border-bb-border bg-bb-panel shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-6 py-5 flex flex-col gap-4">
          <h2 id="reset-preferences-title" className="text-sm font-semibold text-bb-text">
            {i18n.t('menus.file.reset_preferences')}
          </h2>
          <p className="text-sm text-bb-text-muted">
            {i18n.t('menus.file.reset_preferences_confirm')}
          </p>
          <div className="flex justify-end gap-2">
            <button
              type="button"
              data-testid="reset-preferences-cancel"
              onClick={onCancel}
              className="rounded border border-bb-border px-3 py-1.5 text-xs font-medium text-bb-text hover:bg-bb-hover"
            >
              {i18n.t('common.cancel')}
            </button>
            <button
              type="button"
              data-testid="reset-preferences-confirm"
              onClick={onConfirm}
              className="rounded bg-bb-accent px-3 py-1.5 text-xs font-semibold text-bb-on-accent hover:bg-bb-accent-hover"
            >
              {i18n.t('common.ok')}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

function App() {
  const fetchStatus = useAppStore((s) => s.fetchStatus);
  const fetchSettings = useAppStore((s) => s.fetchSettings);
  const loadProject = useProjectStore((s) => s.loadProject);
  const restoreRecoveredProject = useProjectStore((s) => s.restoreRecoveredProject);
  const loadProfiles = useMachineStore((s) => s.loadProfiles);
  const hydrateSession = useMachineStore((s) => s.hydrateSession);
  const updateDialogOpen = useUpdateStore((s) => s.dialogOpen);
  const runStartupUpdateCheck = useUpdateStore((s) => s.runStartupCheck);
  const welcomeDialogOpen = useWelcomeStore((s) => s.dialogOpen);

  const [recoveries, setRecoveries] = useState<RecoveryInfo[]>([]);
  // dismissing the recovery dialog (X / backdrop) should hide it for
  // this session without discarding the underlying recovery files. A separate
  // dismiss flag lets recoveries stay in state so they can re-surface (e.g.
  // on re-mount or next startup) without being purged like Discard All.
  const [recoveryDismissed, setRecoveryDismissed] = useState(false);
  const showNotesDialog = useUiStore((s) => s.showNotesDialog);
  const setShowNotesDialog = useUiStore((s) => s.setShowNotesDialog);
  const [showAboutDialog, setShowAboutDialog] = useState(false);
  const [showSettingsDialog, setShowSettingsDialog] = useState(false);
  const [showHotkeyEditorDialog, setShowHotkeyEditorDialog] = useState(false);
  const [traceDialogObjectId, setTraceDialogObjectId] = useState<string | null>(null);
  const [adjustDialogObjectId, setAdjustDialogObjectId] = useState<string | null>(null);
  const [offsetDialogObjectIds, setOffsetDialogObjectIds] = useState<string[] | null>(null);
  const [booleanAssistantObjectIds, setBooleanAssistantObjectIds] = useState<string[] | null>(null);
  const [barcodeDialogLayerId, setBarcodeDialogLayerId] = useState<string | null>(null);
  const [nestDialogObjectIds, setNestDialogObjectIds] = useState<string[] | null>(null);
  const [closeToleranceObjectIds, setCloseToleranceObjectIds] = useState<string[] | null>(null);
  const [deleteDuplicatesCount, setDeleteDuplicatesCount] = useState<number | null>(null);
  const [feedbackDialog, setFeedbackDialog] = useState<FeedbackReportOpenDetail | null>(null);
  const startupCrashPromptChecked = useRef(false);
  const [showMaterialTestDialog, setShowMaterialTestDialog] = useState(false);
  const [showResetPreferencesDialog, setShowResetPreferencesDialog] = useState(false);
  const [showFocusTestDialog, setShowFocusTestDialog] = useState(false);
  const [showIntervalTestDialog, setShowIntervalTestDialog] = useState(false);
  const deleteDuplicatesResolveRef = useRef<((confirmed: boolean) => void) | null>(null);

  useAutosave();
  useMachinePolling();

  useEffect(() => {
    void cameraService.registerAgentBridge().catch((error) => {
      console.error('[Beam Bench] Failed to register camera agent bridge', error);
    });
    return () => {
      void cameraService.unregisterAgentBridge().catch((error) => {
        console.error('[Beam Bench] Failed to unregister camera agent bridge', error);
      });
    };
  }, []);

  const handleExportPreferences = useCallback(() => {
    void runShortcutAsync(async () => {
      const path = await appService.pickPreferencesExportPath();
      await appService.exportPreferences(path);
      useNotificationStore.getState().push(i18n.t('notifications.preferences_exported'), 'success');
    });
  }, []);

  const handleImportPreferences = useCallback(() => {
    void runShortcutAsync(async () => {
      const path = await appService.pickPreferencesImportPath();
      const settings = await appService.importPreferences(path);
      useAppStore.getState().applySettings(settings);
      useNotificationStore.getState().push(i18n.t('notifications.preferences_imported'), 'success');
    });
  }, []);

  const handleResetPreferences = useCallback(() => {
    setShowResetPreferencesDialog(true);
  }, []);

  const confirmResetPreferences = useCallback(() => {
    setShowResetPreferencesDialog(false);
    void runShortcutAsync(async () => {
      const settings = await appService.resetPreferences();
      useAppStore.getState().applySettings(settings);
      useNotificationStore.getState().push(i18n.t('notifications.preferences_reset'), 'success');
    });
  }, []);

  const handleOpenPreferencesFolder = useCallback(() => {
    void runShortcutAsync(() => appService.openPreferencesFolder());
  }, []);

  const resolveDeleteDuplicatesDialog = useCallback((confirmed: boolean) => {
    const resolve = deleteDuplicatesResolveRef.current;
    deleteDuplicatesResolveRef.current = null;
    setDeleteDuplicatesCount(null);
    resolve?.(confirmed);
  }, []);

  const confirmDeleteDuplicates = useCallback(async (objectIds: string[]): Promise<boolean> => {
    const duplicateCount = await useProjectStore.getState().countDuplicates(objectIds);
    deleteDuplicatesResolveRef.current?.(false);
    return new Promise<boolean>((resolve) => {
      deleteDuplicatesResolveRef.current = resolve;
      setDeleteDuplicatesCount(duplicateCount);
    });
  }, []);

  useEffect(() => () => {
    deleteDuplicatesResolveRef.current?.(false);
    deleteDuplicatesResolveRef.current = null;
  }, []);

  const nativeMenuDialogActions = useMemo<AppCommandDialogActions>(() => ({
    openAbout: () => setShowAboutDialog(true),
    openSettings: () => setShowSettingsDialog(true),
    openImportPreferences: handleImportPreferences,
    openExportPreferences: handleExportPreferences,
    openPreferencesFolder: handleOpenPreferencesFolder,
    resetPreferences: handleResetPreferences,
    openHotkeyEditor: () => setShowHotkeyEditorDialog(true),
    openTraceImage: (objectId: string) => setTraceDialogObjectId(objectId),
    openAdjustImage: (objectId: string) => setAdjustDialogObjectId(objectId),
    openOffset: (objectIds: string[]) => setOffsetDialogObjectIds([...objectIds]),
    openBooleanAssistant: (objectIds: string[]) => setBooleanAssistantObjectIds([...objectIds]),
    openBarcode: (layerId: string) => setBarcodeDialogLayerId(layerId),
    openNest: (objectIds: string[]) => setNestDialogObjectIds([...objectIds]),
    openCloseSelectedPathsWithTolerance: (objectIds: string[]) => setCloseToleranceObjectIds(objectIds),
    confirmDeleteDuplicates,
    openMaterialTest: () => setShowMaterialTestDialog(true),
    openFocusTest: () => setShowFocusTestDialog(true),
    openIntervalTest: () => setShowIntervalTestDialog(true),
    openReportBug: () => setFeedbackDialog({
      kind: 'bug',
      title: '',
      description: '',
      sourceContext: { source: 'help_menu', correlation_ts: new Date().toISOString() },
    }),
  }), [
    confirmDeleteDuplicates,
    handleExportPreferences,
    handleImportPreferences,
    handleOpenPreferencesFolder,
    handleResetPreferences,
  ]);

  useEffect(
    () => setAppCommandDialogActions(nativeMenuDialogActions),
    [nativeMenuDialogActions],
  );

  useEffect(() => {
    const openFeedback = (event: Event) => {
      const detail = (event as CustomEvent<FeedbackReportOpenDetail>).detail;
      setFeedbackDialog(detail);
    };
    window.addEventListener(FEEDBACK_REPORT_OPEN_EVENT, openFeedback);
    return () => window.removeEventListener(FEEDBACK_REPORT_OPEN_EVENT, openFeedback);
  }, []);

  useEffect(() => {
    if (startupCrashPromptChecked.current) return;
    startupCrashPromptChecked.current = true;

    const correlationTs = new Date().toISOString();
    const title = i18n.t('feedback.previous_crash_title');
    const description = i18n.t('feedback.previous_crash_description');
    const sourceContext = {
      source: 'startup_crash_check',
      feature: 'startup',
      correlation_ts: correlationTs,
    };

    void feedbackService.previewReport({
      kind: 'crash',
      title,
      description,
      notes: null,
      reply_to_email: null,
      include_project_file: false,
      source_context: sourceContext,
    }).then((bundle) => {
      if ((bundle?.recent_panics?.length ?? 0) === 0) return;
      setFeedbackDialog((current) => current ?? {
        kind: 'crash',
        title,
        description,
        sourceContext,
      });
    }).catch(() => {
      // Startup crash prompting is best-effort; manual Help > Report a Bug remains available.
    });
  }, []);

  useEffect(() => {
    const openBooleanAssistant = (event: Event) => {
      const objectIds = (event as CustomEvent<string[]>).detail;
      if (Array.isArray(objectIds) && objectIds.length >= 2) {
        setBooleanAssistantObjectIds([...objectIds]);
      }
    };

    window.addEventListener(BOOLEAN_ASSISTANT_OPEN_EVENT, openBooleanAssistant);
    return () => window.removeEventListener(BOOLEAN_ASSISTANT_OPEN_EVENT, openBooleanAssistant);
  }, []);

  // Clear clipboard when project changes (new/open/close/restore)
  useEffect(() => {
    let prevProjectId: string | undefined;
    const unsubscribe = useProjectStore.subscribe((state) => {
      const curId = state.project?.metadata.project_id;
      if (curId !== prevProjectId) {
        if (prevProjectId !== undefined) {
          clearClipboard();
        }
        prevProjectId = curId;
      }
    });
    return unsubscribe;
  }, []);

  useEffect(() => {
    fetchStatus();
    fetchSettings();
    // Restore panel layout from persisted settings
    appService.getSettings().then((settings) => {
      const ui = useUiStore.getState();
      ui.setGridSpacing(settings.grid_spacing_mm);
      ui.setNudgeSteps({
        normal: settings.nudge_step_mm,
        fine: settings.nudge_step_fine_mm,
        coarse: settings.nudge_step_coarse_mm,
      });
      if (settings.panel_layout) {
        const pl = settings.panel_layout;
        // Drop panels that no longer exist (e.g. retired color_palette)
        // from persisted layouts.
        const knownPanel = (id: string) => getPanelById(id) !== undefined;
        const floatingPanels = (pl.floating_panels ?? []).filter((fp) => knownPanel(fp.panel_id)).map((fp) => ({
          panelId: fp.panel_id,
          x: fp.x,
          y: fp.y,
          width: fp.width,
          height: fp.height,
          zIndex: fp.z_index,
          originZone: fp.origin_zone ?? undefined,
          originIndex: fp.origin_index ?? undefined,
        }));
        const maxZ = floatingPanels.reduce((max, fp) => Math.max(max, fp.zIndex), 0);
        const restoredZones = Object.fromEntries(
          Object.entries(pl.zones).map(([k, v]) => {
            const panelIds = v.panel_ids.filter(knownPanel);
            const activeTab = panelIds.includes(v.active_tab) ? v.active_tab : (panelIds[0] ?? '');
            return [k, { panelIds, activeTab }];
          })
        ) as Record<PhysicalDockZone, { panelIds: string[]; activeTab: string }>;
        // Ensure new zones exist for backward compat with old persisted layouts
        if (!restoredZones['left']) restoredZones['left'] = { panelIds: [], activeTab: '' };
        if (!restoredZones['bottom']) restoredZones['bottom'] = { panelIds: [], activeTab: '' };
        const sidePanelsVisible = pl.side_panels_visible ?? true;
        useUiStore.getState().setPanelLayout({
          zones: restoredZones,
          hiddenPanelIds: pl.hidden_panel_ids.filter(knownPanel),
          floatingPanels,
          upperSplitRatio: pl.upper_split_ratio,
          rightPanelWidth: pl.right_panel_width,
          leftPanelWidth: pl.left_panel_width ?? 280,
          bottomPanelHeight: pl.bottom_panel_height ?? 80,
          sidePanelsVisible,
          toolbarVisibility: normalizeToolbarVisibility(pl.toolbar_visibility),
        });
        useUiStore.setState({ nextFloatingZIndex: maxZ + 1, sidePanelsVisible });
      }
      // First-launch / periodic welcome promo, decided after settings resolve.
      if (shouldShowWelcome(settings)) {
        useWelcomeStore.getState().openDialog();
      }
    }).catch(() => {
      useNotificationStore
        .getState()
        .push(i18n.t('notifications.layout_restore_failed'), 'warning');
    });
    // Load persisted machine settings from backend. Optimization is
    // now part of the project and arrives via `loadProject()` below,
    // so the former `loadOptimizationSettings()` boot call is gone.
    void loadProfiles();
    void hydrateSession();
    loadProject().then(() => {
      if (!useProjectStore.getState().project) {
        useProjectStore.getState().createProject('Untitled Project');
      }
    });

    // Check for recovery files on startup
    persistenceService.checkRecovery().then((files) => {
      if (files.length > 0) {
        setRecoveries(files);
      }
    }).catch(() => {
      useNotificationStore
        .getState()
        .push(i18n.t('notifications.recovery_check_failed'), 'warning');
    });
  }, [fetchStatus, fetchSettings, hydrateSession, loadProject, loadProfiles]);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void runStartupUpdateCheck();
    }, 12_000);
    return () => window.clearTimeout(timer);
  }, [runStartupUpdateCheck]);

  const resolveCameraDevice = useCallback(async (cameraId: string): Promise<CameraDeviceInfo> => {
    let devices = useCameraStore.getState().devices;
    let device = devices.find((candidate) => candidate.camera_id === cameraId);
    if (!device) {
      await useCameraStore.getState().refreshDevices();
      devices = useCameraStore.getState().devices;
      device = devices.find((candidate) => candidate.camera_id === cameraId);
    }
    if (!device) {
      throw new Error(`Camera '${cameraId}' not found`);
    }
    return device;
  }, []);

  const handleCameraCaptureBridgeRequest = useCallback(async (
    payload: CameraCaptureRequestedPayload,
  ) => {
    const requestId = payload.request_id;
    const cameraId = payload.camera_id;
    if (!requestId || !cameraId) return;
    useCameraStore.setState({ loading: true, error: null });
    try {
      const device = await resolveCameraDevice(cameraId);
      const capture = await captureBrowserCameraFrame(device);
      const frame = await cameraService.saveFrame(
        cameraId,
        capture.imageData,
        capture.widthPx,
        capture.heightPx,
        capture.mediaType,
      );
      await cameraService.completeCaptureRequest(requestId, frame, null);
      await useCameraStore.getState().refreshOverlayState();
    } catch (error) {
      await cameraService.completeCaptureRequest(
        requestId,
        null,
        error instanceof Error ? error.message : String(error),
      );
      useCameraStore.setState({
        error: error instanceof Error ? error.message : String(error),
      });
    } finally {
      useCameraStore.setState({ loading: false });
    }
  }, [resolveCameraDevice]);

  const handleCameraOverlayRenderBridgeRequest = useCallback(async (
    payload: CameraOverlayRenderRequestedPayload,
  ) => {
    const requestId = payload.request_id;
    const outputPath = payload.options?.output_path;
    if (!requestId || !outputPath) return;
    const previousViewport = {
      offset: useUiStore.getState().viewportOffset,
      zoom: useUiStore.getState().zoom,
    };
    let restoreViewport = false;
    try {
      if (payload.options?.view !== 'current') {
        const project = useProjectStore.getState().project;
        const viewport = getCanvasViewportSize();
        if (project && viewport) {
          const fit = zoomToFitBounds(
            {
              min: { x: 0, y: 0 },
              max: {
                x: project.workspace.bed_width_mm,
                y: project.workspace.bed_height_mm,
              },
            },
            viewport.width,
            viewport.height,
          );
          useUiStore.getState().zoomToFit(fit.offset, fit.zoom);
          restoreViewport = true;
          await waitForCanvasPaint();
        }
      }
      const path = await exportCanvasScreenshot(outputPath, 'png');
      await cameraService.completeOverlayRenderRequest(requestId, path, null);
    } catch (error) {
      await cameraService.completeOverlayRenderRequest(
        requestId,
        null,
        error instanceof Error ? error.message : String(error),
      );
    } finally {
      if (restoreViewport) {
        useUiStore.getState().zoomToFit(previousViewport.offset, previousViewport.zoom);
      }
    }
  }, []);

  // Listen for backend events to verify the event bridge works.
  const handleAppEvent = useCallback((event: AppEvent) => {
    const eventPayload = event.payload as {
      profile_id?: string | null;
      profile?: { id?: string | null };
    } | undefined;
    const profileId = eventPayload?.profile_id ?? eventPayload?.profile?.id ?? null;
    const activeProfileId = useMachineStore.getState().activeProfileId;
    if (
      event.type === 'app.settings.updated'
      || event.type === 'app.preferences.imported'
      || event.type === 'app.preferences.reset'
    ) {
      const settings = (event.payload as { settings?: AppSettings } | undefined)?.settings;
      if (settings) {
        useAppStore.getState().applySettings(settings);
      }
    }
    if (
      event.type === 'profile.saved'
      || event.type === 'profile.deleted'
      || event.type === 'profile.activated'
      || event.type === 'profile.deactivated'
    ) {
      if (consumeSuppressedProfileEvent(event.type, profileId)) {
        return;
      }
      void useMachineStore.getState().loadProfiles();
      if (
        event.type === 'profile.activated'
        || event.type === 'profile.deactivated'
        || profileId === null
        || profileId === activeProfileId
      ) {
        usePreviewStore.getState().invalidate();
      }
    }
    if (JOB_PROGRESS_EVENT_TYPES.has(event.type) && isJobProgressPayload(event.payload)) {
      useMachineStore.setState({ jobProgress: event.payload, error: null });
    }
    if (event.type === 'job.tick_failed') {
      const p = event.payload as { message?: unknown } | undefined;
      const message = typeof p?.message === 'string' && p.message.trim().length > 0
        ? p.message
        : 'Job streaming tick failed';
      useMachineStore.setState({ jobProgress: null, error: message, loading: false });
      useNotificationStore.getState().push(wrapBackendError(message), 'error');
    }
    if (event.type === 'machine.disconnected') {
      const payload = event.payload as { stop_warning?: unknown } | undefined;
      if (typeof payload?.stop_warning === 'string' && payload.stop_warning.trim().length > 0) {
        useNotificationStore.getState().push(wrapBackendError(payload.stop_warning), 'warning');
      }
      const currentJobProgress = useMachineStore.getState().jobProgress;
      useMachineStore.setState({
        sessionState: 'disconnected',
        machineStatus: null,
        connectedPort: null,
        connectionPreview: false,
        machineCoordinatesValid: false,
        capabilities: null,
        loading: false,
        jobProgress: isTerminalJobProgress(currentJobProgress) ? currentJobProgress : null,
      });
    }
    if (event.type === 'machine.homed') {
      useMachineStore.setState({ machineCoordinatesValid: true });
    }
    // Surface import feedback as toast
    if (event.type === 'project.import.auto_routed') {
      const p = event.payload as { reason?: string; to_layer_name?: string } | undefined;
      const layerName = p?.to_layer_name ?? i18n.t('notifications.another_layer');
      const message = p?.reason === 'raster_to_image_layer'
        ? i18n.t('notifications.routed_image', { layer: layerName })
        : i18n.t('notifications.routed_vector', { layer: layerName });
      useNotificationStore.getState().push(message, 'info');
    }
    if (event.type === 'project.import.completed') {
      const p = event.payload as {
        file_count?: number;
        object_ids?: unknown[];
        warnings?: unknown[];
      } | undefined;
      const fileCount = typeof p?.file_count === 'number' ? p.file_count : 0;
      const objectCount = Array.isArray(p?.object_ids) ? p.object_ids.length : 0;
      useNotificationStore.getState().push(i18n.t('notifications.import_complete', { fileCount, objectCount }), 'success');
      const warnings = Array.isArray(p?.warnings)
        ? p.warnings.filter((warning): warning is string => typeof warning === 'string')
        : [];
      if (warnings.length > 0) {
        useNotificationStore.getState().push(warnings.join(' '), 'warning');
      }
    }
    if (event.type === 'project.import.oversized') {
      const p = event.payload as {
        width_mm?: number;
        height_mm?: number;
        bed_width_mm?: number;
        bed_height_mm?: number;
      } | undefined;
      const displayUnit = useAppStore.getState().settings?.display_unit ?? 'mm';
      const fmt = (mm: number) => roundDisplayLength(mmToDisplay(mm, displayUnit), displayUnit);
      useNotificationStore.getState().push(
        i18n.t('notifications.import_oversized', {
          width: fmt(p?.width_mm ?? 0),
          height: fmt(p?.height_mm ?? 0),
          bedWidth: fmt(p?.bed_width_mm ?? 0),
          bedHeight: fmt(p?.bed_height_mm ?? 0),
          unit: lengthUnitLabel(displayUnit),
        }),
        'warning',
      );
    }
    if (event.type === 'project.design.transaction_applied') {
      void useProjectStore.getState().loadProject({ invalidatePreview: true });
    }
    if (event.type === 'camera.overlay.runtime.updated') {
      void useCameraStore.getState().refreshOverlayState();
    }
    if (event.type === 'camera.capture.requested') {
      void handleCameraCaptureBridgeRequest(event.payload as CameraCaptureRequestedPayload);
    }
    if (event.type === 'camera.overlay.render.requested') {
      void handleCameraOverlayRenderBridgeRequest(
        event.payload as CameraOverlayRenderRequestedPayload,
      );
    }
    if (event.type === 'app.close_requested') {
      // The backend held the window open because the project has unsaved
      // changes. Park the actual close behind the Save / Don't Save prompt.
      useUnsavedGuardStore.getState().request({
        // confirmWindowClose sets the confirmation flag AND closes the
        // window from the backend (browser window.close() is unreliable
        // in the webview).
        execute: () => appService.confirmWindowClose(),
      });
    }
    if (import.meta.env.DEV) {
      console.log('[Beam Bench] Event received:', event.type, event.payload);
    }
  }, [handleCameraCaptureBridgeRequest, handleCameraOverlayRenderBridgeRequest]);
  const handleAppEventSubscriptionError = useCallback((error: unknown) => {
    console.error('[Beam Bench] Failed to subscribe to app-event bridge', error);
    useNotificationStore
      .getState()
      .push(i18n.t('notifications.backend_subscribe_failed'), 'warning');
  }, []);
  useEventListener('app-event', handleAppEvent, handleAppEventSubscriptionError);

  useEffect(() => {
    const handlePaste = (event: ClipboardEvent) => {
      if (isEditableEventTarget(event.target)) return;
      if (useUiStore.getState().textEditObjectId) return;
      void pasteClipboardArtworkFromEvent(event).catch((error) => {
        useNotificationStore.getState().push(wrapBackendError(String(error)), 'error');
      });
    };
    window.addEventListener('paste', handlePaste);
    return () => window.removeEventListener('paste', handlePaste);
  }, []);

  // Global keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const ctrl = e.ctrlKey || e.metaKey;
      const shift = e.shiftKey;
      const alt = e.altKey;
      const target = e.target as HTMLElement;
      const isInput = target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable;

      if (e.key === 'Escape' && !isInput && useUiStore.getState().activeTool === 'node') {
        return;
      }

      if (!isInput && isNativeMenuOwnedShortcut(e)) {
        if (isJogLaserBrowserNavigationShortcut(e)) {
          e.preventDefault();
        }
        return;
      }

      // When text editing overlay is active, let the textarea handle all input
      if (useUiStore.getState().textEditObjectId && !e.ctrlKey && !e.metaKey) return;

      // dispatch user-defined macro hotkeys before built-in shortcuts.
      // Gated on !isInput so typing doesn't accidentally fire macros.
      if (!isInput) {
        const macros = useMacroStore.getState().macros;
        for (const macro of macros) {
          if (matchesHotkey(macro.hotkey, e)) {
            e.preventDefault();
            void useMacroStore.getState().runMacro(macro.id);
            return;
          }
        }
        const customHotkeys = useAppStore.getState().settings?.custom_hotkeys ?? {};
        const commandId = findCommandForKeyboardEvent(e, customHotkeys);
        if (commandId) {
          if (commandId === APP_COMMANDS.EDIT_PASTE && !hasClipboardData()) {
            return;
          }
          e.preventDefault();
          void executeAppCommand(commandId, nativeMenuDialogActions, { source: 'shortcut' });
          return;
        }
        if (defaultHotkeyIsOverriddenByEvent(e, customHotkeys)) {
          e.preventDefault();
          return;
        }
      }

      const ps = useProjectStore.getState();
      const ui = useUiStore.getState();
      const selectedObjects = ps.project?.objects.filter((o) => ps.selectedObjectIds.includes(o.id)) ?? [];
      const singleSelectedObject = selectedObjects.length === 1 ? selectedObjects[0] : null;
      const anyLocked = selectedObjects.some((o) => o.locked);
      const selectionContext = createSelectionContext(
        ps.selectedObjectIds,
        ps.project?.objects ?? [],
        ui.hasClipboard,
        [],
        ps.project?.assets ?? [],
      );
      const blockTransform = (kind: 'position' | 'scale' | 'rotation') => {
        if (isTransformLocked(ps.project?.transform_locks, kind)) {
          notifyTransformLocked(kind);
          return true;
        }
        return false;
      };

      // --- Undo/Redo ---
      if (ctrl && e.key === 'z' && !shift && !isInput) {
        e.preventDefault();
        useUndoStore.getState().undo();
      } else if (ctrl && e.key === 'z' && shift && !isInput) {
        e.preventDefault();
        useUndoStore.getState().redo();
      }
      // --- File shortcuts ---
      else if (ctrl && e.key === 'n' && !alt && !isInput) {
        e.preventDefault();
        ps.createProject('Untitled Project');
      } else if (ctrl && e.key === 's' && !shift && !isInput) {
        e.preventDefault();
        void executeAppCommand(APP_COMMANDS.FILE_SAVE);
      } else if (ctrl && e.key === 's' && shift && !isInput) {
        e.preventDefault();
        void executeAppCommand(APP_COMMANDS.FILE_SAVE_AS);
      } else if (ctrl && e.key === 'o' && !isInput) {
        e.preventDefault();
        void executeAppCommand(APP_COMMANDS.FILE_OPEN);
      } else if (ctrl && e.key === 'i' && !shift && !isInput) {
        e.preventDefault();
        void executeAppCommand(APP_COMMANDS.FILE_IMPORT);
      } else if (ctrl && alt && e.key === 'n' && !isInput) {
        e.preventDefault();
        useUiStore.getState().toggleNotesDialog();
      } else if (ctrl && shift && !alt && (e.key === 'P' || e.key === 'p') && !isInput) {
        e.preventDefault();
        void runShortcutAsync(() => executeAppCommand(APP_COMMANDS.FILE_PRINT_COLORS, {}, { source: 'shortcut' }));
      } else if (ctrl && !shift && !alt && (e.key === 'P' || e.key === 'p') && !isInput) {
        e.preventDefault();
        void runShortcutAsync(() => executeAppCommand(APP_COMMANDS.FILE_PRINT_BLACK, {}, { source: 'shortcut' }));
      } else if (ctrl && e.key === 'q' && !isInput) {
        e.preventDefault();
        void appService.requestWindowClose();
      }
      // --- Edit shortcuts ---
      else if (ctrl && e.key === 'a' && !isInput) {
        e.preventDefault();
        ps.selectAllObjects();
      } else if (ctrl && shift && e.key === 'I' && !isInput) {
        e.preventDefault();
        // Invert selection
        const project = ps.project;
        if (project) {
          const allIds = project.objects.map((o) => o.id);
          const selected = new Set(ps.selectedObjectIds);
          const inverted = allIds.filter((id) => !selected.has(id));
          ps.selectObjects(inverted);
        }
      } else if (ctrl && e.key === 'x' && !isInput) {
        e.preventDefault();
        if (!anyLocked) void clipboardCut([...ps.selectedObjectIds]);
      } else if (ctrl && e.key === 'c' && !isInput) {
        e.preventDefault();
        clipboardCopy([...ps.selectedObjectIds]);
      } else if (ctrl && e.key === 'v' && !isInput) {
        if (hasClipboardData()) {
          e.preventDefault();
          void clipboardPaste();
        }
      } else if (ctrl && e.key === 'd' && !isInput) {
        e.preventDefault();
        if (!anyLocked) void clipboardDuplicate([...ps.selectedObjectIds]);
      } else if ((e.key === 'Delete' || e.key === 'Backspace') && !isInput) {
        if (ps.selectedObjectIds.length > 0 && !anyLocked) {
          e.preventDefault();
          void ps.removeObjects([...ps.selectedObjectIds]);
        }
      }
      // --- Group/Ungroup ---
      else if (ctrl && e.key === 'g' && !shift && !isInput) {
        e.preventDefault();
        if (ps.selectedObjectIds.length >= 2 && !anyLocked) {
          void ps.groupObjects(ps.selectedObjectIds);
        }
      } else if (ctrl && e.key === 'u' && !isInput) {
        e.preventDefault();
        if (ps.selectedObjectIds.length === 1 && !anyLocked && singleSelectedObject?.data.type === 'group') {
          void ps.ungroupObjects(ps.selectedObjectIds[0]);
        }
      }
      // --- Snap toggle (moved from Ctrl+G to Ctrl+Shift+G) ---
      else if (ctrl && e.key === 'G' && shift && !isInput) {
        e.preventDefault();
        ui.toggleSnap();
      }
      // --- Transforms ---
      else if (ctrl && shift && e.key === 'H' && !isInput) {
        e.preventDefault();
        if (!blockTransform('scale') && ps.selectedObjectIds.length > 0) {
          void ps.flipObjects(ps.selectedObjectIds, 'horizontal');
        }
      } else if (ctrl && shift && e.key === 'V' && !isInput) {
        e.preventDefault();
        if (!blockTransform('scale') && ps.selectedObjectIds.length > 0) {
          void ps.flipObjects(ps.selectedObjectIds, 'vertical');
        }
      } else if (e.key === '.' && !ctrl && !isInput) {
        if (!blockTransform('rotation') && ps.selectedObjectIds.length > 0) {
          void ps.rotateObjects(ps.selectedObjectIds, 90);
        }
      } else if (e.key === ',' && !ctrl && !isInput) {
        if (!blockTransform('rotation') && ps.selectedObjectIds.length > 0) {
          void ps.rotateObjects(ps.selectedObjectIds, -90);
        }
      }
      // --- Arrangement ---
      else if (e.key === 'PageUp' && !ctrl && !isInput) {
        e.preventDefault();
        if (ps.selectedObjectIds.length === 1) void ps.pushDrawOrder(ps.selectedObjectIds[0], 'forward');
      } else if (e.key === 'PageDown' && !ctrl && !isInput) {
        e.preventDefault();
        if (ps.selectedObjectIds.length === 1) void ps.pushDrawOrder(ps.selectedObjectIds[0], 'backward');
      } else if (e.key === 'PageUp' && ctrl && !isInput) {
        e.preventDefault();
        if (ps.selectedObjectIds.length === 1) void ps.pushDrawOrder(ps.selectedObjectIds[0], 'front');
      } else if (e.key === 'PageDown' && ctrl && !isInput) {
        e.preventDefault();
        if (ps.selectedObjectIds.length === 1) void ps.pushDrawOrder(ps.selectedObjectIds[0], 'back');
      }
      // --- Arrow key nudge ---
      else if (
        (e.key === 'ArrowUp' || e.key === 'ArrowDown' || e.key === 'ArrowLeft' || e.key === 'ArrowRight') &&
        !isInput
      ) {
        // Never nudge while a text edit session is active. The global text-edit
        // guard above intentionally lets Ctrl/Meta combos through (so shortcuts
        // like save still work mid-edit), but Ctrl+Arrow must not move the
        // object being edited.
        if (ui.textEditObjectId) return;
        e.preventDefault();
        const step = ctrl && shift
          ? ui.nudgeStepFineMm / 10
          : ctrl
          ? ui.nudgeStepFineMm
          : shift
            ? ui.nudgeStepCoarseMm
            : ui.nudgeStepMm;
        const dx = e.key === 'ArrowLeft' ? -step : e.key === 'ArrowRight' ? step : 0;
        const dy = e.key === 'ArrowUp' ? -step : e.key === 'ArrowDown' ? step : 0;
        // Locked objects must not move — mirrors SelectTool's
        // selectionIncludesLockedObjects drag guard.
        if (!blockTransform('position') && ps.selectedObjectIds.length > 0 && !anyLocked) {
          void ps.nudgeObjects([...ps.selectedObjectIds], dx, dy);
        }
      }
      // --- Selection cycling (by creation order) ---
      else if (e.key === 'Tab' && !ctrl && !isInput) {
        e.preventDefault();
        const project = ps.project;
        if (project && project.objects.length > 0) {
          const sorted = [...project.objects].sort((a, b) => {
            const ta = a.created_at ?? '';
            const tb = b.created_at ?? '';
            return ta < tb ? -1 : ta > tb ? 1 : 0;
          });
          const curIdx = ps.selectedObjectIds.length > 0
            ? sorted.findIndex((o) => o.id === ps.selectedObjectIds[0])
            : -1;
          const nextIdx = shift
            ? (curIdx <= 0 ? sorted.length - 1 : curIdx - 1)
            : ((curIdx + 1) % sorted.length);
          ps.selectObjects([sorted[nextIdx].id]);
        }
      }
      // --- View shortcuts ---
      else if (e.key === 'g' && !ctrl && !isInput) {
        ui.toggleGrid();
      } else if (e.key === 'p' && !ctrl && !alt && !isInput) {
        usePreviewStore.getState().togglePreview();
      } else if (ctrl && e.key === '0' && !isInput) {
        e.preventDefault();
        // Zoom to fit handled via zoomToFit in StatusBar — set to 100%
        ui.setZoom(100);
      } else if (e.key === 'F12' && !isInput) {
        e.preventDefault();
        ui.toggleSidePanels();
      } else if (e.key === 'Escape' && !isInput) {
        ui.setActiveTool('select');
      }
      // --- Tool shortcuts ---
      else if (!ctrl && !alt && !isInput && e.key === 'v') {
        ui.setActiveTool('select');
      } else if (!ctrl && !isInput && e.key === 'r') {
        ui.setActiveTool('rect');
      } else if (!ctrl && !isInput && e.key === 'e') {
        ui.setActiveTool('ellipse');
      } else if (!ctrl && !isInput && e.key === 't') {
        ui.setActiveTool('text');
      } else if (!ctrl && !isInput && e.key === 'a') {
        ui.setActiveTool('node');
      } else if (ctrl && e.key === 'l' && !isInput) {
        e.preventDefault();
        ui.setActiveTool('line');
      } else if (ctrl && e.key === '`' && !isInput) {
        e.preventDefault();
        ui.setActiveTool('node');
      } else if (ctrl && e.key === 'k' && !isInput) {
        e.preventDefault();
        ui.setActiveTool('trim');
      } else if (alt && e.key === 'l' && !isInput) {
        ui.setActiveTool('laser_position');
      } else if (alt && e.key === 'm' && !isInput) {
        ui.setActiveTool('measure');
      }
      // --- Zoom shortcuts ---
      else if (ctrl && (e.key === '=' || e.key === '+') && !isInput) {
        e.preventDefault();
        ui.zoomIn();
      } else if (ctrl && e.key === '-' && !isInput) {
        e.preventDefault();
        ui.zoomOut();
      }
      // --- Vector/path shortcuts ---
      else if (ctrl && shift && e.key === 'C' && !isInput) {
        e.preventDefault();
        if (selectionContext.canConvertToPath) void ps.convertToPath(ps.selectedObjectIds[0]);
      } else if (ctrl && shift && (e.key === 'B' || e.key === 'b') && !isInput) {
        e.preventDefault();
        if (selectionContext.canConvertToBitmap && singleSelectedObject) {
          void ps.convertToBitmap(singleSelectedObject.id, 300);
        }
      } else if (alt && (e.key === 'T' || e.key === 't') && !ctrl && !isInput) {
        e.preventDefault();
        // Open Trace Image dialog instead of tracing directly
        if (singleSelectedObject?.data.type === 'raster_image') {
          setTraceDialogObjectId(singleSelectedObject.id);
        }
      } else if (alt && (e.key === 'I' || e.key === 'i') && !ctrl && !isInput) {
        e.preventDefault();
        // Open Adjust Image dialog instead of refreshing
        if (singleSelectedObject?.data.type === 'raster_image') {
          setAdjustDialogObjectId(singleSelectedObject.id);
        }
      } else if (alt && e.key === 'j' && !ctrl && !isInput) {
        e.preventDefault();
        // Same tolerance as the Edit menu and command palette entry points (0.05 mm)
        if (selectionContext.canClosePath) void ps.autoJoinShapes(ps.selectedObjectIds, 0.05);
      } else if (alt && shift && (e.key === 'O' || e.key === 'o') && !ctrl && !isInput) {
        e.preventDefault();
        if (selectionContext.canClosePath) void ps.optimizeShapes(ps.selectedObjectIds);
      } else if (alt && e.key === 'd' && !ctrl && !isInput) {
        e.preventDefault();
        void executeAppCommand(APP_COMMANDS.EDIT_DELETE_DUPLICATES, nativeMenuDialogActions, { source: 'shortcut' });
      } else if (alt && e.key === 'b' && !ctrl && !isInput) {
        e.preventDefault();
        if (selectionContext.canBreakApart) void ps.breakApart(ps.selectedObjectIds[0]);
      } else if (ctrl && e.key === 'Tab' && !isInput) {
        e.preventDefault();
        ui.setActiveTool('tabs');
      }
      // --- Boolean shortcuts ---
      else if (alt && e.key === '+' && !ctrl && !isInput) {
        e.preventDefault();
        const objs = ps.project?.objects ?? [];
        const sel = ps.selectedObjectIds;
        const selObjs = objs.filter((o) => sel.includes(o.id));
        if (sel.length === 2 && selObjs.every((o) => isBooleanCompatible(o, objs))) {
          void ps.booleanUnion(sel[0], sel[1]);
        }
      } else if (alt && e.key === '-' && !ctrl && !isInput) {
        e.preventDefault();
        const objs = ps.project?.objects ?? [];
        const sel = ps.selectedObjectIds;
        const selObjs = objs.filter((o) => sel.includes(o.id));
        if (sel.length === 2 && selObjs.every((o) => isBooleanCompatible(o, objs))) {
          void ps.booleanSubtract(sel[0], sel[1]);
        }
      } else if (alt && e.key === '*' && !ctrl && !isInput) {
        e.preventDefault();
        const objs = ps.project?.objects ?? [];
        const sel = ps.selectedObjectIds;
        const selObjs = objs.filter((o) => sel.includes(o.id));
        if (sel.length === 2 && selObjs.every((o) => isBooleanCompatible(o, objs))) {
          void ps.booleanIntersection(sel[0], sel[1]);
        }
      } else if (ctrl && e.key === 'w' && !isInput) {
        e.preventDefault();
        const objs = ps.project?.objects ?? [];
        const sel = ps.selectedObjectIds;
        const selObjs = objs.filter((o) => sel.includes(o.id));
        if (sel.length >= 2 && selObjs.every((o) => isBooleanCompatible(o, objs))) {
          void ps.booleanWeld(sel);
        }
      }
      // --- Paste in Place ---
      else if (alt && e.key === 'v' && !ctrl && !isInput) {
        e.preventDefault();
        void clipboardPasteInPlace();
      }
      // --- Export shortcuts ---
      else if (alt && e.key === 'x' && !ctrl && !isInput) {
        e.preventDefault();
        void runShortcutAsync(() => executeAppCommand(APP_COMMANDS.FILE_EXPORT));
      } else if (alt && shift && (e.key === 'L' || e.key === 'l') && !ctrl && !isInput) {
        e.preventDefault();
        void runShortcutAsync(() => executeAppCommand(APP_COMMANDS.FILE_SAVE_MACHINE_FILES, {}, { source: 'shortcut' }));
      }
      // --- Quick Help ---
      else if (e.key === 'F1' && !isInput) {
        e.preventDefault();
        void runShortcutAsync(() => appService.openExternalUrl(QUICK_HELP_DOCS_URL));
      }
      // --- Quick Offset (Alt+O) ---
      else if (alt && (e.key === 'o' || e.key === 'O') && !ctrl && !shift && !isInput) {
        e.preventDefault();
        if (ps.selectedObjectIds.length > 0) {
          void ps.offsetShapes(ps.selectedObjectIds, 1, 'outward');
        }
      }
      // --- Toggle Wireframe/Filled (Alt+Shift+W) ---
      else if (alt && shift && (e.key === 'w' || e.key === 'W') && !ctrl && !isInput) {
        e.preventDefault();
        ui.toggleFilledRendering();
      }
      // --- Boolean Assistant (Ctrl+B / Cmd+B) ---
      else if (ctrl && !shift && (e.key === 'b' || e.key === 'B') && !isInput) {
        e.preventDefault();
        const objs = ps.project?.objects ?? [];
        const sel = ps.selectedObjectIds;
        const selObjs = objs.filter((o) => sel.includes(o.id));
        if (sel.length === 2 && selObjs.every((o) => isBooleanCompatible(o, objs))) {
          void executeAppCommand(APP_COMMANDS.TOOLS_BOOLEAN_ASSISTANT, nativeMenuDialogActions, { source: 'shortcut' });
        } else {
          useNotificationStore.getState().push(i18n.t('notifications.boolean_needs_two'), 'warning');
        }
      }
      // --- Frame Selection ---
      else if (ctrl && shift && e.key === 'A' && !isInput) {
        e.preventDefault();
        // Zoom to fit all objects — reset to 100% and center
        ui.setZoom(100);
        ui.setViewportOffset({ x: 0, y: 0 });
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [nativeMenuDialogActions]);

  const handleRestore = async (path: string) => {
    try {
      const project = await persistenceService.restoreRecovery(path);
      restoreRecoveredProject(project);
      useNotificationStore.getState().push(i18n.t('notifications.project_restored'), 'success');
      setRecoveries((prev) => prev.filter((r) => r.path !== path));
    } catch (e) {
      useNotificationStore.getState().push(wrapBackendError(String(e)), 'error');
    }
  };

  const handleDiscard = async (path: string) => {
    try {
      await persistenceService.discardRecovery(path);
      setRecoveries((prev) => prev.filter((r) => r.path !== path));
    } catch (e) {
      useNotificationStore.getState().push(wrapBackendError(String(e)), 'error');
    }
  };

  const handleDiscardAll = async () => {
    const { discardedPaths, failedProjectNames } = await discardRecoveryBatch(
      recoveries,
      (path) => persistenceService.discardRecovery(path),
    );

    setRecoveries((prev) => prev.filter((r) => !discardedPaths.has(r.path)));

    if (failedProjectNames.length > 0) {
      const detail = failedProjectNames.slice(0, 3).join(', ');
      const suffix = failedProjectNames.length > 3 ? ', ...' : '';
      useNotificationStore
        .getState()
        .push(i18n.t('notifications.discard_recovery_failed', { count: failedProjectNames.length, detail: detail + suffix }), 'error');
    }
  };

  return (
    <>
      <NativeMenuBridge dialogActions={nativeMenuDialogActions} />
      <AgentSelectionSyncBridge />
      <FeedbackErrorBoundary>
        <AppShell />
      </FeedbackErrorBoundary>
      <ToastContainer />
      {updateDialogOpen && <UpdateDialog />}
      {welcomeDialogOpen && <WelcomeDialog />}
      {recoveries.length > 0 && !recoveryDismissed && (
        <RecoveryDialog
          recoveries={recoveries}
          onRestore={handleRestore}
          onDiscard={handleDiscard}
          onDiscardAll={handleDiscardAll}
          onClose={() => setRecoveryDismissed(true)}
        />
      )}
      {showNotesDialog && (
        <NotesDialog onClose={() => setShowNotesDialog(false)} />
      )}
      {showAboutDialog && (
        <AboutDialog onClose={() => setShowAboutDialog(false)} />
      )}
      {showSettingsDialog && (
        <SettingsDialog onClose={() => setShowSettingsDialog(false)} />
      )}
      {showHotkeyEditorDialog && (
        <HotkeyEditorDialog onClose={() => setShowHotkeyEditorDialog(false)} />
      )}
      {traceDialogObjectId && (
        <TraceImageDialog objectId={traceDialogObjectId} onClose={() => setTraceDialogObjectId(null)} />
      )}
      {adjustDialogObjectId && (
        <AdjustImageDialog objectId={adjustDialogObjectId} onClose={() => setAdjustDialogObjectId(null)} />
      )}
      {offsetDialogObjectIds && (
        <OffsetDialog objectIds={offsetDialogObjectIds} onClose={() => setOffsetDialogObjectIds(null)} />
      )}
      {booleanAssistantObjectIds && (
        <BooleanAssistantDialog
          objectIds={booleanAssistantObjectIds}
          onClose={() => setBooleanAssistantObjectIds(null)}
        />
      )}
      {barcodeDialogLayerId && (
        <BarcodeDialog layerId={barcodeDialogLayerId} onClose={() => setBarcodeDialogLayerId(null)} />
      )}
      {nestDialogObjectIds && (
        <NestDialog objectIds={nestDialogObjectIds} onClose={() => setNestDialogObjectIds(null)} />
      )}
      {closeToleranceObjectIds && (
        <CloseSelectedPathsWithToleranceDialog
          objectIds={closeToleranceObjectIds}
          onClose={() => setCloseToleranceObjectIds(null)}
        />
      )}
      {deleteDuplicatesCount !== null && (
        <DeleteDuplicatesDialog
          duplicateCount={deleteDuplicatesCount}
          onCancel={() => resolveDeleteDuplicatesDialog(false)}
          onConfirm={() => resolveDeleteDuplicatesDialog(true)}
        />
      )}
      {showMaterialTestDialog && (
        <MaterialTestDialog onClose={() => setShowMaterialTestDialog(false)} />
      )}
      {showResetPreferencesDialog && (
        <ResetPreferencesDialog
          onCancel={() => setShowResetPreferencesDialog(false)}
          onConfirm={confirmResetPreferences}
        />
      )}
      {showFocusTestDialog && (
        <FocusTestDialog onClose={() => setShowFocusTestDialog(false)} />
      )}
      {showIntervalTestDialog && (
        <IntervalTestDialog onClose={() => setShowIntervalTestDialog(false)} />
      )}
      {feedbackDialog && (
        <FeedbackReportDialog
          kind={feedbackDialog.kind}
          title={feedbackDialog.title}
          description={feedbackDialog.description}
          notes={feedbackDialog.notes}
          sourceContext={feedbackDialog.sourceContext}
          onClose={() => setFeedbackDialog(null)}
        />
      )}
      <PreviewGenerationDialog />
      <NestingOverlay />
      <UnsavedChangesDialog />
      <PreviewWindowBridge />
    </>
  );
}

export default App;
