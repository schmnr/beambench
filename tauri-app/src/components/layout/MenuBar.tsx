import { useState, useRef, useEffect, type FocusEvent as ReactFocusEvent, type KeyboardEvent as ReactKeyboardEvent, type MouseEvent as ReactMouseEvent, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import i18n, { SUPPORTED_LOCALES, type SupportedLocale } from '../../i18n';
import { MENU_LABEL_KEYS, type MenuLabelEnglish } from '../../i18n/menuLabelKeys';
import type { AlignmentType, DistributionDirection } from '../../types/project';
import { useProjectStore } from '../../stores/projectStore';
import { useMachineStore } from '../../stores/machineStore';
import { usePreviewStore } from '../../stores/previewStore';
import { useUiStore } from '../../stores/uiStore';
import { useAppStore } from '../../stores/appStore';
import { useUndoStore } from '../../stores/undoStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { wrapBackendError } from '../../i18n/errors';
import { useUpdateStore } from '../../stores/updateStore';
import { appService } from '../../services/appService';
import { openFeedbackReport } from '../../feedbackEvents';
import { executeAppCommand, QUICK_HELP_DOCS_URL, type AppCommandDialogActions } from '../../commands/appCommands';
import { APP_COMMANDS } from '../../commands/appCommandIds';
import { MachineProfileDialog } from '../machine/MachineProfileDialog';
import { DeviceSettingsDialog } from '../dialogs/DeviceSettingsDialog';
import { SettingsDialog } from '../settings/SettingsDialog';
import { AboutDialog } from '../settings/AboutDialog';
import { GridArrayDialog } from '../dialogs/GridArrayDialog';
import { CircularArrayDialog } from '../dialogs/CircularArrayDialog';
import { OffsetDialog } from '../dialogs/OffsetDialog';
import { BooleanAssistantDialog } from '../dialogs/BooleanAssistantDialog';
import { TraceImageDialog } from '../dialogs/TraceImageDialog';
import { AdjustImageDialog } from '../dialogs/AdjustImageDialog';
import { BarcodeDialog } from '../dialogs/BarcodeDialog';
import { MaterialTestDialog } from '../dialogs/MaterialTestDialog';
import { FocusTestDialog } from '../dialogs/FocusTestDialog';
import { IntervalTestDialog } from '../dialogs/IntervalTestDialog';
import { CopyAlongPathDialog } from '../dialogs/CopyAlongPathDialog';
import { NestDialog } from '../dialogs/NestDialog';
import {
  createSelectionContext,
  isClosedVectorCompatible,
  orderSelectedObjects,
  pickLastSelectedVectorGuide,
  resolveEffectiveData,
} from '../../commands/selectionContext';
import type { RecentFile } from '../../types/commands';
import { findAutoGroupCandidates } from '../../utils/autoGroupCandidates';
import {
  WINDOW_PANEL_TOOLBAR_MENU_ITEMS,
  WINDOW_VIEW_STYLE_ITEMS,
} from '../../commands/windowMenuDefinitions';

const EMPTY_RECENT_FILES: RecentFile[] = [];
const noop = () => undefined;
const APP_NAME = 'Beam Bench';
const BRAND_MARK = '\u25c6';
const CHECK_MARK = '\u2713';

const LANGUAGE_DISPLAY: Record<SupportedLocale, string> = {
  en: 'English',
  de: 'Deutsch (German)',
  'es-ES': 'Español (Spanish)',
  'es-419': 'Español, Latinoamérica (Spanish, Latin America)',
  fr: 'Français (French)',
  it: 'Italiano (Italian)',
  'pt-BR': 'Português, Brasil (Portuguese, Brazil)',
  nl: 'Nederlands (Dutch)',
  pl: 'Polski (Polish)',
  cs: 'Čeština (Czech)',
  sv: 'Svenska (Swedish)',
  nb: 'Norsk bokmål (Norwegian Bokmål)',
  da: 'Dansk (Danish)',
  fi: 'Suomi (Finnish)',
  hu: 'Magyar (Hungarian)',
  tr: 'Türkçe (Turkish)',
  el: 'Ελληνικά (Greek)',
  ru: 'Русский (Russian)',
  sl: 'Slovenščina (Slovenian)',
  ja: '日本語 (Japanese)',
  ko: '한국어 (Korean)',
  'zh-CN': '简体中文 (Simplified Chinese)',
  'zh-TW': '繁體中文 (Traditional Chinese)',
};

export function MenuBar() {
  const { t } = useTranslation();
  const createProject = useProjectStore((s) => s.createProject);
  const openProject = useProjectStore((s) => s.openProject);
  const openProjectFromPath = useProjectStore((s) => s.openProjectFromPath);
  const saveProject = useProjectStore((s) => s.saveProject);
  const saveProjectAs = useProjectStore((s) => s.saveProjectAs);
  const importFiles = useProjectStore((s) => s.importFiles);
  const project = useProjectStore((s) => s.project);
  const selectedLayerId = useProjectStore((s) => s.selectedLayerId);
  const selectedObjectIds = useProjectStore((s) => s.selectedObjectIds);
  const removeObjects = useProjectStore((s) => s.removeObjects);
  const selectAllObjects = useProjectStore((s) => s.selectAllObjects);
  const duplicateObjects = useProjectStore((s) => s.duplicateObjects);
  const groupObjects = useProjectStore((s) => s.groupObjects);
  const ungroupObjects = useProjectStore((s) => s.ungroupObjects);
  const lockObjects = useProjectStore((s) => s.lockObjects);
  const unlockObjects = useProjectStore((s) => s.unlockObjects);
  const pushDrawOrder = useProjectStore((s) => s.pushDrawOrder);
  const autoJoinShapes = useProjectStore((s) => s.autoJoinShapes);
  const optimizeShapes = useProjectStore((s) => s.optimizeShapes);
  const selectOpenShapes = useProjectStore((s) => s.selectOpenShapes);
  const selectOpenShapesSetToFill = useProjectStore((s) => s.selectOpenShapesSetToFill);
  const selectAllShapesInCurrentLayer = useProjectStore((s) => s.selectAllShapesInCurrentLayer);
  const selectContainedShapes = useProjectStore((s) => s.selectContainedShapes);
  const selectShapesSmallerThanSelected = useProjectStore((s) => s.selectShapesSmallerThanSelected);
  const convertToPath = useProjectStore((s) => s.convertToPath);
  const breakApart = useProjectStore((s) => s.breakApart);
  const closePath = useProjectStore((s) => s.closePath);
  const refreshImage = useProjectStore((s) => s.refreshImage);
  const replaceImage = useProjectStore((s) => s.replaceImage);
  const replaceImageToFit = useProjectStore((s) => s.replaceImageToFit);
  const moveObjectsTogether = useProjectStore((s) => s.moveObjectsTogether);
  const alignObjects = useProjectStore((s) => s.alignObjects);
  const distributeObjects = useProjectStore((s) => s.distributeObjects);
  const mirrorAcrossLine = useProjectStore((s) => s.mirrorAcrossLine);
  const sidePanelsVisible = useUiStore((s) => s.sidePanelsVisible);
  const toggleSidePanels = useUiStore((s) => s.toggleSidePanels);
  const hasClipboard = useUiStore((s) => s.hasClipboard);
  const hiddenPanelIds = useUiStore((s) => s.panelLayout.hiddenPanelIds);
  const toolbarVisibility = useUiStore((s) => s.panelLayout.toolbarVisibility);
  const showNotesDialog = useUiStore((s) => s.showNotesDialog);
  const toggleNotesDialog = useUiStore((s) => s.toggleNotesDialog);
  const recentFiles = useAppStore((s) => s.settings?.recent_files) ?? EMPTY_RECENT_FILES;
  const displayLanguage = useAppStore((s) => s.settings?.display_language ?? 'en');
  const fetchSettings = useAppStore((s) => s.fetchSettings);
  const checkForUpdates = useUpdateStore((s) => s.checkForUpdates);

  const sessionState = useMachineStore((s) => s.sessionState);
  const machineStatus = useMachineStore((s) => s.machineStatus);
  const jobProgress = useMachineStore((s) => s.jobProgress);
  const loading = useMachineStore((s) => s.loading);
  const refreshPorts = useMachineStore((s) => s.refreshPorts);
  const disconnect = useMachineStore((s) => s.disconnect);
  const home = useMachineStore((s) => s.home);
  const unlock = useMachineStore((s) => s.unlock);
  const runPreflight = useMachineStore((s) => s.runPreflight);
  const startJob = useMachineStore((s) => s.startJob);
  const pauseJob = useMachineStore((s) => s.pauseJob);
  const resumeJob = useMachineStore((s) => s.resumeJob);
  const cancelJob = useMachineStore((s) => s.cancelJob);
  const emergencyStop = useMachineStore((s) => s.emergencyStop);
  const setWorkOrigin = useMachineStore((s) => s.setWorkOrigin);
  const previewState = usePreviewStore((s) => s.state);
  const generatePreview = usePreviewStore((s) => s.generatePreview);

  const zoomIn = useUiStore((s) => s.zoomIn);
  const zoomOut = useUiStore((s) => s.zoomOut);
  const toggleGrid = useUiStore((s) => s.toggleGrid);
  const toggleSnap = useUiStore((s) => s.toggleSnap);
  const toggleSnapToObjects = useUiStore((s) => s.toggleSnapToObjects);
  const gridVisible = useUiStore((s) => s.gridVisible);
  const snapToGrid = useUiStore((s) => s.snapToGrid);
  const snapToObjects = useUiStore((s) => s.snapToObjects);
  const togglePreview = usePreviewStore((s) => s.togglePreview);
  const previewWindowOpen = usePreviewStore((s) => s.previewWindowOpen);
  const viewStyle = useUiStore((s) => s.viewStyle);

  const canUndo = useUndoStore((s) => s.canUndo);
  const canRedo = useUndoStore((s) => s.canRedo);
  const undo = useUndoStore((s) => s.undo);
  const redo = useUndoStore((s) => s.redo);

  const [openMenu, setOpenMenu] = useState<string | null>(null);
  const [showProfileDialog, setShowProfileDialog] = useState(false);
  const [showDeviceSettingsDialog, setShowDeviceSettingsDialog] = useState(false);
  const [showSettingsDialog, setShowSettingsDialog] = useState(false);
  const [showAboutDialog, setShowAboutDialog] = useState(false);
  const [gridArrayDialogObjectIds, setGridArrayDialogObjectIds] = useState<string[] | null>(null);
  const [circularArrayDialogObjectIds, setCircularArrayDialogObjectIds] = useState<string[] | null>(null);
  const [copyAlongPathDialogState, setCopyAlongPathDialogState] = useState<{ objectIds: string[]; pathObjectId: string } | null>(null);
  const [offsetDialogObjectIds, setOffsetDialogObjectIds] = useState<string[] | null>(null);
  const [nestDialogObjectIds, setNestDialogObjectIds] = useState<string[] | null>(null);
  const [booleanAssistantObjectIds, setBooleanAssistantObjectIds] = useState<string[] | null>(null);
  const [traceDialogObjectId, setTraceDialogObjectId] = useState<string | null>(null);
  const [adjustDialogObjectId, setAdjustDialogObjectId] = useState<string | null>(null);
  const [barcodeDialogLayerId, setBarcodeDialogLayerId] = useState<string | null>(null);
  const [showMaterialTestDialog, setShowMaterialTestDialog] = useState(false);
  const [showResetPreferencesConfirm, setShowResetPreferencesConfirm] = useState(false);
  const [showFocusTestDialog, setShowFocusTestDialog] = useState(false);
  const [showIntervalTestDialog, setShowIntervalTestDialog] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const startInFlightRef = useRef(false);
  const [startInFlight, setStartInFlight] = useState(false);
  const ml = (label: MenuLabelEnglish) => t(MENU_LABEL_KEYS[label]);
  const mlDynamic = (label: string) => {
    const key = MENU_LABEL_KEYS[label as MenuLabelEnglish];
    return key ? t(key) : label;
  };

  // Load recent files when File menu opens
  useEffect(() => {
    if (openMenu === 'file') {
      void fetchSettings();
    }
  }, [fetchSettings, openMenu]);

  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setOpenMenu(null);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const runMenuAsync = async (action: () => unknown | Promise<unknown>) => {
    try {
      await action();
    } catch (e) {
      if (String(e).toLowerCase().includes('cancelled')) {
        return;
      }
      useNotificationStore.getState().push(wrapBackendError(String(e)), 'error');
    }
  };

  const handleImport = () => {
    setOpenMenu(null);
    if (!project) return;
    const layerId = selectedLayerId ?? project.layers[0]?.id ?? '';
    importFiles(layerId);
  };

  const handleExportArtwork = async () => {
    setOpenMenu(null);
    await executeAppCommand(APP_COMMANDS.FILE_EXPORT);
  };

  const handleSaveMachineFiles = async () => {
    setOpenMenu(null);
    await executeAppCommand(APP_COMMANDS.FILE_SAVE_MACHINE_FILES);
  };

  const handleSaveProcessedBitmap = async () => {
    setOpenMenu(null);
    await executeAppCommand(APP_COMMANDS.FILE_SAVE_PROCESSED_BITMAP);
  };

  const handlePrint = async (mode: 'black' | 'color') => {
    setOpenMenu(null);
    await executeAppCommand(mode === 'black' ? APP_COMMANDS.FILE_PRINT_BLACK : APP_COMMANDS.FILE_PRINT_COLORS);
  };

  const handleImportPreferences = async () => {
    setOpenMenu(null);
    await runMenuAsync(async () => {
      const path = await appService.pickPreferencesImportPath();
      const settings = await appService.importPreferences(path);
      useAppStore.getState().applySettings(settings);
      useNotificationStore.getState().push(t('menus.file.preferences_imported'), 'success');
    });
  };

  const handleExportPreferences = async () => {
    setOpenMenu(null);
    await runMenuAsync(async () => {
      const path = await appService.pickPreferencesExportPath();
      await appService.exportPreferences(path);
      useNotificationStore.getState().push(t('menus.file.preferences_exported'), 'success');
    });
  };

  const handleOpenPreferencesFolder = async () => {
    setOpenMenu(null);
    await runMenuAsync(() => appService.openPreferencesFolder());
  };

  const handleResetPreferences = () => {
    setOpenMenu(null);
    setShowResetPreferencesConfirm(true);
  };

  const handleResetPreferencesConfirm = async () => {
    setShowResetPreferencesConfirm(false);
    await runMenuAsync(async () => {
      const settings = await appService.resetPreferences();
      useAppStore.getState().applySettings(settings);
      useNotificationStore.getState().push(t('menus.file.preferences_reset'), 'success');
    });
  };

  const handleConnect = () => {
    setOpenMenu(null);
    void refreshPorts();
    setShowDeviceSettingsDialog(true);
  };

  const handleDisconnect = async () => {
    setOpenMenu(null);
    await disconnect();
  };

  const handleHome = async () => {
    setOpenMenu(null);
    await home();
  };

  const handleUnlock = async () => {
    setOpenMenu(null);
    await unlock();
  };

  const handleRunPreflight = async () => {
    setOpenMenu(null);
    const report = await runPreflight();
    if (report) {
      useMachineStore.getState().openPreflightDialog();
    }
  };

  const handleStartJob = async () => {
    setOpenMenu(null);
    if (startInFlightRef.current) return;

    startInFlightRef.current = true;
    setStartInFlight(true);

    try {
      let previewReady = previewState === 'current';
      if (!previewReady) {
        previewReady = await generatePreview();
      }
      if (!previewReady) {
        return;
      }

      const report = await runPreflight();
      if (!report) return;
      if (report.outcome === 'pass') {
        await startJob();
      } else {
        useMachineStore.getState().openPreflightDialog();
      }
    } finally {
      startInFlightRef.current = false;
      setStartInFlight(false);
    }
  };

  const handlePauseJob = async () => {
    setOpenMenu(null);
    await pauseJob();
  };

  const handleResumeJob = async () => {
    setOpenMenu(null);
    await resumeJob();
  };

  const handleCancelJob = async () => {
    setOpenMenu(null);
    await cancelJob();
  };

  const handleEmergencyStop = async () => {
    setOpenMenu(null);
    await emergencyStop();
  };

  const handleSetOrigin = async () => {
    setOpenMenu(null);
    await setWorkOrigin();
  };

  const handleProfiles = () => {
    setOpenMenu(null);
    setShowProfileDialog(true);
  };

  const handleDeviceSettings = () => {
    setOpenMenu(null);
    setShowDeviceSettingsDialog(true);
  };

  const handleUndo = async () => {
    setOpenMenu(null);
    await undo();
  };

  const handleRedo = async () => {
    setOpenMenu(null);
    await redo();
  };

  const handleDeleteSelected = () => {
    setOpenMenu(null);
    if (selectedObjectIds.length > 0) {
      void removeObjects([...selectedObjectIds]);
    }
  };

  const handleDuplicateSelected = () => {
    setOpenMenu(null);
    if (selectedObjectIds.length > 0) {
      void duplicateObjects(selectedObjectIds);
    }
  };

  const handleSelectAll = () => {
    setOpenMenu(null);
    selectAllObjects();
  };

  const handleGroup = async () => {
    setOpenMenu(null);
    if (selectedObjectIds.length >= 2) {
      await groupObjects(selectedObjectIds);
    }
  };

  const handleUngroup = async () => {
    setOpenMenu(null);
    if (selectedObjectIds.length === 1) {
      await ungroupObjects(selectedObjectIds[0]);
    }
  };

  const handleCopyAlongPath = async () => {
    setOpenMenu(null);
    const guide = copyAlongGuideObject;
    if (!guide) return;
    const sourceIds = selectionOrderedObjects
      .filter((o) => o.id !== guide.id)
      .map((o) => o.id);
    if (sourceIds.length === 0) return;
    setCopyAlongPathDialogState({
      objectIds: sourceIds,
      pathObjectId: guide.id,
    });
  };

  const handleAlign = async (alignmentType: AlignmentType) => {
    setOpenMenu(null);
    if (selectedObjectIds.length < 2) return;
    await alignObjects(selectedObjectIds, alignmentType);
  };

  const handleDistribute = async (direction: DistributionDirection) => {
    setOpenMenu(null);
    if (selectedObjectIds.length < 3) return;
    await distributeObjects(selectedObjectIds, direction);
  };

  const handleMoveTogether = async (axis: 'horizontal' | 'vertical') => {
    setOpenMenu(null);
    await moveObjectsTogether(axis);
  };

  const handleMirrorAcrossLine = async () => {
    setOpenMenu(null);
    await mirrorAcrossLine();
  };

  const menuCommandDialogs: AppCommandDialogActions = {
    openTraceImage: setTraceDialogObjectId,
    openAdjustImage: setAdjustDialogObjectId,
    openBarcode: setBarcodeDialogLayerId,
    openOffset: (objectIds) => setOffsetDialogObjectIds([...objectIds]),
    openBooleanAssistant: (objectIds) => setBooleanAssistantObjectIds([...objectIds]),
    openGridArray: (objectIds) => setGridArrayDialogObjectIds([...objectIds]),
    openCircularArray: (objectIds) => setCircularArrayDialogObjectIds([...objectIds]),
    openCopyAlongPath: (objectIds, pathObjectId) => setCopyAlongPathDialogState({ objectIds: [...objectIds], pathObjectId }),
    openNest: (objectIds) => setNestDialogObjectIds([...objectIds]),
  };

  const handleAppCommand = async (commandId: string) => {
    setOpenMenu(null);
    await executeAppCommand(commandId, menuCommandDialogs, { source: 'menu' });
  };

  const handleOpenRecent = (path: string) => {
    setOpenMenu(null);
    void openProjectFromPath(path).finally(() => {
      void fetchSettings();
    });
  };

  const isConnected = sessionState === 'ready' || sessionState === 'running' || sessionState === 'paused' || sessionState === 'alarm';
  const isAnyConnection = sessionState !== 'disconnected';
  const canHome = isConnected && machineStatus?.run_state === 'idle';
  const canUnlock = sessionState === 'alarm';
  const canRunPreflight = sessionState === 'ready' && machineStatus?.run_state === 'idle';
  const canStartJob =
    sessionState === 'ready' &&
    machineStatus?.run_state === 'idle' &&
    !loading &&
    previewState !== 'generating' &&
    !startInFlight;
  const canPauseJob = jobProgress?.state === 'running';
  const canResumeJob = jobProgress?.state === 'paused';
  const canCancelJob =
    jobProgress?.state === 'preparing' || jobProgress?.state === 'running' || jobProgress?.state === 'paused';
  // Shared selection context (used by both MenuBar and context menu)
  const selCtx = createSelectionContext(
    selectedObjectIds,
    project?.objects ?? [],
    hasClipboard,
    hiddenPanelIds,
    project?.assets ?? [],
  );
  const {
    hasSelection, selectedObjects, singleSelected: singleSelectedObject,
    canGroup, canUngroup, canMutate, canConvertToPath, canClosePath, canBreakApart,
    canConvertToBitmap: canConvertToBitmapCtx,
  } = selCtx;
  const booleanPending = useProjectStore((s) => s.booleanPending);
  const canBoolean = selCtx.canBoolean && !booleanPending;
  const hasMovableSelection = selectedObjects.some((object) => !object.locked);
  const lockedSelectedObjectIds = selectedObjects.filter((object) => object.locked).map((object) => object.id);
  const unlockedSelectedObjectIds = selectedObjects.filter((object) => !object.locked).map((object) => object.id);
  const unlockedSelection = selectedObjects.length > 0 && selectedObjects.every((object) => !object.locked);
  const canAlign = selectedObjectIds.length >= 2 && hasMovableSelection;
  const canDistribute = selectedObjectIds.length >= 3 && hasMovableSelection;
  const canAutoGroup = findAutoGroupCandidates(project, selectedObjectIds).length > 0;
  const canMirrorAcrossLine = selectedObjectIds.length >= 2 && unlockedSelection;
  const canMoveTogether = selectedObjectIds.length >= 2 && hasMovableSelection;
  const canDock = hasSelection && unlockedSelection;
  const canMoveSelected = hasSelection && unlockedSelection;
  const canMoveLaserToSelection = hasSelection;
  // Resolve VirtualClone chains so clone-backed text/raster/vector
  // objects keep their menu affordances — the backend commands all
  // call ensure_resolved before operating, so the UI must mirror the
  // effective type check instead of looking at the clone wrapper's
  // own data.type.
  const allObjects = project?.objects ?? [];
  const selectionOrderedObjects = orderSelectedObjects(selectedObjectIds, allObjects);
  const effectiveType = (o: typeof selectedObjects[number]) =>
    resolveEffectiveData(o, allObjects)?.type;
  const isEffectiveVectorType = (t: string | undefined) =>
    t === 'vector_path' || t === 'shape' || t === 'polygon' || t === 'star';
  const selectedTextObject = selectedObjects.find((o) => effectiveType(o) === 'text') ?? null;
  const selectedRasterObject = selectedObjects.find((o) => effectiveType(o) === 'raster_image') ?? null;
  const selectedPathObject = selectedObjects.find((o) => isEffectiveVectorType(effectiveType(o))) ?? null;
  const selectedMaskObjects = selectedObjects.filter((o) => isEffectiveVectorType(effectiveType(o)));
  const copyAlongGuideObject = pickLastSelectedVectorGuide(selectedObjectIds, allObjects);
  const canApplyPathToText = selectedObjects.length === 2 && Boolean(selectedTextObject && selectedPathObject && selectedTextObject.id !== selectedPathObject.id);
  const canCropImage = selectedObjects.length === 2 && Boolean(selectedRasterObject && selectedPathObject && selectedRasterObject.id !== selectedPathObject.id);
  const canApplyMaskToImage = Boolean(selectedRasterObject)
    && selectedMaskObjects.length > 0
    && selectedMaskObjects.every((o) =>
      o.id !== selectedRasterObject?.id && isClosedVectorCompatible(o, allObjects)
    );
  // Replace Image swaps the owned raster asset on a concrete
  // RasterImage only; clones don't own the asset so the backend
  // rejects them.
  const canReplaceImage = singleSelectedObject?.data.type === 'raster_image';
  const canRefreshImage = selCtx.canRefreshImage;
  const canSelectContainedShapes = selCtx.canSelectContainedShapes;
  const canConvertToBitmap = canConvertToBitmapCtx;
  const canCopyAlongPath = selectedObjects.length >= 2
    && Boolean(copyAlongGuideObject)
    && selectedObjects.some((o) => o.id !== copyAlongGuideObject?.id);
  const canJoin = selectedObjectIds.length >= 2 && !booleanPending
    && selectedObjects.every((o) => !o.locked && isEffectiveVectorType(effectiveType(o)));
  return (
    <div
      ref={menuRef}
      className="no-select flex items-center h-8 bg-bb-panel px-3 gap-4 text-sm border-b border-bb-border"
    >
      <span className="text-bb-accent font-semibold">{BRAND_MARK} {APP_NAME}</span>

      {/* File menu */}
      <div className="relative">
        <button
          className="text-bb-text-muted hover:text-bb-text"
          onClick={() => setOpenMenu(openMenu === 'file' ? null : 'file')}
        >
          {ml("File")}
        </button>
        {openMenu === 'file' && (
          <div className="absolute top-full left-0 mt-0.5 bg-bb-panel border border-bb-border rounded shadow-lg py-1 min-w-[220px] z-50">
            <MenuItem
              label={ml("New")}
              shortcut="Ctrl+N"
              onClick={() => {
                setOpenMenu(null);
                createProject(t('menus.file.untitled_project'));
              }}
            />
            <MenuItem
              label={ml("New Window")}
              onClick={() => {
                setOpenMenu(null);
                void executeAppCommand(APP_COMMANDS.FILE_NEW_WINDOW);
              }}
            />
            <MenuSubmenu label={ml("Recent Projects")}>
              {recentFiles.length === 0 ? (
                <MenuItem label={ml("No Recent Projects")} disabled onClick={noop} />
              ) : (
                recentFiles.map((file) => (
                  <MenuItem
                    key={file.path}
                    label={file.name || file.path}
                    onClick={() => handleOpenRecent(file.path)}
                  />
                ))
              )}
            </MenuSubmenu>
            <MenuItem
              label={ml("Open")}
              shortcut="Ctrl+O"
              onClick={() => {
                setOpenMenu(null);
                void openProject().finally(() => {
                  void fetchSettings();
                });
              }}
            />
            <MenuItem
              label={ml("Import")}
              shortcut="Ctrl+I"
              disabled={!project}
              onClick={handleImport}
            />
            <MenuItem
              label={showNotesDialog ? ml("Hide Notes") : ml("Show Notes")}
              shortcut="Ctrl+Alt+N"
              disabled={!project}
              onClick={() => {
                setOpenMenu(null);
                toggleNotesDialog();
              }}
            />
            <div className="border-t border-bb-border my-1" />
            <MenuItem
              label={ml("Save")}
              shortcut="Ctrl+S"
              disabled={!project}
              onClick={() => {
                setOpenMenu(null);
                void saveProject().finally(() => {
                  void fetchSettings();
                });
              }}
            />
            <MenuItem
              label={ml("Save As")}
              shortcut="Ctrl+Shift+S"
              disabled={!project}
              onClick={() => {
                setOpenMenu(null);
                void saveProjectAs().finally(() => {
                  void fetchSettings();
                });
              }}
            />
            <MenuItem
              label={selectedObjectIds.length > 0 ? ml("Export Selection") : ml("Export")}
              shortcut="Alt+X"
              disabled={!project}
              onClick={handleExportArtwork}
            />
            <div className="border-t border-bb-border my-1" />
            <MenuSubmenu label={ml("Preferences")}>
              <MenuItem label={ml("Import Prefs")} onClick={() => { void handleImportPreferences(); }} />
              <MenuItem label={ml("Export Prefs")} onClick={() => { void handleExportPreferences(); }} />
              <MenuItem label={ml("Open Prefs Folder")} onClick={() => { void handleOpenPreferencesFolder(); }} />
              <MenuItem
                label={ml("Reset Prefs to Defaults")}
                onClick={handleResetPreferences}
              />
            </MenuSubmenu>
            <div className="border-t border-bb-border my-1" />
            <MenuItem
              label={ml("Print (black only)")}
              shortcut="Ctrl+P"
              disabled={!project}
              onClick={() => { void handlePrint('black'); }}
            />
            <MenuItem
              label={ml("Print (keep colors)")}
              shortcut="Ctrl+Shift+P"
              disabled={!project}
              onClick={() => { void handlePrint('color'); }}
            />
            <MenuItem
              label={ml("Save Processed Bitmap")}
              disabled={!selCtx.canSaveProcessedBitmap}
              onClick={handleSaveProcessedBitmap}
            />
            <MenuItem label={ml("Save Background Capture")} disabled onClick={noop} />
          </div>
        )}
      </div>

      {/* Edit menu */}
      <div className="relative">
        <button
          className="text-bb-text-muted hover:text-bb-text"
          onClick={() => setOpenMenu(openMenu === 'edit' ? null : 'edit')}
        >
          {ml("Edit")}
        </button>
        {openMenu === 'edit' && (
          <div className="absolute top-full left-0 mt-0.5 bg-bb-panel border border-bb-border rounded shadow-lg py-1 min-w-[180px] z-50 max-h-[70vh] overflow-y-auto">
            <MenuItem label={ml("Undo")} shortcut="Ctrl+Z" disabled={!canUndo} onClick={handleUndo} />
            <MenuItem label={ml("Redo")} shortcut="Ctrl+Shift+Z" disabled={!canRedo} onClick={handleRedo} />
            <div className="border-t border-bb-border my-1" />
            <MenuItem label={ml("Select All")} shortcut="Ctrl+A" disabled={!project} onClick={handleSelectAll} />
            <MenuItem
              label={ml("Invert Selection")}
              shortcut="Ctrl+Shift+I"
              disabled={!project}
              onClick={() => {
                setOpenMenu(null);
                if (project) {
                  const allIds = project.objects.map((o) => o.id);
                  const sel = new Set(selectedObjectIds);
                  useProjectStore.getState().selectObjects(allIds.filter((id) => !sel.has(id)));
                }
              }}
            />
            <div className="border-t border-bb-border my-1" />
            <MenuItem
              label={ml("Cut")}
              shortcut="Ctrl+X"
              disabled={!canMutate}
              onClick={() => { setOpenMenu(null); void executeAppCommand(APP_COMMANDS.EDIT_CUT); }}
            />
            <MenuItem
              label={ml("Copy")}
              shortcut="Ctrl+C"
              disabled={!hasSelection}
              onClick={() => { setOpenMenu(null); void executeAppCommand(APP_COMMANDS.EDIT_COPY); }}
            />
            <MenuItem label={ml("Duplicate")} shortcut="Ctrl+D" disabled={!canMutate} onClick={handleDuplicateSelected} />
            <MenuItem
              label={ml("Paste")}
              shortcut="Ctrl+V"
              disabled={!project || !hasClipboard}
              onClick={() => { setOpenMenu(null); void executeAppCommand(APP_COMMANDS.EDIT_PASTE); }}
            />
            <MenuItem
              label={ml("Paste in Place")}
              shortcut="Alt+V"
              disabled={!project || !hasClipboard}
              onClick={() => { setOpenMenu(null); void executeAppCommand(APP_COMMANDS.EDIT_PASTE_IN_PLACE); }}
            />
            <MenuItem
              label={ml("Delete")}
              shortcut="Del"
              disabled={!canMutate}
              onClick={handleDeleteSelected}
            />
            <div className="border-t border-bb-border my-1" />
            <MenuItem
              label={ml("Convert to Path")}
              shortcut="Ctrl+Shift+C"
              disabled={!canConvertToPath}
              onClick={() => {
                setOpenMenu(null);
                void convertToPath(selectedObjectIds[0]);
              }}
            />
            <MenuItem
              label={ml("Convert to Bitmap")}
              shortcut="Ctrl+Shift+B"
              disabled={!canConvertToBitmap}
              onClick={() => { void handleAppCommand(APP_COMMANDS.EDIT_CONVERT_TO_BITMAP); }}
            />
            <div className="border-t border-bb-border my-1" />
            <MenuItem
              label={ml("Close Path")}
              disabled={!canClosePath}
              onClick={() => {
                setOpenMenu(null);
                void Promise.all(selectedObjectIds.map((id) => closePath(id)));
              }}
            />
            <MenuItem
              label={ml("Close Selected Paths With Tolerance")}
              disabled={!canClosePath}
              onClick={() => {
                setOpenMenu(null);
                void executeAppCommand(APP_COMMANDS.EDIT_CLOSE_SELECTED_PATHS_WITH_TOLERANCE);
              }}
            />
            <MenuItem
              label={ml("Auto-Join Selected Shapes")}
              shortcut="Alt+J"
              disabled={!canClosePath}
              onClick={() => {
                setOpenMenu(null);
                void autoJoinShapes(selectedObjectIds, 0.05);
              }}
            />
            <MenuItem
              label={ml("Close & Join")}
              disabled={!canJoin}
              onClick={() => { void handleAppCommand(APP_COMMANDS.EDIT_CLOSE_AND_JOIN); }}
            />
            <MenuItem
              label={ml("Optimize Selected Shapes")}
              shortcut="Alt+Shift+O"
              disabled={!canClosePath}
              onClick={() => {
                setOpenMenu(null);
                void optimizeShapes(selectedObjectIds);
              }}
            />
            <MenuItem
              label={ml("Delete Duplicates")}
              shortcut="Alt+D"
              disabled={!project}
              onClick={() => {
                setOpenMenu(null);
                void executeAppCommand(APP_COMMANDS.EDIT_DELETE_DUPLICATES, undefined, { source: 'menu' });
              }}
            />
            <div className="border-t border-bb-border my-1" />
            <MenuItem
              label={ml("Select Open Shapes")}
              disabled={!project}
              onClick={() => { setOpenMenu(null); void selectOpenShapes(); }}
            />
            <MenuItem
              label={ml("Select Open Shapes Set to Fill")}
              disabled={!project}
              onClick={() => { setOpenMenu(null); void selectOpenShapesSetToFill(); }}
            />
            <MenuItem
              label={ml("Select All Shapes in Current Layer")}
              disabled={!selectedLayerId}
              onClick={() => { setOpenMenu(null); void selectAllShapesInCurrentLayer(); }}
            />
            <MenuItem
              label={ml("Select Contained Shapes")}
              disabled={!canSelectContainedShapes}
              onClick={() => { setOpenMenu(null); void selectContainedShapes(); }}
            />
            <MenuItem
              label={ml("Select Shapes Smaller Than Selected")}
              disabled={!hasSelection}
              onClick={() => { setOpenMenu(null); void selectShapesSmallerThanSelected(); }}
            />
            <div className="border-t border-bb-border my-1" />
            <MenuSubmenu label={ml("Image Options")} disabled={!canReplaceImage}>
              <MenuItem label={ml("Refresh Image")} disabled={!canRefreshImage} onClick={() => { setOpenMenu(null); if (singleSelectedObject) void refreshImage(singleSelectedObject.id); }} />
              <MenuItem label={ml("Replace Image")} onClick={() => { setOpenMenu(null); if (singleSelectedObject) void replaceImage(singleSelectedObject.id); }} />
              <MenuItem label={ml("Replace Image to Fit")} onClick={() => { setOpenMenu(null); if (singleSelectedObject) void replaceImageToFit(singleSelectedObject.id); }} />
            </MenuSubmenu>
            <div className="border-t border-bb-border my-1" />
            <MenuItem
              label={ml("Settings")}
              onClick={() => {
                setOpenMenu(null);
                setShowSettingsDialog(true);
              }}
            />
          </div>
        )}
      </div>

      {/* View menu */}
      <div className="relative">
        <button
          className="text-bb-text-muted hover:text-bb-text"
          onClick={() => setOpenMenu(openMenu === 'view' ? null : 'view')}
        >
          {ml("View")}
        </button>
        {openMenu === 'view' && (
          <div className="absolute top-full left-0 mt-0.5 bg-bb-panel border border-bb-border rounded shadow-lg py-1 min-w-[340px] z-50">
            <MenuItem
              label={ml("Zoom In")}
              shortcut="Ctrl+="
              onClick={() => {
                setOpenMenu(null);
                zoomIn();
              }}
            />
            <MenuItem
              label={ml("Zoom Out")}
              shortcut="Ctrl+-"
              onClick={() => {
                setOpenMenu(null);
                zoomOut();
              }}
            />
            <div className="border-t border-bb-border my-1" />
            <MenuCheckItem
              label={ml("Grid")}
              shortcut="G"
              checked={gridVisible}
              onClick={() => {
                setOpenMenu(null);
                toggleGrid();
              }}
            />
            <MenuCheckItem
              label={ml("Snap to Grid")}
              shortcut="Ctrl+Shift+G"
              checked={snapToGrid}
              onClick={() => {
                setOpenMenu(null);
                toggleSnap();
              }}
            />
            <MenuCheckItem
              label={ml("Snap to Objects")}
              checked={snapToObjects}
              onClick={() => {
                setOpenMenu(null);
                toggleSnapToObjects();
              }}
            />
            <div className="border-t border-bb-border my-1" />
            <MenuCheckItem
              label={ml("Preview")}
              shortcut="P"
              checked={previewWindowOpen}
              onClick={() => {
                setOpenMenu(null);
                togglePreview();
              }}
            />
            <MenuItem
              label={ml("Refresh Preview")}
              shortcut="Shift+P"
              disabled={!previewWindowOpen && previewState === 'idle'}
              onClick={() => {
                setOpenMenu(null);
                void executeAppCommand(APP_COMMANDS.WINDOW_REFRESH_PREVIEW);
              }}
            />
            <MenuCheckItem
              label={ml("Side Panels")}
              shortcut="F12"
              checked={sidePanelsVisible}
              onClick={() => {
                setOpenMenu(null);
                toggleSidePanels();
              }}
            />
          </div>
        )}
      </div>

      {/* Tools menu */}
      <div className="relative">
        <button
          className="text-bb-text-muted hover:text-bb-text"
          onClick={() => setOpenMenu(openMenu === 'tools' ? null : 'tools')}
        >
          {ml("Tools")}
        </button>
        {openMenu === 'tools' && (
          <div className="absolute top-full left-0 mt-0.5 bg-bb-panel border border-bb-border rounded shadow-lg py-1 min-w-[240px] z-50 max-h-[70vh] overflow-y-auto">
            <MenuItem
              label={ml("Select")}
              shortcut="Esc"
              disabled={!project}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_SELECT); }}
            />
            <MenuItem
              label={ml("Draw Lines")}
              shortcut="Ctrl+L"
              disabled={!project}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_LINE); }}
            />
            <MenuSubmenu label={ml("Draw Shape")} disabled={!project}>
              <MenuItem label={ml("Rectangle")} shortcut="Ctrl+R" onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_RECTANGLE); }} />
              <MenuItem label={ml("Ellipse")} shortcut="Ctrl+E" onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_ELLIPSE); }} />
              <MenuItem label={ml("Triangle")} onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_TRIANGLE); }} />
              <MenuItem label={ml("Pentagon")} onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_PENTAGON); }} />
              <MenuItem label={ml("Polygon")} onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_POLYGON); }} />
              <MenuItem label={ml("Octagon")} onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_OCTAGON); }} />
              <MenuItem label={ml("Star")} onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_STAR); }} />
              <MenuItem label={ml("Dual Star")} onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_DUAL_STAR); }} />
            </MenuSubmenu>
            <MenuItem
              label={ml("Edit Nodes")}
              shortcut="Ctrl+`"
              disabled={!project}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_NODE); }}
            />
            <MenuItem
              label={ml("Trim Shapes")}
              shortcut="Ctrl+K"
              disabled={!project}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_TRIM); }}
            />
            <MenuItem
              label={ml("Add Tabs")}
              shortcut="Ctrl+Tab"
              disabled={!project}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_TABS); }}
            />
            <MenuItem
              label={ml("Edit Text")}
              shortcut="Ctrl+T"
              disabled={!project}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_TEXT); }}
            />
            <MenuItem
              label={ml("Position Laser")}
              shortcut="Ctrl+Shift+L"
              disabled={!project}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_POSITION_LASER); }}
            />
            <MenuItem
              label={ml("Measure")}
              shortcut="Ctrl+M"
              disabled={!project}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_MEASURE); }}
            />
            <MenuItem
              label={ml("Create Bar Code")}
              disabled={!project}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_BARCODE); }}
            />
            <MenuItem
              label={ml("Offset Shapes")}
              shortcut="Alt+O"
              disabled={!unlockedSelection}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_OFFSET); }}
            />
            <MenuItem
              label={ml("Weld Shapes")}
              shortcut="Ctrl+W"
              disabled={!selCtx.canWeld || booleanPending}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_BOOLEAN_WELD); }}
            />
            <MenuItem
              label={ml("Boolean Union")}
              shortcut="Alt++"
              disabled={!canBoolean}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_BOOLEAN_UNION); }}
            />
            <MenuItem
              label={ml("Boolean Subtract")}
              shortcut="Alt+-"
              disabled={!canBoolean}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_BOOLEAN_SUBTRACT); }}
            />
            <MenuItem
              label={ml("Boolean Intersection")}
              shortcut="Alt+*"
              disabled={!canBoolean}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_BOOLEAN_INTERSECTION); }}
            />
            <MenuItem
              label={ml("Boolean Assistant")}
              shortcut="Ctrl+B"
              disabled={!canBoolean}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_BOOLEAN_ASSISTANT); }}
            />
            <MenuItem
              label={ml("Cut Shapes")}
              shortcut="Alt+Shift+C"
              disabled={!canJoin}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_CUT_SHAPES); }}
            />
            <MenuItem
              label={ml("Adjust Image")}
              shortcut="Alt+I"
              disabled={!canReplaceImage}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_ADJUST_IMAGE); }}
            />
            <MenuItem
              label={ml("Trace Image")}
              shortcut="Alt+T"
              disabled={!canReplaceImage}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_TRACE_IMAGE); }}
            />
            <MenuItem
              label={ml("Apply Path to Text")}
              disabled={!canApplyPathToText}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_APPLY_PATH_TO_TEXT); }}
            />
            <MenuItem
              label={ml("Apply Mask to Image")}
              disabled={!canApplyMaskToImage}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_APPLY_MASK_TO_IMAGE); }}
            />
            <MenuItem
              label={ml("Crop Image")}
              disabled={!canCropImage}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_CROP_IMAGE); }}
            />
            <MenuItem
              label={ml("Warp Selection (4 Points)")}
              disabled={!hasSelection}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_WARP_SELECTION); }}
            />
            <MenuItem
              label={ml("Deform Selection (16 Points)")}
              disabled={!hasSelection}
              onClick={() => { void handleAppCommand(APP_COMMANDS.TOOLS_DEFORM_SELECTION); }}
            />
          </div>
        )}
      </div>

      {/* Arrange menu */}
      <div className="relative">
        <button
          className="text-bb-text-muted hover:text-bb-text"
          onClick={() => setOpenMenu(openMenu === 'arrange' ? null : 'arrange')}
        >
          {ml("Arrange")}
        </button>
        {openMenu === 'arrange' && (
          <div className="absolute top-full left-0 mt-0.5 bg-bb-panel border border-bb-border rounded shadow-lg py-1 min-w-[340px] z-50">
            <MenuItem label={ml("Group")} shortcut="Ctrl+G" disabled={!canGroup} onClick={handleGroup} />
            <MenuSubmenu label={ml("Ungroup")}>
              <MenuItem label={ml("Ungroup")} shortcut="Ctrl+U" disabled={!canUngroup} onClick={handleUngroup} />
              <MenuItem label={ml("Auto-Group")} disabled={!canAutoGroup} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_AUTO_GROUP); }} />
            </MenuSubmenu>
            <div className="border-t border-bb-border my-1" />
            <MenuSubmenu label={ml("Flip Horizontal / Vertical")}>
              <MenuItem label={ml("Flip Horizontal")} shortcut="Ctrl+Shift+H" disabled={!unlockedSelection} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_FLIP_HORIZONTAL); }} />
              <MenuItem label={ml("Flip Vertical")} shortcut="Ctrl+Shift+V" disabled={!unlockedSelection} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_FLIP_VERTICAL); }} />
            </MenuSubmenu>
            <MenuItem
              label={ml("Mirror Across Line")}
              shortcut="Ctrl+Shift+M"
              disabled={!canMirrorAcrossLine}
              onClick={() => { void handleMirrorAcrossLine(); }}
            />
            <MenuSubmenu label={ml("Rotate 90° Clockwise / Counter-Clockwise")}>
              <MenuItem label={ml("Rotate 90° Clockwise")} shortcut="." disabled={!unlockedSelection} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_ROTATE_CW); }} />
              <MenuItem label={ml("Rotate 90° Counter-Clockwise")} shortcut="," disabled={!unlockedSelection} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_ROTATE_CCW); }} />
            </MenuSubmenu>
            <MenuItem label={ml("Two-Point Rotate / Scale")} shortcut="Ctrl+2" disabled={!unlockedSelection} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_TWO_POINT_ROTATE_SCALE); }} />
            <div className="border-t border-bb-border my-1" />
            <MenuSubmenu label={ml("Align")}>
              <MenuItem label={ml("Align Centers")} disabled={!canAlign} onClick={() => handleAlign('centers_xy')} />
              <MenuItem label={ml("Align Vertical Centers")} shortcut="Alt+PgUp" disabled={!canAlign} onClick={() => handleAlign('centers_v')} />
              <MenuItem label={ml("Align Horizontal Centers")} shortcut="Alt+PgDn" disabled={!canAlign} onClick={() => handleAlign('centers_h')} />
              <MenuItem label={ml("Align Left")} shortcut="Alt+Left" disabled={!canAlign} onClick={() => handleAlign('left')} />
              <MenuItem label={ml("Align Right")} shortcut="Alt+Right" disabled={!canAlign} onClick={() => handleAlign('right')} />
              <MenuItem label={ml("Align Bottom")} shortcut="Alt+Down" disabled={!canAlign} onClick={() => handleAlign('bottom')} />
              <MenuItem label={ml("Align Top")} shortcut="Alt+Up" disabled={!canAlign} onClick={() => handleAlign('top')} />
            </MenuSubmenu>
            <MenuSubmenu label={ml("Distribute")}>
              <MenuItem label={ml("Distribute V-Spaced")} disabled={!canDistribute} onClick={() => handleDistribute('v_spaced')} />
              <MenuItem label={ml("Distribute V-Centered")} disabled={!canDistribute} onClick={() => handleDistribute('v_centered')} />
              <MenuItem label={ml("Distribute H-Spaced")} disabled={!canDistribute} onClick={() => handleDistribute('h_spaced')} />
              <MenuItem label={ml("Distribute H-Centered")} disabled={!canDistribute} onClick={() => handleDistribute('h_centered')} />
              <MenuItem label={ml("Move H Together")} shortcut="Alt+Shift+H" disabled={!canMoveTogether} onClick={() => { void handleMoveTogether('horizontal'); }} />
              <MenuItem label={ml("Move V Together")} shortcut="Alt+Shift+V" disabled={!canMoveTogether} onClick={() => { void handleMoveTogether('vertical'); }} />
            </MenuSubmenu>
            <div className="border-t border-bb-border my-1" />
            <MenuItem label={ml("Nest Selected")} disabled={!unlockedSelection} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_NEST_SELECTED); }} />
            <MenuSubmenu label={ml("Dock")} disabled={!canDock}>
              <MenuItem label={ml("Dock Left")} disabled={!canDock} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_DOCK_LEFT); }} />
              <MenuItem label={ml("Dock Right")} disabled={!canDock} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_DOCK_RIGHT); }} />
              <MenuItem label={ml("Dock Up")} disabled={!canDock} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_DOCK_UP); }} />
              <MenuItem label={ml("Dock Down")} disabled={!canDock} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_DOCK_DOWN); }} />
            </MenuSubmenu>
            <MenuSubmenu label={ml("Move Selected Objects")} disabled={!canMoveSelected}>
              <MenuItem label={ml("Move to Laser Position")} disabled={!canMoveSelected} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_MOVE_TO_LASER_POSITION); }} />
              <MenuItem label={ml("Move to Page Center")} shortcut="P" disabled={!canMoveSelected} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_MOVE_TO_PAGE_CENTER); }} />
              <MenuItem label={ml("Move to Upper Left")} disabled={!canMoveSelected} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_MOVE_TO_UPPER_LEFT); }} />
              <MenuItem label={ml("Move to Upper Right")} disabled={!canMoveSelected} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_MOVE_TO_UPPER_RIGHT); }} />
              <MenuItem label={ml("Move to Lower Left")} disabled={!canMoveSelected} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_MOVE_TO_LOWER_LEFT); }} />
              <MenuItem label={ml("Move to Lower Right")} disabled={!canMoveSelected} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_MOVE_TO_LOWER_RIGHT); }} />
              <MenuItem label={ml("Move to Left")} disabled={!canMoveSelected} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_MOVE_TO_LEFT); }} />
              <MenuItem label={ml("Move to Right")} disabled={!canMoveSelected} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_MOVE_TO_RIGHT); }} />
              <MenuItem label={ml("Move to Top")} disabled={!canMoveSelected} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_MOVE_TO_TOP); }} />
              <MenuItem label={ml("Move to Bottom")} disabled={!canMoveSelected} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_MOVE_TO_BOTTOM); }} />
            </MenuSubmenu>
            <MenuSubmenu label={ml("Move Laser to Selection")} disabled={!canMoveLaserToSelection}>
              <MenuItem label={ml("Move Laser to Selection Center")} disabled={!canMoveLaserToSelection} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_CENTER); }} />
              <MenuItem label={ml("Move Laser to Upper Left of Selection")} disabled={!canMoveLaserToSelection} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_UPPER_LEFT); }} />
              <MenuItem label={ml("Move Laser to Upper Right of Selection")} disabled={!canMoveLaserToSelection} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_UPPER_RIGHT); }} />
              <MenuItem label={ml("Move Laser to Lower Left of Selection")} disabled={!canMoveLaserToSelection} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_LOWER_LEFT); }} />
              <MenuItem label={ml("Move Laser to Lower Right of Selection")} disabled={!canMoveLaserToSelection} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_LOWER_RIGHT); }} />
              <MenuItem label={ml("Move Laser to Left of Selection")} disabled={!canMoveLaserToSelection} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_LEFT); }} />
              <MenuItem label={ml("Move Laser to Right of Selection")} disabled={!canMoveLaserToSelection} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_RIGHT); }} />
              <MenuItem label={ml("Move Laser to Top of Selection")} disabled={!canMoveLaserToSelection} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_TOP); }} />
              <MenuItem label={ml("Move Laser to Bottom of Selection")} disabled={!canMoveLaserToSelection} onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_BOTTOM); }} />
              <MenuSubmenu label={ml("Jog Laser")}>
                <MenuItem label={ml("Jog Laser Left")} shortcut="Alt+Ctrl+[" onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_JOG_LASER_LEFT); }} />
                <MenuItem label={ml("Jog Laser Right")} shortcut="Alt+Ctrl+]" onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_JOG_LASER_RIGHT); }} />
                <MenuItem label={ml("Jog Laser Up")} shortcut="Ctrl+Shift+]" onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_JOG_LASER_UP); }} />
                <MenuItem label={ml("Jog Laser Down")} shortcut="Ctrl+Shift+[" onClick={() => { void handleAppCommand(APP_COMMANDS.ARRANGE_JOG_LASER_DOWN); }} />
              </MenuSubmenu>
            </MenuSubmenu>
            <div className="border-t border-bb-border my-1" />
            <MenuItem
              label={ml("Grid Array")}
              disabled={!unlockedSelection}
              onClick={() => {
                setOpenMenu(null);
                setGridArrayDialogObjectIds([...selectedObjectIds]);
              }}
            />
            <MenuItem
              label={ml("Circular Array")}
              disabled={!unlockedSelection}
              onClick={() => {
                setOpenMenu(null);
                setCircularArrayDialogObjectIds([...selectedObjectIds]);
              }}
            />
            <MenuItem
              label={ml("Copy Along Path")}
              disabled={!canCopyAlongPath}
              onClick={handleCopyAlongPath}
            />
            <MenuItem
              label={ml("Break Apart")}
              shortcut="Alt+B"
              disabled={!canBreakApart}
              onClick={() => { setOpenMenu(null); void breakApart(selectedObjectIds[0]); }}
            />
            <div className="border-t border-bb-border my-1" />
            <MenuSubmenu label={ml("Push in Draw Order")}>
              <MenuItem label={ml("Bring Forward")} shortcut="PgUp" disabled={selectedObjectIds.length !== 1} onClick={() => { setOpenMenu(null); void pushDrawOrder(selectedObjectIds[0], 'forward'); }} />
              <MenuItem label={ml("Send Backward")} shortcut="PgDn" disabled={selectedObjectIds.length !== 1} onClick={() => { setOpenMenu(null); void pushDrawOrder(selectedObjectIds[0], 'backward'); }} />
              <MenuItem label={ml("Bring to Front")} shortcut="Ctrl+PgUp" disabled={selectedObjectIds.length !== 1} onClick={() => { setOpenMenu(null); void pushDrawOrder(selectedObjectIds[0], 'front'); }} />
              <MenuItem label={ml("Send to Back")} shortcut="Ctrl+PgDn" disabled={selectedObjectIds.length !== 1} onClick={() => { setOpenMenu(null); void pushDrawOrder(selectedObjectIds[0], 'back'); }} />
            </MenuSubmenu>
            <MenuItem
              label={ml("Lock Selected Shapes")}
              disabled={unlockedSelectedObjectIds.length === 0}
              onClick={() => { setOpenMenu(null); void lockObjects(unlockedSelectedObjectIds); }}
            />
            <MenuItem
              label={ml("Unlock Selected Shapes")}
              disabled={lockedSelectedObjectIds.length === 0}
              onClick={() => { setOpenMenu(null); void unlockObjects(lockedSelectedObjectIds); }}
            />
          </div>
        )}
      </div>

      {/* Machine menu */}
      <div className="relative">
        <button
          className="text-bb-text-muted hover:text-bb-text"
          onClick={() => setOpenMenu(openMenu === 'machine' ? null : 'machine')}
        >
          {ml("Machine")}
        </button>
        {openMenu === 'machine' && (
          <div className="absolute top-full left-0 mt-0.5 bg-bb-panel border border-bb-border rounded shadow-lg py-1 min-w-[180px] z-50">
            <MenuItem
              label={ml("Connect...")}
              disabled={isAnyConnection}
              onClick={handleConnect}
            />
            <MenuItem
              label={ml("Disconnect")}
              disabled={!isConnected}
              onClick={handleDisconnect}
            />
            <div className="border-t border-bb-border my-1" />
            <MenuItem
              label={ml("Home")}
              disabled={!canHome}
              onClick={handleHome}
            />
            <MenuItem
              label={ml("Unlock")}
              disabled={!canUnlock}
              onClick={handleUnlock}
            />
            <MenuItem
              label={ml("Set Origin")}
              disabled={!canHome || !project}
              onClick={handleSetOrigin}
            />
            <MenuItem
              label={ml("Emergency Stop")}
              disabled={!isAnyConnection}
              onClick={handleEmergencyStop}
            />
            <div className="border-t border-bb-border my-1" />
            <MenuItem
              label={ml("Run Preflight")}
              disabled={!canRunPreflight}
              onClick={handleRunPreflight}
            />
            <MenuItem
              label={ml("Start Job")}
              disabled={!canStartJob}
              onClick={handleStartJob}
            />
            <MenuItem
              label={ml("Pause Job")}
              disabled={!canPauseJob}
              onClick={handlePauseJob}
            />
            <MenuItem
              label={ml("Resume Job")}
              disabled={!canResumeJob}
              onClick={handleResumeJob}
            />
            <MenuItem
              label={ml("Cancel Job")}
              disabled={!canCancelJob}
              onClick={handleCancelJob}
            />
            <div className="border-t border-bb-border my-1" />
            <MenuItem
              label={ml("Machine Profiles...")}
              onClick={handleProfiles}
            />
            <MenuItem
              label={ml("Device Settings...")}
              onClick={handleDeviceSettings}
            />
          </div>
        )}
      </div>

      {/* Laser Tools menu (M3 quality-test workflows) */}
      <div className="relative">
        <button
          className="text-bb-text-muted hover:text-bb-text"
          onClick={() => setOpenMenu(openMenu === 'laserTools' ? null : 'laserTools')}
        >
          {ml("Laser Tools")}
        </button>
        {openMenu === 'laserTools' && (
          <div className="absolute top-full left-0 mt-0.5 bg-bb-panel border border-bb-border rounded shadow-lg py-1 min-w-[180px] z-50">
            <MenuItem
              label={ml("Save Machine Files")}
              shortcut="Alt+Shift+L"
              disabled={!project || previewState !== 'current'}
              onClick={handleSaveMachineFiles}
            />
            <div className="border-t border-bb-border my-1" />
            <MenuItem
              label={ml("Material Test...")}
              onClick={() => {
                setOpenMenu(null);
                setShowMaterialTestDialog(true);
              }}
            />
            <MenuItem
              label={ml("Focus Test...")}
              onClick={() => {
                setOpenMenu(null);
                setShowFocusTestDialog(true);
              }}
            />
            <MenuItem
              label={ml("Interval Test...")}
              onClick={() => {
                setOpenMenu(null);
                setShowIntervalTestDialog(true);
              }}
            />
          </div>
        )}
      </div>

      {/* Window menu */}
      <div className="relative">
        <button
          className="text-bb-text-muted hover:text-bb-text"
          onClick={() => setOpenMenu(openMenu === 'window' ? null : 'window')}
        >
          {ml("Window")}
        </button>
        {openMenu === 'window' && (
          <div className="absolute top-full left-0 mt-0.5 bg-bb-panel border border-bb-border rounded shadow-lg py-1 min-w-[260px] z-50">
            <MenuItem
              label={ml("Reset to Default Layout")}
              onClick={() => {
                setOpenMenu(null);
                void executeAppCommand(APP_COMMANDS.WINDOW_RESET_LAYOUT);
              }}
            />
            <div className="border-t border-bb-border my-1" />
            <MenuCheckItem
              label={ml("Preview")}
              shortcut="Alt+P"
              checked={previewWindowOpen}
              disabled={!project}
              onClick={() => {
                setOpenMenu(null);
                void executeAppCommand(APP_COMMANDS.WINDOW_PREVIEW);
              }}
            />
            <MenuItem
              label={ml("Zoom to Page")}
              shortcut="Ctrl+0"
              disabled={!project}
              onClick={() => {
                setOpenMenu(null);
                void executeAppCommand(APP_COMMANDS.WINDOW_ZOOM_TO_PAGE);
              }}
            />
            <MenuItem
              label={ml("Zoom In")}
              shortcut="Ctrl+="
              disabled={!project}
              onClick={() => {
                setOpenMenu(null);
                void executeAppCommand(APP_COMMANDS.WINDOW_ZOOM_IN);
              }}
            />
            <MenuItem
              label={ml("Zoom Out")}
              shortcut="Ctrl+-"
              disabled={!project}
              onClick={() => {
                setOpenMenu(null);
                void executeAppCommand(APP_COMMANDS.WINDOW_ZOOM_OUT);
              }}
            />
            <MenuItem
              label={ml("Frame Selection")}
              shortcut="Ctrl+Shift+A"
              disabled={selectedObjectIds.length === 0}
              onClick={() => {
                setOpenMenu(null);
                void executeAppCommand(APP_COMMANDS.WINDOW_FRAME_SELECTION);
              }}
            />
            <div className="border-t border-bb-border my-1" />
            <MenuLabel label={ml("View Style:")} />
            {WINDOW_VIEW_STYLE_ITEMS.map((item) => (
              <MenuCheckItem
                key={item.commandId}
                label={mlDynamic(item.label)}
                checked={viewStyle === item.viewStyle}
                onClick={() => {
                  setOpenMenu(null);
                  void executeAppCommand(item.commandId);
                }}
              />
            ))}
            <MenuItem
              label={ml("Toggle Wireframe / Filled")}
              shortcut="Alt+Shift+W"
              onClick={() => {
                setOpenMenu(null);
                void executeAppCommand(APP_COMMANDS.WINDOW_TOGGLE_WIREFRAME_FILLED);
              }}
            />
            <div className="border-t border-bb-border my-1" />
            <MenuCheckItem
              label={ml("Toggle Side Panels")}
              shortcut="F12"
              checked={sidePanelsVisible}
              onClick={() => {
                setOpenMenu(null);
                void executeAppCommand(APP_COMMANDS.WINDOW_SIDE_PANELS);
              }}
            />
            <div className="border-t border-bb-border my-1" />
            {WINDOW_PANEL_TOOLBAR_MENU_ITEMS.map((item) => {
              const checked = 'panelId' in item
                ? !hiddenPanelIds.includes(item.panelId)
                : toolbarVisibility[item.toolbarId];
              return (
                <MenuCheckItem
                  key={item.commandId}
                  label={mlDynamic(item.label)}
                  checked={checked}
                  onClick={() => {
                    setOpenMenu(null);
                    void executeAppCommand(item.commandId);
                  }}
                />
              );
            })}
          </div>
        )}
      </div>

      {/* Language menu */}
      <div className="relative">
        <button
          className="text-bb-text-muted hover:text-bb-text"
          onClick={() => setOpenMenu(openMenu === 'language' ? null : 'language')}
        >
          {t('menus.language.label')}
        </button>
        {openMenu === 'language' && (
          <div className="absolute top-full left-0 mt-0.5 bg-bb-panel border border-bb-border rounded shadow-lg py-1 min-w-[260px] max-h-[80vh] overflow-y-auto z-50">
            {SUPPORTED_LOCALES.map((code) => (
              <MenuCheckItem
                key={code}
                label={LANGUAGE_DISPLAY[code]}
                checked={displayLanguage === code}
                onClick={() => {
                  void useAppStore.getState().updateSettings({ display_language: code });
                  setOpenMenu(null);
                }}
              />
            ))}
            {((import.meta as ImportMeta & { env?: { DEV?: boolean } }).env?.DEV) && (
              <>
                <div className="border-t border-bb-border my-1" />
                <MenuCheckItem
                  label={ml("en-XA (Pseudo-locale)")}
                  checked={i18n.language === 'en-XA'}
                  onClick={() => {
                    void i18n.changeLanguage('en-XA');
                    setOpenMenu(null);
                  }}
                />
              </>
            )}
          </div>
        )}
      </div>

      {/* Help menu */}
      <div className="relative">
        <button
          className="text-bb-text-muted hover:text-bb-text"
          onClick={() => setOpenMenu(openMenu === 'help' ? null : 'help')}
        >
          {ml("Help")}
        </button>
        {openMenu === 'help' && (
          <div className="absolute top-full left-0 mt-0.5 bg-bb-panel border border-bb-border rounded shadow-lg py-1 min-w-[180px] z-50">
            <MenuItem
              label={ml("Quick Help")}
              shortcut="F1"
              onClick={() => {
                setOpenMenu(null);
                void runMenuAsync(() => appService.openExternalUrl(QUICK_HELP_DOCS_URL));
              }}
            />
            <MenuItem
              label={ml("Report a Bug...")}
              onClick={() => {
                setOpenMenu(null);
                openFeedbackReport({
                  kind: 'bug',
                  sourceContext: { source: 'help_menu', correlation_ts: new Date().toISOString() },
                });
              }}
            />
            <MenuItem
              label={ml("Check for Updates...")}
              onClick={() => {
                setOpenMenu(null);
                void checkForUpdates('manual');
              }}
            />
            <div className="border-t border-bb-border my-1" />
            <MenuItem
              label={ml("About Beam Bench...")}
              onClick={() => {
                setOpenMenu(null);
                setShowAboutDialog(true);
              }}
            />
          </div>
        )}
      </div>

      {showProfileDialog && (
        <MachineProfileDialog onClose={() => setShowProfileDialog(false)} />
      )}

      {showDeviceSettingsDialog && (
        <DeviceSettingsDialog onClose={() => setShowDeviceSettingsDialog(false)} />
      )}

      {showSettingsDialog && (
        <SettingsDialog onClose={() => setShowSettingsDialog(false)} />
      )}

      {showAboutDialog && (
        <AboutDialog onClose={() => setShowAboutDialog(false)} />
      )}

      {gridArrayDialogObjectIds && (
        <GridArrayDialog objectIds={gridArrayDialogObjectIds} onClose={() => setGridArrayDialogObjectIds(null)} />
      )}

      {circularArrayDialogObjectIds && (
        <CircularArrayDialog objectIds={circularArrayDialogObjectIds} onClose={() => setCircularArrayDialogObjectIds(null)} />
      )}

      {copyAlongPathDialogState && (
        <CopyAlongPathDialog
          objectIds={copyAlongPathDialogState.objectIds}
          pathObjectId={copyAlongPathDialogState.pathObjectId}
          onClose={() => setCopyAlongPathDialogState(null)}
        />
      )}

      {offsetDialogObjectIds && (
        <OffsetDialog objectIds={offsetDialogObjectIds} onClose={() => setOffsetDialogObjectIds(null)} />
      )}

      {nestDialogObjectIds && (
        <NestDialog objectIds={nestDialogObjectIds} onClose={() => setNestDialogObjectIds(null)} />
      )}

      {booleanAssistantObjectIds && (
        <BooleanAssistantDialog
          objectIds={booleanAssistantObjectIds}
          onClose={() => setBooleanAssistantObjectIds(null)}
        />
      )}

      {traceDialogObjectId && (
        <TraceImageDialog objectId={traceDialogObjectId} onClose={() => setTraceDialogObjectId(null)} />
      )}

      {adjustDialogObjectId && (
        <AdjustImageDialog objectId={adjustDialogObjectId} onClose={() => setAdjustDialogObjectId(null)} />
      )}

      {barcodeDialogLayerId && (
        <BarcodeDialog
          layerId={barcodeDialogLayerId}
          onClose={() => setBarcodeDialogLayerId(null)}
        />
      )}

      {showMaterialTestDialog && (
        <MaterialTestDialog onClose={() => setShowMaterialTestDialog(false)} />
      )}
      {showFocusTestDialog && (
        <FocusTestDialog onClose={() => setShowFocusTestDialog(false)} />
      )}
      {showIntervalTestDialog && (
        <IntervalTestDialog onClose={() => setShowIntervalTestDialog(false)} />
      )}

      {showResetPreferencesConfirm && (
        <div
          role="dialog"
          aria-modal="true"
          aria-label={ml("Reset Prefs to Defaults")}
          className="fixed inset-0 z-[110] flex items-center justify-center bg-black/50"
          onClick={(e) => {
            if (e.target === e.currentTarget) setShowResetPreferencesConfirm(false);
          }}
        >
          <div className="w-full max-w-sm rounded border border-bb-border bg-bb-panel p-4 shadow-xl">
            <div className="mb-3 text-xs font-medium uppercase tracking-wider text-bb-accent">
              {ml("Reset Prefs to Defaults")}
            </div>
            <p className="text-sm text-bb-text">{t('menus.file.reset_preferences_confirm')}</p>
            <div className="mt-4 flex justify-end gap-2">
              <button
                type="button"
                className="rounded border border-bb-border bg-bb-surface px-3 py-1.5 text-sm text-bb-text transition hover:bg-bb-hover"
                onClick={() => setShowResetPreferencesConfirm(false)}
              >
                {t('common.cancel')}
              </button>
              <button
                type="button"
                className="rounded border border-bb-accent/60 bg-bb-accent/20 px-3 py-1.5 text-sm text-bb-text transition hover:bg-bb-accent/30"
                onClick={() => { void handleResetPreferencesConfirm(); }}
              >
                {t('common.ok')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function MenuSubmenu({
  label,
  disabled,
  children,
}: {
  label: string;
  disabled?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const [pinned, setPinned] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const submenuRef = useRef<HTMLDivElement>(null);
  const closeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearCloseTimer = () => {
    if (closeTimerRef.current) {
      clearTimeout(closeTimerRef.current);
      closeTimerRef.current = null;
    }
  };

  const openSubmenu = (pin = false) => {
    clearCloseTimer();
    if (disabled) return;
    setOpen(true);
    if (pin) setPinned(true);
  };

  const closeSubmenu = () => {
    clearCloseTimer();
    setOpen(false);
    setPinned(false);
  };

  const scheduleClose = () => {
    clearCloseTimer();
    if (pinned) return;
    closeTimerRef.current = setTimeout(() => {
      setOpen(false);
    }, 180);
  };

  const focusFirstSubmenuItem = () => {
    window.setTimeout(() => {
      submenuRef.current?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
    });
  };

  const handleButtonClick = (event: ReactMouseEvent<HTMLButtonElement>) => {
    event.preventDefault();
    if (disabled) return;
    if (open && pinned) {
      closeSubmenu();
    } else {
      openSubmenu(true);
    }
  };

  const handleKeyDown = (event: ReactKeyboardEvent<HTMLDivElement | HTMLButtonElement>) => {
    if (disabled) return;
    if (event.key === 'Escape' || event.key === 'ArrowLeft') {
      event.preventDefault();
      closeSubmenu();
      buttonRef.current?.focus();
      return;
    }
    if (event.key === 'Enter' || event.key === ' ' || event.key === 'ArrowRight') {
      event.preventDefault();
      openSubmenu(true);
      focusFirstSubmenuItem();
    }
  };

  const handleBlur = (event: ReactFocusEvent<HTMLDivElement>) => {
    if (!rootRef.current?.contains(event.relatedTarget as Node | null)) {
      closeSubmenu();
    }
  };

  useEffect(() => () => {
    if (closeTimerRef.current) {
      clearTimeout(closeTimerRef.current);
    }
  }, []);

  return (
    <div
      ref={rootRef}
      className="relative"
      onMouseEnter={() => openSubmenu(false)}
      onMouseLeave={scheduleClose}
      onBlur={handleBlur}
      onKeyDown={handleKeyDown}
    >
      <button
        ref={buttonRef}
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        className={`w-full text-left px-3 py-1 text-sm flex items-center justify-between ${
          disabled
            ? 'text-bb-text-dim cursor-default'
            : 'text-bb-text hover:bg-bb-hover'
        }`}
        onClick={handleButtonClick}
        disabled={disabled}
      >
        <span>{label}</span>
        <span className="text-bb-text-dim text-xs ml-4">&#8250;</span>
      </button>
      {!disabled && (
        <div
          ref={submenuRef}
          role="menu"
          aria-hidden={!open}
          className={`absolute left-full top-0 bg-bb-panel border border-bb-border rounded shadow-lg py-1 min-w-[300px] z-50 ${
            open ? 'block' : 'hidden'
          }`}
          onMouseEnter={() => openSubmenu(false)}
        >
          {children}
        </div>
      )}
    </div>
  );
}

function MenuItem({
  label,
  shortcut,
  disabled,
  onClick,
}: {
  label: string;
  shortcut?: string;
  disabled?: boolean;
  onClick: (event?: ReactMouseEvent<HTMLButtonElement>) => void;
}) {
  return (
    <button
      className={`w-full text-left px-3 py-1 text-sm flex items-center justify-between ${
        disabled
          ? 'text-bb-text-dim cursor-default'
          : 'text-bb-text hover:bg-bb-hover'
      }`}
      onClick={disabled ? undefined : onClick}
      disabled={disabled}
    >
      <span>{label}</span>
      {shortcut && <span className="text-bb-text-dim text-xs ml-4">{shortcut}</span>}
    </button>
  );
}

function MenuLabel({ label }: { label: string }) {
  return (
    <div className="px-3 py-1 text-sm text-bb-text-dim cursor-default select-none">
      {label}
    </div>
  );
}

function MenuCheckItem({
  label,
  shortcut,
  checked,
  disabled,
  onClick,
}: {
  label: string;
  shortcut?: string;
  checked: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      className={`w-full text-left px-3 py-1 text-sm flex items-center justify-between ${
        disabled
          ? 'text-bb-text-dim cursor-default'
          : 'text-bb-text hover:bg-bb-hover'
      }`}
      onClick={disabled ? undefined : onClick}
      disabled={disabled}
    >
      <span>
        <span className="inline-block w-4 text-center">{checked ? CHECK_MARK : ''}</span>
        {label}
      </span>
      {shortcut && <span className="text-bb-text-dim text-xs ml-4">{shortcut}</span>}
    </button>
  );
}
