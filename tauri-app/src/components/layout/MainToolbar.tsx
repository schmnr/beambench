import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { AlignmentType, DistributionDirection } from '../../types/project';
import { useProjectStore } from '../../stores/projectStore';
import { useMachineStore } from '../../stores/machineStore';
import { useUiStore } from '../../stores/uiStore';
import { useUndoStore } from '../../stores/undoStore';
import { usePreviewStore } from '../../stores/previewStore';
import { useCameraStore } from '../../stores/cameraStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { wrapBackendError } from '../../i18n/errors';
import { projectService } from '../../services/projectService';
import { isTransformLocked, notifyTransformLocked, notifyObjectLocked } from '../../utils/transformLocks';
import { zoomToFitBounds } from '../../canvas/ViewportTransform';
import { computeVisualBoundsWorld } from '../../canvas/alignment';
import { getCanvasViewportSize } from '../../canvas/canvasViewportRegistry';
import { DeviceSettingsDialog } from '../dialogs/DeviceSettingsDialog';
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
  Group, Ungroup, FlipHorizontal2, FlipVertical2,
  AlignStartVertical, AlignEndVertical,
  AlignStartHorizontal, AlignEndHorizontal,
  AlignCenterHorizontal, AlignCenterVertical,
  AlignHorizontalSpaceAround, AlignVerticalSpaceAround,
  Crosshair,
  Play,
} from 'lucide-react';
import {
  MirrorAcrossLineIcon,
  MakeSameWidthIcon,
  MakeSameHeightIcon,
  MoveHorizontallyTogetherIcon,
  MoveVerticallyTogetherIcon,
  DockToEdgeIcon,
} from '../icons/ArrangeIcons';

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

const FLIP_HORIZONTAL = 'horizontal' as const;
const FLIP_VERTICAL = 'vertical' as const;
const ALIGN_LEFT = 'left' as const;
const ALIGN_RIGHT = 'right' as const;
const ALIGN_TOP = 'top' as const;
const ALIGN_BOTTOM = 'bottom' as const;
const ALIGN_VERTICAL_CENTERS = 'centers_v' as const;
const ALIGN_HORIZONTAL_CENTERS = 'centers_h' as const;
const DISTRIBUTE_H_CENTERED = 'h_centered' as const;
const DISTRIBUTE_V_CENTERED = 'v_centered' as const;
const SIZE_WIDTH = 'width' as const;
const SIZE_HEIGHT = 'height' as const;

export function MainToolbar() {
  const { t } = useTranslation();
  const createProject = useProjectStore((s) => s.createProject);
  const openProject = useProjectStore((s) => s.openProject);
  const saveProject = useProjectStore((s) => s.saveProject);
  const saveProjectAs = useProjectStore((s) => s.saveProjectAs);
  const importFiles = useProjectStore((s) => s.importFiles);
  const project = useProjectStore((s) => s.project);
  const selectedLayerId = useProjectStore((s) => s.selectedLayerId);
  const selectedObjectIds = useProjectStore((s) => s.selectedObjectIds);
  const groupObjects = useProjectStore((s) => s.groupObjects);
  const ungroupObjects = useProjectStore((s) => s.ungroupObjects);
  const flipObjects = useProjectStore((s) => s.flipObjects);
  const moveObjectsTo = useProjectStore((s) => s.moveObjectsTo);
  const moveObjectsTogether = useProjectStore((s) => s.moveObjectsTogether);
  const mirrorAcrossLine = useProjectStore((s) => s.mirrorAcrossLine);
  const makeSameSize = useProjectStore((s) => s.makeSameSize);
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

  const sessionState = useMachineStore((s) => s.sessionState);

  const canUndo = useUndoStore((s) => s.canUndo);
  const canRedo = useUndoStore((s) => s.canRedo);
  const undo = useUndoStore((s) => s.undo);
  const redo = useUndoStore((s) => s.redo);

  const togglePreview = usePreviewStore((s) => s.togglePreview);
  const overlayVisible = useCameraStore((s) => s.overlayVisible);
  const toggleOverlayVisible = useCameraStore((s) => s.toggleOverlayVisible);

  const [showDeviceSettings, setShowDeviceSettings] = useState(false);
  const [showZoomMenu, setShowZoomMenu] = useState(false);
  const [dockDialogObjectIds, setDockDialogObjectIds] = useState<string[] | null>(null);

  const loadMacros = useMacroStore((s) => s.loadMacros);
  const toolbarMacros = useMacroStore((s) => s.macros).filter((m) => m.show_in_toolbar);
  const runMacro = useMacroStore((s) => s.runMacro);

  useEffect(() => {
    void loadMacros();
  }, [loadMacros]);

  const hasSelection = selectedObjectIds.length > 0;
  const selCount = selectedObjectIds.length;
  const selectedObjects = project?.objects.filter((o) => selectedObjectIds.includes(o.id)) ?? [];
  const anyLocked = selectedObjects.some((o) => o.locked);
  const canMutate = hasSelection && !anyLocked;
  const singleSelected = selectedObjects.length === 1 ? selectedObjects[0] : null;
  const canGroup = selCount >= 2 && !anyLocked;
  const canUngroup = selCount === 1 && !anyLocked && singleSelected?.data.type === 'group';
  const canAlign = selCount >= 2 && !anyLocked;
  const canDistribute = selCount >= 3 && !anyLocked;
  const arrangementSelection = computeDockArrangementSelection();
  const mirrorAcrossLineSelection = computeMirrorAcrossLineSelection();
  const canMoveTogether = arrangementSelection.length >= 2 && !anyLocked;
  const canDock = arrangementSelection.length >= 1 && !anyLocked;
  const canMirrorAcrossLine = mirrorAcrossLineSelection.length >= 2 && !anyLocked;
  const canMakeSameSize = arrangementSelection.length >= 2 && !anyLocked;

  const blockTransform = (kind: 'position' | 'scale' | 'rotation') => {
    const locks = useProjectStore.getState().project?.transform_locks;
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

  const handleAlign = async (alignmentType: AlignmentType) => {
    if (selectedObjectIds.length < 2) return;
    if (anyLocked) { notifyObjectLocked(); return; }
    if (blockTransform('position')) return;
    try {
      const updatedObjects = await projectService.alignObjects(selectedObjectIds, alignmentType);
      const project = useProjectStore.getState().project;
      if (project) {
        const updatedMap = new Map(updatedObjects.map((o) => [o.id, o]));
        useProjectStore.setState({
          project: {
            ...project,
            objects: project.objects.map((o) => updatedMap.get(o.id) ?? o),
            dirty: true,
          },
        });
        usePreviewStore.getState().invalidate();
        await useUndoStore.getState().refresh();
      }
    } catch (error) {
      useNotificationStore.getState().push(wrapBackendError(String(error)), 'error');
    }
  };

  const handleDistribute = async (direction: DistributionDirection) => {
    if (selectedObjectIds.length < 3) return;
    if (anyLocked) { notifyObjectLocked(); return; }
    if (blockTransform('position')) return;
    try {
      const updatedObjects = await projectService.distributeObjects(selectedObjectIds, direction);
      const project = useProjectStore.getState().project;
      if (project) {
        const updatedMap = new Map(updatedObjects.map((o) => [o.id, o]));
        useProjectStore.setState({
          project: {
            ...project,
            objects: project.objects.map((o) => updatedMap.get(o.id) ?? o),
            dirty: true,
          },
        });
        usePreviewStore.getState().invalidate();
        await useUndoStore.getState().refresh();
      }
    } catch (error) {
      useNotificationStore.getState().push(wrapBackendError(String(error)), 'error');
    }
  };

  const handleMoveTogether = async (axis: 'horizontal' | 'vertical') => {
    await moveObjectsTogether(axis);
  };

  const handleMirrorAcrossLine = async () => {
    if (anyLocked) { notifyObjectLocked(); return; }
    await mirrorAcrossLine();
  };

  const handleMakeSameSize = async (axis: 'width' | 'height', preserveAspect: boolean) => {
    await makeSameSize(axis, preserveAspect);
  };

  const handleOpenDockDialog = () => {
    if (blockTransform('position')) return;
    if (arrangementSelection.length === 0) return;
    setDockDialogObjectIds(arrangementSelection);
  };

  const handleCenterOnPage = async () => {
    if (selectedObjectIds.length < 1 || !project) return;
    if (anyLocked) { notifyObjectLocked(); return; }
    if (blockTransform('position')) return;
    const cx = project.workspace.bed_width_mm / 2;
    const cy = project.workspace.bed_height_mm / 2;
    await moveObjectsTo(selectedObjectIds, cx, cy);
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

  const handleZoomToSelection = () => {
    if (!project || selectedObjectIds.length === 0) return;
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const id of selectedObjectIds) {
      const obj = project.objects.find((o) => o.id === id);
      if (obj) {
        // Transform-aware bounds: raw obj.bounds ignores rotation/shear and
        // zooms to the wrong region for rotated objects (matches drawSelection).
        const vb = computeVisualBoundsWorld(obj, project.objects);
        minX = Math.min(minX, vb.min.x);
        minY = Math.min(minY, vb.min.y);
        maxX = Math.max(maxX, vb.max.x);
        maxY = Math.max(maxY, vb.max.y);
      }
    }
    if (!isFinite(minX)) return;
    const size = getCanvasViewportSize();
    if (!size) return;
    const result = zoomToFitBounds({ min: { x: minX, y: minY }, max: { x: maxX, y: maxY } }, size.width, size.height);
    zoomToFit(result.offset, result.zoom);
  };

  const sz = 20;
  const showMain = toolbarVisibility.main;
  const showArrange = toolbarVisibility.arrange;
  const showArrangeLong = toolbarVisibility.arrangeLong;
  const showDocking = toolbarVisibility.docking;

  if (!showMain && !showArrange && !showArrangeLong && !showDocking) {
    return null;
  }

  const normalizedSessionState = sessionState ?? 'disconnected';
  const connectionDot = CONNECTION_DOT_COLORS[normalizedSessionState] ?? 'bg-gray-500';
  const connectionLabel = t(`status.connection.${normalizedSessionState}`, {
    defaultValue:
      normalizedSessionState.charAt(0).toUpperCase() + normalizedSessionState.slice(1),
  });

  return (
    <div className="no-select flex items-center h-11 bg-bb-panel px-3 gap-0.5 text-xs border-b border-bb-border">
      {/* Brand + project identity */}
      <span
        aria-hidden="true"
        className="flex h-6 w-6 flex-shrink-0 items-center justify-center rounded-lg bg-bb-accent text-[13px] font-extrabold text-bb-on-accent"
      >
        B
      </span>
      <span className="mx-2 max-w-48 truncate text-xs font-medium text-bb-text">
        {project?.metadata.project_name ?? t('toolbars.main.untitled_project')}
        {project?.dirty ? (
          <span className="text-bb-accent ml-1" title={t('status.unsaved_changes')}>*</span>
        ) : null}
      </span>
      <Separator />
      {showMain && (
        <>
      {/* File group */}
      <IconButton icon={<FilePlus size={sz} />} label={t('toolbars.main.new')} onClick={() => createProject(t('toolbars.main.untitled_project'))} />
      <IconButton icon={<FolderOpen size={sz} />} label={t('toolbars.main.open')} onClick={() => void openProject()} />
      <IconButton icon={<Save size={sz} />} label={t('toolbars.main.save')} onClick={() => void saveProject()} disabled={!project} />
      <IconButton icon={<SaveAll size={sz} />} label={t('toolbars.main.save_as')} onClick={() => void saveProjectAs()} disabled={!project} />
      <IconButton icon={<Import size={sz} />} label={t('toolbars.main.import')} onClick={handleImport} disabled={!project} />
      <Separator />

      {/* Undo/Redo */}
      <IconButton icon={<Undo2 size={sz} />} label={t('toolbars.main.undo')} onClick={() => void undo()} disabled={!canUndo} />
      <IconButton icon={<Redo2 size={sz} />} label={t('toolbars.main.redo')} onClick={() => void redo()} disabled={!canRedo} />
      <Separator />

      {/* Zoom group */}
      <IconButton icon={<ZoomOut size={sz} />} label={t('toolbars.main.zoom_out')} onClick={zoomOutFn} />
      <IconButton icon={<ZoomIn size={sz} />} label={t('toolbars.main.zoom_in')} onClick={zoomInFn} />
      <div className="relative">
        <IconButton
          icon={<Maximize2 size={sz} />}
          label={t('status.zoom_to_fit')}
          onClick={() => setShowZoomMenu((v) => !v)}
          active={showZoomMenu}
        />
        {showZoomMenu && (
          <>
            <div className="fixed inset-0 z-40" onClick={() => setShowZoomMenu(false)} />
            <div className="absolute left-0 top-full z-50 mt-1 min-w-44 rounded-lg border border-bb-border bg-bb-panel py-1 shadow-lg">
              <button
                className="block w-full px-3 py-1.5 text-left text-xs text-bb-text hover:bg-bb-hover disabled:text-bb-text-disabled disabled:hover:bg-transparent"
                onClick={() => { handleZoomToPage(); setShowZoomMenu(false); }}
                disabled={!project}
              >
                {t('toolbars.main.fit_page')}
              </button>
              <button
                className="block w-full px-3 py-1.5 text-left text-xs text-bb-text hover:bg-bb-hover disabled:text-bb-text-disabled disabled:hover:bg-transparent"
                onClick={() => { handleZoomToSelection(); setShowZoomMenu(false); }}
                disabled={!hasSelection}
              >
                {t('toolbars.main.fit_selection')}
              </button>
            </div>
          </>
        )}
      </div>
      <Separator />

      {/* Grid/Snap */}
      <IconButton icon={<Grid3x3 size={sz} />} label={t('toolbars.main.grid')} onClick={toggleGrid} active={gridVisible} />
      <IconButton icon={<Magnet size={sz} />} label={t('toolbars.main.snap')} onClick={toggleSnap} active={snapToGrid} />
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
      <IconButton icon={<Settings size={sz} />} label={t('toolbars.main.device_settings')} onClick={() => setShowDeviceSettings(true)} />
      <div className="w-3" />
        </>
      )}

      {showArrange && (
        <>
      {/* Group/Ungroup */}
      <IconButton icon={<Group size={sz} />} label={t('toolbars.main.group')} onClick={() => void groupObjects(selectedObjectIds)} disabled={!canGroup} />
      <IconButton icon={<Ungroup size={sz} />} label={t('toolbars.main.ungroup')} onClick={() => void ungroupObjects(selectedObjectIds[0])} disabled={!canUngroup} />
      <Separator />

      {/* Flip */}
      <IconButton icon={<FlipHorizontal2 size={sz} />} label={t('toolbars.main.flip_horizontal')} onClick={() => handleFlip(FLIP_HORIZONTAL)} disabled={!canMutate} />
      <IconButton icon={<FlipVertical2 size={sz} />} label={t('toolbars.main.flip_vertical')} onClick={() => handleFlip(FLIP_VERTICAL)} disabled={!canMutate} />
      <IconButton icon={<MirrorAcrossLineIcon size={sz} />} label={t('toolbars.main.mirror_across_line')} onClick={() => void handleMirrorAcrossLine()} disabled={!canMirrorAcrossLine} />
      <Separator />

      {/* Align */}
      <IconButton icon={<AlignStartVertical size={sz} />} label={t('toolbars.main.align_left')} onClick={() => void handleAlign(ALIGN_LEFT)} disabled={!canAlign} />
      <IconButton icon={<AlignEndVertical size={sz} />} label={t('toolbars.main.align_right')} onClick={() => void handleAlign(ALIGN_RIGHT)} disabled={!canAlign} />
      <IconButton icon={<AlignStartHorizontal size={sz} />} label={t('toolbars.main.align_top')} onClick={() => void handleAlign(ALIGN_TOP)} disabled={!canAlign} />
      <IconButton icon={<AlignEndHorizontal size={sz} />} label={t('toolbars.main.align_bottom')} onClick={() => void handleAlign(ALIGN_BOTTOM)} disabled={!canAlign} />
      <IconButton icon={<AlignCenterHorizontal size={sz} />} label={t('toolbars.main.align_vertical_centers')} onClick={() => void handleAlign(ALIGN_VERTICAL_CENTERS)} disabled={!canAlign} />
      <IconButton icon={<AlignCenterVertical size={sz} />} label={t('toolbars.main.align_horizontal_centers')} onClick={() => void handleAlign(ALIGN_HORIZONTAL_CENTERS)} disabled={!canAlign} />
      <Separator />
        </>
      )}

      {showArrangeLong && (
        <>
      {/* Distribute */}
      <IconButton icon={<AlignHorizontalSpaceAround size={sz} />} label={t('toolbars.main.distribute_h_centered')} onClick={() => void handleDistribute(DISTRIBUTE_H_CENTERED)} disabled={!canDistribute} />
      <IconButton icon={<AlignVerticalSpaceAround size={sz} />} label={t('toolbars.main.distribute_v_centered')} onClick={() => void handleDistribute(DISTRIBUTE_V_CENTERED)} disabled={!canDistribute} />
      <IconButton icon={<MakeSameWidthIcon size={sz} />} label={t('toolbars.main.make_same_width')} onClick={(event) => void handleMakeSameSize(SIZE_WIDTH, Boolean(event?.shiftKey))} disabled={!canMakeSameSize} />
      <IconButton icon={<MakeSameHeightIcon size={sz} />} label={t('toolbars.main.make_same_height')} onClick={(event) => void handleMakeSameSize(SIZE_HEIGHT, Boolean(event?.shiftKey))} disabled={!canMakeSameSize} />
      <IconButton icon={<MoveHorizontallyTogetherIcon size={sz} />} label={t('toolbars.main.move_h_together')} onClick={() => void handleMoveTogether(FLIP_HORIZONTAL)} disabled={!canMoveTogether} />
      <IconButton icon={<MoveVerticallyTogetherIcon size={sz} />} label={t('toolbars.main.move_v_together')} onClick={() => void handleMoveTogether(FLIP_VERTICAL)} disabled={!canMoveTogether} />
      <Separator />

      {/* Center on Page */}
      <IconButton icon={<Crosshair size={sz} />} label={t('toolbars.main.center_on_page')} onClick={() => void handleCenterOnPage()} disabled={!canMutate} />
        </>
      )}

      {showDocking && (
        <>
          <IconButton icon={<DockToEdgeIcon size={sz} />} label={t('toolbars.main.dock')} onClick={handleOpenDockDialog} disabled={!canDock} />
          <Separator />
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

      {/* Right side: machine connection pill */}
      <div className="flex-1" />
      <span className="flex flex-shrink-0 items-center gap-1.5 rounded-full bg-bb-surface-2 px-3 py-1 text-xxs text-bb-text-muted">
        <span className={`h-2 w-2 rounded-full ${connectionDot}`} />
        <span>{connectionLabel}</span>
      </span>

      {showDeviceSettings && (
        <DeviceSettingsDialog onClose={() => setShowDeviceSettings(false)} />
      )}
      {dockDialogObjectIds && (
        <DockDialog objectIds={dockDialogObjectIds} onClose={() => setDockDialogObjectIds(null)} />
      )}
    </div>
  );
}
