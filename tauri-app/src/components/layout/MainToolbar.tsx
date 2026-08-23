import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { useMachineStore } from '../../stores/machineStore';
import { useUiStore } from '../../stores/uiStore';
import { useUndoStore } from '../../stores/undoStore';
import { usePreviewStore } from '../../stores/previewStore';
import { useCameraStore } from '../../stores/cameraStore';
import {
  effectiveTransformLocks,
  isTransformLocked,
  notifyTransformLocked,
  notifyObjectLocked,
} from '../../utils/transformLocks';
import { zoomToFitBounds } from '../../canvas/ViewportTransform';
import { getCanvasViewportSize } from '../../canvas/canvasViewportRegistry';
import { DeviceSettingsDialog } from '../dialogs/DeviceSettingsDialog';
import { ROTARY_SETUP_OPEN_EVENT } from '../../rotaryEvents';
import { DockDialog } from '../dialogs/DockDialog';
import { IconButton } from '../shared/IconButton';
import { useMacroStore } from '../../stores/macroStore';
import {
  FilePlus, FolderOpen, Save, SaveAll, Import,
  Undo2, Redo2,
  ZoomIn, ZoomOut, Maximize2,
  Grid3x3, Magnet,
  Eye,
  Camera,
  Settings,
  FlipHorizontal2, FlipVertical2,
  Play,
  PenLine, Zap, MapPin,
} from 'lucide-react';
import {
  MirrorAcrossLineIcon,
  DockToEdgeIcon,
} from '../icons/ArrangeIcons';
import { GridSpacingControl } from './GridSpacingControl';
import { projectDisplayName } from '../../utils/windowTitle';

function Separator() {
  return <div className="w-px h-4 bg-bb-border mx-0.5" />;
}

function MacroToolbarIcon({ number }: { number: number }) {
  return (
    <span className="relative flex h-6 w-6 items-center justify-center" aria-hidden="true">
      <Play size={17} />
      <span className="absolute -bottom-1 -right-1 flex h-3.5 min-w-3.5 items-center justify-center rounded-full border border-bb-panel bg-bb-accent px-0.5 text-[9px] font-bold leading-none text-bb-on-accent tabular-nums">
        {number}
      </span>
    </span>
  );
}

const CONNECTION_DOT_COLORS: Record<string, string> = {
  disconnected: 'bg-gray-500',
  connecting: 'bg-yellow-500',
  ready: 'bg-green-500',
  alarm: 'bg-red-500',
};

/** Pill styling per connection state — quiet when idle, loud when live. */
const CONNECTION_PILL_CLASSES: Record<string, string> = {
  disconnected: 'bg-bb-surface-2 text-bb-text-muted',
  connecting: 'bg-bb-warning-bg text-bb-warning-fg',
  ready: 'bg-bb-success-bg text-bb-success-fg',
  alarm: 'bg-bb-error-bg text-bb-error-fg',
};

const FLIP_HORIZONTAL = 'horizontal' as const;
const FLIP_VERTICAL = 'vertical' as const;
const EMERGENCY_STOP_SYMBOL = '■';
const CONNECTION_SETTINGS_TAB = 'connection' as const;
const MACHINE_SETTINGS_TAB = 'machine' as const;
const TOOL_SELECT = 'select' as const;
const TOOL_LASER_POSITION = 'laser_position' as const;

export function MainToolbar() {
  const { t } = useTranslation();
  const createProject = useProjectStore((s) => s.createProject);
  const openProject = useProjectStore((s) => s.openProject);
  const saveProject = useProjectStore((s) => s.saveProject);
  const saveProjectAs = useProjectStore((s) => s.saveProjectAs);
  const importFiles = useProjectStore((s) => s.importFiles);
  const project = useProjectStore((s) => s.project);
  const projectPath = useProjectStore((s) => s.projectPath);
  const selectedLayerId = useProjectStore((s) => s.selectedLayerId);
  const selectedObjectIds = useProjectStore((s) => s.selectedObjectIds);
  const flipObjects = useProjectStore((s) => s.flipObjects);
  const mirrorAcrossLine = useProjectStore((s) => s.mirrorAcrossLine);
  const computeDockArrangementSelection = useProjectStore((s) => s.computeDockArrangementSelection);
  const computeMirrorAcrossLineSelection = useProjectStore((s) => s.computeMirrorAcrossLineSelection);

  const zoomInFn = useUiStore((s) => s.zoomIn);
  const zoomOutFn = useUiStore((s) => s.zoomOut);
  const gridVisible = useUiStore((s) => s.gridVisible);
  const snapToGrid = useUiStore((s) => s.snapToGrid);
  const toggleGrid = useUiStore((s) => s.toggleGrid);
  const toggleSnap = useUiStore((s) => s.toggleSnap);
  const zoomToFit = useUiStore((s) => s.zoomToFit);
  const toolbarVisibility = useUiStore((s) => s.panelLayout.toolbarVisibility);
  const activeTool = useUiStore((s) => s.activeTool);
  const setActiveTool = useUiStore((s) => s.setActiveTool);

  const sessionState = useMachineStore((s) => s.sessionState);
  const emergencyStop = useMachineStore((s) => s.emergencyStop);
  const activeProfileName = useMachineStore(
    (s) => (s.profiles ?? []).find((p) => p.id === s.activeProfileId)?.name ?? null,
  );
  const workspaceMode = useUiStore((s) => s.workspaceMode);
  const setWorkspaceMode = useUiStore((s) => s.setWorkspaceMode);

  const canUndo = useUndoStore((s) => s.canUndo);
  const canRedo = useUndoStore((s) => s.canRedo);
  const undo = useUndoStore((s) => s.undo);
  const redo = useUndoStore((s) => s.redo);

  const togglePreview = usePreviewStore((s) => s.togglePreview);
  const overlayVisible = useCameraStore((s) => s.overlayVisible);
  const toggleOverlayVisible = useCameraStore((s) => s.toggleOverlayVisible);

  const [showDeviceSettings, setShowDeviceSettings] = useState(false);
  const [deviceSettingsInitialTab, setDeviceSettingsInitialTab] = useState<
    typeof CONNECTION_SETTINGS_TAB | typeof MACHINE_SETTINGS_TAB
  >(CONNECTION_SETTINGS_TAB);
  const [dockDialogObjectIds, setDockDialogObjectIds] = useState<string[] | null>(null);

  const loadMacros = useMacroStore((s) => s.loadMacros);
  const toolbarMacros = useMacroStore((s) => s.macros).filter((m) => m.show_in_toolbar);
  const runMacro = useMacroStore((s) => s.runMacro);

  useEffect(() => {
    void loadMacros();
  }, [loadMacros]);

  useEffect(() => {
    const openRotary = () => {
      setDeviceSettingsInitialTab(MACHINE_SETTINGS_TAB);
      setShowDeviceSettings(true);
    };
    window.addEventListener(ROTARY_SETUP_OPEN_EVENT, openRotary);
    return () => window.removeEventListener(ROTARY_SETUP_OPEN_EVENT, openRotary);
  }, []);

  const hasSelection = selectedObjectIds.length > 0;
  const selectedObjects = project?.objects.filter((o) => selectedObjectIds.includes(o.id)) ?? [];
  const anyLocked = selectedObjects.some((o) => o.locked);
  const canMutate = hasSelection && !anyLocked;
  const arrangementSelection = computeDockArrangementSelection();
  const mirrorAcrossLineSelection = computeMirrorAcrossLineSelection();
  const canDock = arrangementSelection.length >= 1 && !anyLocked;
  const canMirrorAcrossLine = mirrorAcrossLineSelection.length >= 2 && !anyLocked;

  const blockTransform = (kind: 'position' | 'scale' | 'rotation') => {
    const locks = effectiveTransformLocks(selectedObjects);
    if (isTransformLocked(locks, kind)) {
      notifyTransformLocked(kind);
      return true;
    }
    return false;
  };

  const handleFlip = (direction: 'horizontal' | 'vertical') => {
    if (anyLocked) { notifyObjectLocked(); return; }
    if (blockTransform('position')) return;
    void flipObjects(selectedObjectIds, direction);
  };

  const handleMirrorAcrossLine = async () => {
    if (anyLocked) { notifyObjectLocked(); return; }
    await mirrorAcrossLine();
  };

  const handleOpenDockDialog = () => {
    if (blockTransform('position')) return;
    if (arrangementSelection.length === 0) return;
    setDockDialogObjectIds(arrangementSelection);
  };

  const handleImport = () => {
    if (!project) return;
    const layerId = selectedLayerId ?? project.layers[0]?.id ?? '';
    importFiles(layerId);
  };

  const handleZoomToPage = () => {
    if (!project) return;
    const { bed_width_mm, bed_height_mm } = project.workspace;
    const size = getCanvasViewportSize();
    if (!size) return;
    const result = zoomToFitBounds(
      { min: { x: 0, y: 0 }, max: { x: bed_width_mm, y: bed_height_mm } },
      size.width,
      size.height,
    );
    zoomToFit(result.offset, result.zoom);
  };

  const sz = 20;
  const showMain = toolbarVisibility.main;
  const showMirror = toolbarVisibility.arrange || toolbarVisibility.arrangeLong;
  const showDocking = toolbarVisibility.docking;

  if (!showMain && workspaceMode !== 'design') {
    return null;
  }

  const normalizedSessionState = sessionState ?? 'disconnected';
  const connectionDot = CONNECTION_DOT_COLORS[normalizedSessionState] ?? 'bg-gray-500';
  const connectionLabel = t(`status.connection.${normalizedSessionState}`, {
    defaultValue:
      normalizedSessionState.charAt(0).toUpperCase() + normalizedSessionState.slice(1),
  });

  return (
    <div className="no-select relative flex items-center h-11 bg-bb-panel px-3 gap-0.5 text-xs border-b border-bb-border">
      {/* Brand + project identity */}
      <span
        aria-hidden="true"
        className="flex h-6 w-6 flex-shrink-0 items-center justify-center rounded-lg bg-bb-accent text-[13px] font-extrabold text-bb-on-accent"
      >
        B
      </span>
      <span className="mx-2 max-w-64 truncate text-xs font-medium text-bb-text">
        {projectDisplayName(projectPath, project?.metadata.project_name ?? null) ??
          t('toolbars.main.untitled_project')}
        {project?.dirty ? (
          <span className="text-bb-accent ml-1" title={t('status.unsaved_changes')}>*</span>
        ) : null}
      </span>
      {/* Design | Run workspace switch — the app's primary navigation */}
      <div
        className="mx-1.5 flex flex-shrink-0 rounded-xl border border-bb-accent/40 bg-bb-surface-2 p-0.5 shadow-[0_0_10px_rgba(45,212,222,0.15)]"
        data-testid="mode-switch"
      >
        <button
          className={`flex items-center gap-1.5 rounded-lg px-4 py-1 text-sm ${
            workspaceMode === 'design'
              ? 'bg-bb-accent font-bold text-bb-on-accent shadow'
              : 'text-bb-text-muted hover:bg-bb-hover hover:text-bb-text'
          }`}
          onClick={() => setWorkspaceMode('design')}
          title={t('toolbars.main.mode_design_hint')}
          data-testid="mode-design"
        >
          <PenLine size={14} />
          {t('toolbars.main.mode_design')}
        </button>
        <button
          className={`flex items-center gap-1.5 rounded-lg px-4 py-1 text-sm ${
            workspaceMode === 'run'
              ? 'bg-bb-accent font-bold text-bb-on-accent shadow'
              : 'text-bb-text-muted hover:bg-bb-hover hover:text-bb-text'
          }`}
          onClick={() => setWorkspaceMode('run')}
          title={t('toolbars.main.mode_run_hint')}
          data-testid="mode-run"
        >
          <Zap size={14} />
          {t('toolbars.main.mode_run')}
        </button>
      </div>
      <Separator />
      {showMain && (
        <>
      {/* File group */}
      <IconButton icon={<FilePlus size={sz} />} label={t('toolbars.main.new')} onClick={() => createProject(t('toolbars.main.untitled_project'))} />
      <IconButton icon={<FolderOpen size={sz} />} label={t('toolbars.main.open')} onClick={() => void openProject()} />
      <IconButton icon={<Save size={sz} />} label={t('toolbars.main.save')} onClick={() => void saveProject()} disabled={!project} />
      <IconButton icon={<SaveAll size={sz} />} label={t('toolbars.main.save_as')} onClick={() => void saveProjectAs()} disabled={!project} />
      <IconButton icon={<Import size={sz} />} label={t('toolbars.main.import')} onClick={handleImport} disabled={!project || workspaceMode === 'run'} />
      <Separator />

      {/* Undo/Redo */}
      <IconButton icon={<Undo2 size={sz} />} label={t('toolbars.main.undo')} onClick={() => void undo()} disabled={!canUndo} />
      <IconButton icon={<Redo2 size={sz} />} label={t('toolbars.main.redo')} onClick={() => void redo()} disabled={!canRedo} />
      <Separator />

      {/* Zoom group */}
      <IconButton icon={<ZoomOut size={sz} />} label={t('toolbars.main.zoom_out')} onClick={zoomOutFn} />
      <IconButton icon={<ZoomIn size={sz} />} label={t('toolbars.main.zoom_in')} onClick={zoomInFn} />
      <IconButton
        icon={<Maximize2 size={sz} />}
        label={t('toolbars.main.fit_page')}
        onClick={handleZoomToPage}
        disabled={!project}
      />
      <Separator />

      {/* Grid/Snap */}
      <div className="flex flex-shrink-0 items-center gap-0.5">
        <IconButton icon={<Grid3x3 size={sz} />} label={t('toolbars.main.grid')} onClick={toggleGrid} active={gridVisible} />
        <GridSpacingControl label={t('dialog.settings.grid_spacing')} />
        <IconButton icon={<Magnet size={sz} />} label={t('toolbars.main.snap')} onClick={toggleSnap} active={snapToGrid} />
      </div>
      <Separator />

      {/* Preview */}
      <IconButton icon={<Eye size={sz} />} label={t('toolbars.main.preview')} onClick={() => void togglePreview()} disabled={!project} />
      <IconButton
        icon={<Camera size={sz} />}
        label={t('toolbars.main.camera_overlay')}
        onClick={toggleOverlayVisible}
        active={overlayVisible}
      />
      <Separator />

      {/* Settings */}
      <IconButton icon={<Settings size={sz} />} label={t('toolbars.main.device_settings')} onClick={() => {
        setDeviceSettingsInitialTab(CONNECTION_SETTINGS_TAB);
        setShowDeviceSettings(true);
      }} />
        </>
      )}

      {workspaceMode === 'design' && (
        <>
          <Separator />
          <IconButton icon={<FlipHorizontal2 size={sz} />} label={t('toolbars.main.flip_horizontal')} onClick={() => handleFlip(FLIP_HORIZONTAL)} disabled={!canMutate} />
          <IconButton icon={<FlipVertical2 size={sz} />} label={t('toolbars.main.flip_vertical')} onClick={() => handleFlip(FLIP_VERTICAL)} disabled={!canMutate} />
          {showMirror && (
            <IconButton icon={<MirrorAcrossLineIcon size={sz} />} label={t('toolbars.main.mirror_across_line')} onClick={() => void handleMirrorAcrossLine()} disabled={!canMirrorAcrossLine} />
          )}
          {showDocking && (
            <IconButton icon={<DockToEdgeIcon size={sz} />} label={t('toolbars.main.dock')} onClick={handleOpenDockDialog} disabled={!canDock} />
          )}
          <div className="w-3" />
        </>
      )}

      {workspaceMode === 'run' && (
        <>
          <Separator />
          <IconButton
            icon={<MapPin size={sz} />}
            label={t('toolbars.creation.laser_position')}
            onClick={() => setActiveTool(
              activeTool === TOOL_LASER_POSITION ? TOOL_SELECT : TOOL_LASER_POSITION,
            )}
            active={activeTool === TOOL_LASER_POSITION}
          />
        </>
      )}

      {showMain && toolbarMacros.length > 0 && (
        <>
          <Separator />
          {toolbarMacros.map((macro, index) => {
            const number = index + 1;
            return (
              <IconButton
                key={macro.id}
                icon={<MacroToolbarIcon number={number} />}
                label={`${number}. ${macro.name}`}
                onClick={() => void runMacro(macro.id)}
                data-testid={`toolbar-macro-${macro.id}`}
              />
            );
          })}
        </>
      )}

      {/* Right side: emergency stop (whenever a machine session exists) + connection pill */}
      <div className="flex-1" />
      {normalizedSessionState !== 'disconnected' && (
        <button
          className="mr-1 flex flex-shrink-0 items-center gap-1 rounded-lg bg-bb-error px-3 py-1 text-xs font-bold text-bb-on-error hover:bg-bb-error-hover"
          onClick={() => void emergencyStop()}
          data-testid="toolbar-emergency-stop"
        >
          {EMERGENCY_STOP_SYMBOL} {t('panels.machine.laser.stop')}
        </button>
      )}
      <button
        className={`flex flex-shrink-0 items-center gap-1.5 rounded-full px-3 py-1 text-xxs hover:brightness-110 ${
          CONNECTION_PILL_CLASSES[normalizedSessionState] ?? CONNECTION_PILL_CLASSES.disconnected
        }`}
        onClick={() => setWorkspaceMode('run')}
        title={t('toolbars.main.mode_run_hint')}
        data-testid="connection-pill"
      >
        <span className={`h-2 w-2 rounded-full ${connectionDot}`} />
        <span>
          {connectionLabel}
          {normalizedSessionState !== 'disconnected' && activeProfileName ? ` · ${activeProfileName}` : ''}
        </span>
      </button>

      {showDeviceSettings && (
        <DeviceSettingsDialog
          initialTab={deviceSettingsInitialTab}
          onClose={() => setShowDeviceSettings(false)}
        />
      )}
      {dockDialogObjectIds && (
        <DockDialog objectIds={dockDialogObjectIds} onClose={() => setDockDialogObjectIds(null)} />
      )}
    </div>
  );
}
