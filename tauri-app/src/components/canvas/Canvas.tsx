import { useRef, useEffect, useCallback, useMemo, useState, type DragEvent as ReactDragEvent } from 'react';
import { useProjectStore } from '../../stores/projectStore';
import { renderOptionsFromArtworkDisplayMode, useUiStore, type ToolType } from '../../stores/uiStore';
import { usePreviewStore } from '../../stores/previewStore';
import { useUndoStore } from '../../stores/undoStore';
import { useAppStore } from '../../stores/appStore';
import { useCameraStore } from '../../stores/cameraStore';
import { useMeasurementStore } from '../../stores/measurementStore';
import { vectorService } from '../../services/vectorService';
import { remapPersistentTabMarker, type PersistentTabMarkerSnapshot } from '../../canvas/tabMarkerPreview';
import { projectService } from '../../services/projectService';
import { cameraFrameAssetUrl, verifyCameraFrameTempScope } from '../../services/cameraFrameAsset';
import { registerCanvasScreenshotProvider } from '../../services/canvasScreenshotExportService';
import { useCanvasSize } from '../../hooks/useCanvasSize';
import { clearCanvasViewportSize, setCanvasViewportSize } from '../../canvas/canvasViewportRegistry';
import { CanvasRenderer, type CanvasInteractionState } from '../../canvas/CanvasRenderer';
import type { CameraOverlayHitTarget } from '../../canvas/cameraOverlay';
import {
  hitTestCameraOverlayControls,
  rotateCameraOverlayTransform,
  scaleCameraOverlayTransform,
  translateCameraOverlayTransform,
} from '../../canvas/cameraOverlay';
import type { ViewportParams } from '../../canvas/ViewportTransform';
import {
  screenToWorld,
  screenToWorldDist,
  worldToScreen,
  zoomToFitBounds,
} from '../../canvas/ViewportTransform';
import { chooseInchInterval } from '../../canvas/drawWorkspace';
import type { CanvasTool, CanvasMouseEvent, ToolContext } from '../../canvas/tools/types';
import { SelectTool } from '../../canvas/tools/SelectTool';
import { RectTool } from '../../canvas/tools/RectTool';
import { EllipseTool } from '../../canvas/tools/EllipseTool';
import { TextTool } from '../../canvas/tools/TextTool';
import {
  NodeTool,
  type NodeEditAction,
  type NodeImmediateAction,
} from '../../canvas/tools/NodeTool';
import { PenTool } from '../../canvas/tools/PenTool';
import { PolygonTool } from '../../canvas/tools/PolygonTool';
import { StarTool } from '../../canvas/tools/StarTool';
import { TrimTool } from '../../canvas/tools/TrimTool';
import { TabTool } from '../../canvas/tools/TabTool';
import { RadiusTool } from '../../canvas/tools/RadiusTool';
import { MeasureTool } from '../../canvas/tools/MeasureTool';
import { LaserPositionTool } from '../../canvas/tools/LaserPositionTool';
import { TwoPointRotateScaleTool } from '../../canvas/tools/TwoPointRotateScaleTool';
import { WarpTool } from '../../canvas/tools/SelectionMeshDeformTool';
import { useMachineStore } from '../../stores/machineStore';
import { hitTestPoint } from '../../canvas/hitTest';
import { useNotificationStore } from '../../stores/notificationStore';
import { wrapBackendError } from '../../i18n/errors';
import { useArtLibraryStore } from '../../stores/artLibraryStore';
import { ZOOM_FACTOR, MIN_ZOOM, MAX_ZOOM, DARK_THEME, LIGHT_THEME, RULER_SIZE } from '../../canvas/constants';
import { ContextMenu } from '../shared/ContextMenu';
import { useCanvasContextMenu } from './useCanvasContextMenu';
import { TextEditOverlay } from './TextEditOverlay';
import { SelectionPickerPopover } from './SelectionPickerPopover';
import { SelectionIsolationBreadcrumb } from './SelectionIsolationBreadcrumb';
import { cancelPendingGuidePathSelection } from './guidePathCancel';
import { exitStartPointPickMode } from './startPointPick';
import {
  buildCanvasArtLibraryDragOverState,
  buildCanvasArtLibraryDropPayload,
  getCanvasArtLibraryDropEffect,
  resolveCanvasArtLibraryDragState,
} from './artLibraryCanvasDrop';
import { resolveRulerGuideDropValue } from './rulerGuideDrag';
import { isArtLibraryDragDataTransfer } from '../shared/artLibraryDragData';
import { TraceImageDialog } from '../dialogs/TraceImageDialog';
import { AdjustImageDialog } from '../dialogs/AdjustImageDialog';
import { commitPendingTextEdit, getPendingContent, updatePendingContent } from '../../canvas/textEditSession';
import { beginTextEditFromDoubleClick } from '../../canvas/textDoubleClick';
import { registerToolInstances } from '../layout/CreationToolbar';
import {
  resolveWorkspaceCanvasTool,
  tracksObjectDragInteraction,
} from '../../canvas/workspaceCanvasTool';
import type { StartPointMode } from '../../types/vector';
import { resolveCanvasPointerSnap } from '../../canvas/pointerSnap';
import {
  getVectorPathCommandCount,
  getVectorPathCommandCountForObject,
} from '../../canvas/drawObjects';
import { adaptiveMeshPreviewFrameInterval } from '../../canvas/meshDeformPreview';
import { shouldAnimateSelectionDashes } from '../../canvas/selectionDashAnimation';
import { prepareCanvasSurface, presentCanvasBuffer } from '../../canvas/canvasSurface';
import { offsetPreviewTranslation } from '../../canvas/offsetPreview';
import { machineToCanvasPoint } from '../../utils/workspaceCoordinates';
import type { SimilarityTransform } from '../../types/camera';
import type { Point2D } from '../../types/project';
import { isEffectiveVector } from '../../commands/selectionContext';
import { effectiveTransformLocks } from '../../utils/transformLocks';

const nodeTool = new NodeTool();

const TOOL_INSTANCES: Record<ToolType, CanvasTool> = {
  select: new SelectTool(),
  rect: new RectTool(),
  ellipse: new EllipseTool(),
  star: new StarTool(),
  text: new TextTool(),
  node: nodeTool,
  line: new PenTool(),
  polygon: new PolygonTool(),
  trim: new TrimTool(),
  tabs: new TabTool(),
  radius: new RadiusTool(),
  measure: new MeasureTool(),
  laser_position: new LaserPositionTool(),
  two_point_rotate_scale: new TwoPointRotateScaleTool(),
  warp: new WarpTool(),
};

// Register TOOL_INSTANCES so CreationToolbar can configure polygon sides / star dualRadius
registerToolInstances(TOOL_INSTANCES as unknown as Record<string, unknown>);

const INTERACTION_IDLE_MS = 150;
const POINTER_DRAG_INTERACTION_THRESHOLD_PX = 4;
const HEAVY_SELECTION_COMMAND_THRESHOLD = 2000;
const RUN_MODE_SELECTION_LOCKS = {
  move_enabled: false,
  size_enabled: false,
  rotate_enabled: false,
  shear_enabled: false,
} as const;

type MouseEventLike = {
  clientX: number;
  clientY: number;
  button: number;
  buttons?: number;
  shiftKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey?: boolean;
};

type CameraOverlayAdjustDrag = {
  target: CameraOverlayHitTarget;
  startScreen: Point2D;
  startTransform: SimilarityTransform;
  preDragDirty: boolean;
};

type SelectionPickerState = { x: number; y: number; objectIds: string[] } | null;

export function Canvas() {
  const containerRef = useRef<HTMLDivElement>(null);
  const baseCanvasRef = useRef<HTMLCanvasElement>(null);
  const overlayCanvasRef = useRef<HTMLCanvasElement>(null);
  const overlayBufferRef = useRef<HTMLCanvasElement | null>(null);
  const baseRendererRef = useRef<CanvasRenderer | null>(null);
  const overlayRendererRef = useRef<CanvasRenderer | null>(null);
  const sceneRafRef = useRef<number>(0);
  const overlayRafRef = useRef<number>(0);
  const overlayLastRenderAtRef = useRef(0);
  const meshPreviewRenderAverageRef = useRef(0);
  const meshPreviewFrameIntervalRef = useRef(0);
  const pointerMoveRafRef = useRef<number>(0);
  const pendingPointerMoveRef = useRef<MouseEventLike | null>(null);
  const isPanningRef = useRef(false);
  const panStartRef = useRef({ x: 0, y: 0 });
  const spaceHeldRef = useRef(false);
  const statusMsgRef = useRef('');
  const prevToolRef = useRef<ToolType>('select');
  const geometrySnapMemoryRef = useRef<string | null>(null);
  const rulerDragAxisRef = useRef<'horizontal' | 'vertical' | null>(null);
  const cameraOverlayAdjustDragRef = useRef<CameraOverlayAdjustDrag | null>(null);
  const interactionEndTimerRef = useRef<number | null>(null);
  const pointerDragCandidateRef = useRef<{ x: number; y: number; objectIds: string[] } | null>(null);

  const { width, height } = useCanvasSize(containerRef);

  useEffect(() => {
    setCanvasViewportSize({ width, height });
  }, [width, height]);

  useEffect(() => () => clearCanvasViewportSize(), []);

  // Store selectors
  const project = useProjectStore((s) => s.project);
  const meshDeformMode = useUiStore((s) => s.meshDeformMode);
  const selectedObjectIds = useProjectStore((s) => s.selectedObjectIds);
  const selectionTransformLocks = useMemo(
    () => effectiveTransformLocks(
      project?.objects.filter((object) => selectedObjectIds.includes(object.id)) ?? [],
    ),
    [project?.objects, selectedObjectIds],
  );
  const selectedLayerId = useProjectStore((s) => s.selectedLayerId);
  const selectObjects = useProjectStore((s) => s.selectObjects);
  const toggleObjectSelection = useProjectStore((s) => s.toggleObjectSelection);
  const addObject = useProjectStore((s) => s.addObject);
  const updateObject = useProjectStore((s) => s.updateObject);
  const rotateObjects = useProjectStore((s) => s.rotateObjects);
  const shearObjects = useProjectStore((s) => s.shearObjects);
  const updateObjectBoundsBatch = useProjectStore((s) => s.updateObjectBoundsBatch);

  const previewState = usePreviewStore((s) => s.state);
  const previewData = usePreviewStore((s) => s.data);
  const showPreview = usePreviewStore((s) => s.showPreview);
  const manualRefreshRequired = usePreviewStore((s) => s.manualRefreshRequired);

  const settings = useAppStore((s) => s.settings);

  const activeTool = useUiStore((s) => s.activeTool);
  const zoom = useUiStore((s) => s.zoom);
  const viewportOffset = useUiStore((s) => s.viewportOffset);
  const artworkDisplayMode = useUiStore((s) => s.artworkDisplayMode);
  const smoothEdges = useUiStore((s) => s.smoothEdges);
  const workspaceMode = useUiStore((s) => s.workspaceMode);
  const gridVisible = useUiStore((s) => s.gridVisible);
  const snapEnabled = useUiStore((s) => s.snapToGrid);
  const snapToObjects = useUiStore((s) => s.snapToObjects);
  const gridSpacingMm = useUiStore((s) => s.gridSpacingMm);
  const flashedLayerId = useUiStore((s) => s.flashedLayerId);
  const selectionIsolationPath = useUiStore((s) => s.selectionIsolationPath);
  const setSelectionIsolationPath = useUiStore((s) => s.setSelectionIsolationPath);
  const setZoom = useUiStore((s) => s.setZoom);
  const setViewportOffset = useUiStore((s) => s.setViewportOffset);
  const zoomToFit = useUiStore((s) => s.zoomToFit);
  const setCursorWorldPos = useUiStore((s) => s.setCursorWorldPos);
  const showLastPosition = useUiStore((s) => s.showLastPosition);
  const textEditObjectId = useUiStore((s) => s.textEditObjectId);
  // Offset dialog live preview (dashed ghost). Subscribe with a selector so the
  // dirty-flag canvas repaints when the dialog sets/clears it.
  const offsetPreview = useUiStore((s) => s.offsetPreview);
  const offsetPreviewSourceFrame = useUiStore((s) => s.offsetPreviewSourceFrame);
  // Cursor feedback must update immediately when the text panel switches
  // between Point and Box mode. TextTool reads the same value in getCursor().
  const textBoxModeActive = useUiStore((s) => (s.textDefaults.max_width ?? 0) > 0);
  const insertArtLibraryItem = useArtLibraryStore((s) => s.insertToProject);
  const setArtLibraryDragState = useArtLibraryStore((s) => s.setDragState);

  const activeProfileId = useMachineStore((s) => s.activeProfileId);
  const machineStatus = useMachineStore((s) => s.machineStatus);
  const cameraOverlayState = useCameraStore((s) => s.overlayState);
  const cameraOverlayVisible = useCameraStore((s) => s.overlayVisible);
  const cameraOverlayOpacity = useCameraStore((s) => s.overlayOpacity);
  const cameraCalibration = useCameraStore((s) => s.calibration);
  const cameraAlignment = useCameraStore((s) => s.alignment);
  const cameraDraftOverlayTransform = useCameraStore((s) => s.draftOverlayTransform);
  const cameraOverlayAdjustMode = useCameraStore((s) => s.overlayAdjustMode);
  const cameraOverlayDraftDirty = useCameraStore((s) => s.overlayDraftDirty);
  const refreshCameraOverlayState = useCameraStore((s) => s.refreshOverlayState);
  const setCameraDraftOverlayTransform = useCameraStore((s) => s.setDraftOverlayTransform);
  const commitCameraDraftOverlayTransform = useCameraStore((s) => s.commitDraftOverlayTransform);

  const tool = TOOL_INSTANCES[resolveWorkspaceCanvasTool(workspaceMode, activeTool)];

  const theme = settings?.dark_mode ? DARK_THEME : LIGHT_THEME;
  const { useLayerAppearance, filledRendering } = renderOptionsFromArtworkDisplayMode(artworkDisplayMode);
  const antialiasing = smoothEdges;

  // In inch mode the visible grid uses chooseInchInterval, so snap must match
  const displayUnit = settings?.display_unit === 'inches' ? 'inches' : 'mm';
  const effectiveSnapSpacing =
    displayUnit === 'inches' ? chooseInchInterval(zoom).spacingMm : gridSpacingMm;

  // Selection dash animation refs
  const dashOffsetRef = useRef(0);
  const animFrameRef = useRef(0);
  const lastAutoFitRef = useRef<{
    projectId: string;
    workspaceKey: string;
    canvasKey: string;
    offset: { x: number; y: number };
    zoom: number;
  } | null>(null);

  // Always-visible persistent tab markers (must be before render callback which reads it)
  const [persistentTabMarkers, setPersistentTabMarkers] = useState<
    PersistentTabMarkerSnapshot[]
  >([]);
  const [rulerGuidePreview, setRulerGuidePreview] = useState<
    { axis: 'horizontal' | 'vertical'; value: number } | null
  >(null);
  const [interactionState, setInteractionState] = useState<CanvasInteractionState>({
    active: false,
    kind: 'none',
    objectIds: [],
  });
  const [selectionPicker, setSelectionPicker] = useState<SelectionPickerState>(null);

  useEffect(() => {
    if (!project || selectionIsolationPath.length === 0) return;
    const validIds = new Set(project.objects.filter((object) => object.data.type === 'group').map((object) => object.id));
    const validPath = selectionIsolationPath.filter((id) => validIds.has(id));
    if (validPath.length !== selectionIsolationPath.length) setSelectionIsolationPath(validPath);
  }, [project, selectionIsolationPath, setSelectionIsolationPath]);

  // Start-point pick mode state
  const [startPointVertices, setStartPointVertices] = useState<
    {
      worldX: number;
      worldY: number;
      isStart: boolean;
      subpathIndex: number;
      vertexIndex: number;
      subpathClosed: boolean;
    }[]
  >([]);
  const startPointHoveredRef = useRef<number | null>(null);

  const vp: ViewportParams = useMemo(
    () => ({
      offset: viewportOffset,
      zoom,
      canvasWidth: width,
      canvasHeight: height,
    }),
    [viewportOffset, zoom, width, height],
  );
  const assetSignature = useMemo(
    () => (project?.assets ?? []).map((asset) => asset.id).join(','),
    [project?.assets],
  );
  const laserPosition = useMemo(() => {
    if (!project || !machineStatus?.work_position) return null;
    return machineToCanvasPoint(
      { x: machineStatus.work_position.x, y: machineStatus.work_position.y },
      project.workspace,
    );
  }, [project, machineStatus?.work_position]);
  const cameraFrame = cameraOverlayState?.frame ?? null;
  const cameraImageWarp = (
    cameraOverlayState?.alignment ?? cameraAlignment
  )?.image_warp ?? null;
  const cameraOverlayWidthPx = cameraImageWarp?.output_width_px ?? cameraFrame?.width_px ?? 0;
  const cameraOverlayHeightPx = cameraImageWarp?.output_height_px ?? cameraFrame?.height_px ?? 0;
  const cameraOverlayAssetUrl = useMemo(() => {
    if (!cameraFrame) return null;
    return cameraFrameAssetUrl(cameraFrame.file_path, cameraFrame.handle_id);
  }, [cameraFrame]);
  const cameraOverlayTransform = useMemo(() => {
    const savedTransform = (
      cameraOverlayState?.alignment
      ?? cameraAlignment
      ?? cameraOverlayState?.calibration
      ?? cameraCalibration
    )?.transform ?? null;
    if (
      cameraDraftOverlayTransform &&
      (cameraOverlayAdjustMode || cameraOverlayDraftDirty || !savedTransform)
    ) {
      return cameraDraftOverlayTransform;
    }
    return savedTransform ?? cameraDraftOverlayTransform;
  }, [
    cameraAlignment,
    cameraCalibration,
    cameraOverlayDraftDirty,
    cameraDraftOverlayTransform,
    cameraOverlayAdjustMode,
    cameraOverlayState?.alignment,
    cameraOverlayState?.calibration,
  ]);
  const cameraOverlay = useMemo(() => {
    if (
      !cameraOverlayVisible ||
      !cameraFrame ||
      !cameraOverlayTransform
    ) {
      return null;
    }
    return {
      frameHandleId: cameraFrame.handle_id,
      sourceWidthPx: cameraFrame.width_px,
      sourceHeightPx: cameraFrame.height_px,
      widthPx: cameraOverlayWidthPx,
      heightPx: cameraOverlayHeightPx,
      imageWarp: cameraImageWarp,
      transform: cameraOverlayTransform,
      opacity: cameraOverlayOpacity,
    };
  }, [
    cameraFrame,
    cameraImageWarp,
    cameraOverlayHeightPx,
    cameraOverlayOpacity,
    cameraOverlayTransform,
    cameraOverlayVisible,
    cameraOverlayWidthPx,
  ]);

  useEffect(() => {
    if (cameraFrame?.file_path) {
      void verifyCameraFrameTempScope(cameraFrame.file_path);
    }
  }, [cameraFrame?.file_path]);

  useEffect(() => {
    void refreshCameraOverlayState();
  }, [activeProfileId, refreshCameraOverlayState]);

  useEffect(() => {
    if (!project || width <= 0 || height <= 0) return;

    const { bed_width_mm, bed_height_mm, origin } = project.workspace;
    if (bed_width_mm <= 0 || bed_height_mm <= 0) return;

    const projectId = project.metadata.project_id;
    const workspaceKey = `${bed_width_mm}:${bed_height_mm}:${origin}`;
    const canvasKey = `${width}:${height}`;
    const last = lastAutoFitRef.current;
    const viewportStillAtAutoFit = last
      && Math.abs(viewportOffset.x - last.offset.x) < 0.001
      && Math.abs(viewportOffset.y - last.offset.y) < 0.001
      && Math.abs(zoom - last.zoom) < 0.001;
    const shouldFit =
      !last
      || last.projectId !== projectId
      || Boolean(viewportStillAtAutoFit);

    if (!shouldFit) return;
    if (
      last?.projectId === projectId
      && last.workspaceKey === workspaceKey
      && last.canvasKey === canvasKey
    ) {
      return;
    }

    const result = zoomToFitBounds(
      { min: { x: 0, y: 0 }, max: { x: bed_width_mm, y: bed_height_mm } },
      width,
      height,
    );
    lastAutoFitRef.current = {
      projectId,
      workspaceKey,
      canvasKey,
      offset: result.offset,
      zoom: result.zoom,
    };
    zoomToFit(result.offset, result.zoom);
  }, [project, viewportOffset.x, viewportOffset.y, zoom, width, height, zoomToFit]);

  const hasHeavySelection = useMemo(() => {
    if (!project || selectedObjectIds.length === 0) return false;

    const selected = new Set(selectedObjectIds);
    let commandCount = 0;
    for (const obj of project.objects) {
      if (!selected.has(obj.id)) continue;

      if (obj.data.type === 'vector_path') {
        commandCount += getVectorPathCommandCountForObject(obj);
      } else if (obj.data.type === 'text' && obj.data.resolved_path_data) {
        commandCount += getVectorPathCommandCount(obj.data.resolved_path_data);
      }

      if (commandCount >= HEAVY_SELECTION_COMMAND_THRESHOLD) {
        return true;
      }
    }

    return false;
  }, [project, selectedObjectIds]);

  useEffect(() => registerCanvasScreenshotProvider(() => {
    const baseCanvas = baseCanvasRef.current;
    const overlayCanvas = overlayCanvasRef.current;
    if (!baseCanvas || !overlayCanvas) return null;
    return { baseCanvas, overlayCanvas };
  }), []);

  const scheduleInteractionStop = useCallback(() => {
    if (interactionEndTimerRef.current !== null) {
      window.clearTimeout(interactionEndTimerRef.current);
    }
    interactionEndTimerRef.current = window.setTimeout(() => {
      setInteractionState({ active: false, kind: 'none', objectIds: [] });
      interactionEndTimerRef.current = null;
    }, INTERACTION_IDLE_MS);
  }, []);

  const beginInteraction = useCallback((kind: CanvasInteractionState['kind'], objectIds: string[] = []) => {
    if (interactionEndTimerRef.current !== null) {
      window.clearTimeout(interactionEndTimerRef.current);
      interactionEndTimerRef.current = null;
    }
    setInteractionState((current) => {
      const sameIds =
        (current.objectIds ?? []).length === objectIds.length &&
        (current.objectIds ?? []).every((id, index) => id === objectIds[index]);
      if (current.active && current.kind === kind && sameIds) {
        return current;
      }
      return { active: true, kind, objectIds };
    });
    scheduleInteractionStop();
  }, [scheduleInteractionStop]);

  useEffect(() => {
    usePreviewStore.getState().setInteractionActive(interactionState.active);
  }, [interactionState.active]);

  // Context menu
  const {
    menuState,
    handleContextMenu,
    closeMenu,
    traceImageObjectId,
    closeTraceDialog,
    adjustImageObjectId,
    closeAdjustDialog,
  } = useCanvasContextMenu();

  // Initialize renderer
  useEffect(() => {
    const baseCanvas = baseCanvasRef.current;
    const overlayCanvas = overlayCanvasRef.current;
    if (!baseCanvas || !overlayCanvas) return;
    const baseCtx = baseCanvas.getContext('2d');
    const overlayBuffer = document.createElement('canvas');
    const overlayCtx = overlayBuffer.getContext('2d');
    if (!baseCtx || !overlayCtx) return;
    const baseRenderer = new CanvasRenderer(baseCtx);
    const overlayRenderer = new CanvasRenderer(overlayCtx);
    overlayBufferRef.current = overlayBuffer;
    baseRendererRef.current = baseRenderer;
    overlayRendererRef.current = overlayRenderer;
    return () => {
      baseRenderer.dispose();
      overlayRenderer.dispose();
      baseRendererRef.current = null;
      overlayRendererRef.current = null;
      overlayBufferRef.current = null;
    };
  }, []);

  // Clear the preview bitmap cache whenever a new plan arrives, so
  // blob URLs from the previous plan are released promptly and the new
  // plan's bitmaps replace them cleanly.
  useEffect(() => {
    baseRendererRef.current?.clearPreviewBitmapCache();
  }, [previewData?.plan_id]);

  useEffect(() => {
    baseRendererRef.current?.clearImageCache();
  }, [project?.metadata.project_id, assetSignature]);

  useEffect(() => {
    return () => {
      if (interactionEndTimerRef.current !== null) {
        window.clearTimeout(interactionEndTimerRef.current);
      }
      cancelAnimationFrame(sceneRafRef.current);
      cancelAnimationFrame(overlayRafRef.current);
      cancelAnimationFrame(pointerMoveRafRef.current);
      cancelAnimationFrame(animFrameRef.current);
    };
  }, []);

  const prepareCanvas = useCallback((canvas: HTMLCanvasElement | null) => {
    if (!canvas) return null;
    const dpr = window.devicePixelRatio || 1;
    return prepareCanvasSurface(canvas, width, height, dpr);
  }, [width, height]);

  const buildEffectiveOverlay = useCallback(() => {
    if (!project) return { type: 'none' } as ReturnType<CanvasTool['getOverlay']>;

    // Never carry a partially-drawn design overlay into the Run workspace.
    let effectiveOverlay = workspaceMode === 'run'
      ? ({ type: 'none' } as ReturnType<CanvasTool['getOverlay']>)
      : tool.getOverlay();
    if (rulerGuidePreview) {
      effectiveOverlay = {
        type: 'ruler-guide-preview',
        axis: rulerGuidePreview.axis,
        value: rulerGuidePreview.value,
      };
    }

    const pendingSPId = useUiStore.getState().pendingStartPointObjectId;
    if (pendingSPId && startPointVertices.length > 0) {
      const spObj = project.objects.find((o) => o.id === pendingSPId);
      let effectiveEditsObj = spObj;
      let cloneDepth = 0;
      while (effectiveEditsObj?.data?.type === 'virtual_clone' && cloneDepth < 10) {
        const srcId = effectiveEditsObj.data.source_id;
        effectiveEditsObj = project.objects.find((o) => o.id === srcId);
        cloneDepth++;
      }
      const edits = effectiveEditsObj?.start_point_edits ?? [];
      const subpathMap = new Map<number, typeof startPointVertices>();
      for (const v of startPointVertices) {
        if (!v.subpathClosed) continue;
        if (!subpathMap.has(v.subpathIndex)) subpathMap.set(v.subpathIndex, []);
        subpathMap.get(v.subpathIndex)!.push(v);
      }
      const subpathArrows: Array<{
        subpathIndex: number;
        fromX: number;
        fromY: number;
        toX: number;
        toY: number;
        isCustom: boolean;
      }> = [];
      for (const [spIdx, verts] of subpathMap) {
        if (verts.length < 2) continue;
        const start = verts.find((v) => v.isStart) ?? verts[0];
        const startIdx = verts.indexOf(start);
        const next = verts[(startIdx + 1) % verts.length];
        subpathArrows.push({
          subpathIndex: spIdx,
          fromX: start.worldX,
          fromY: start.worldY,
          toX: next.worldX,
          toY: next.worldY,
          isCustom: edits.some((e) => e.subpathIndex === spIdx),
        });
      }
      effectiveOverlay = {
        type: 'start-point-pick' as const,
        vertices: startPointVertices,
        subpathArrows,
        hoveredIndex: startPointHoveredRef.current,
      };
    }

    // Offset dialog preview overrides tool overlays while active (the dialog is
    // modal, so no tool interaction is competing for the overlay slot).
    if (offsetPreview && offsetPreview.length > 0) {
      effectiveOverlay = {
        type: 'offset-preview' as const,
        paths: offsetPreview,
        translation: offsetPreviewTranslation(offsetPreviewSourceFrame, project?.objects ?? []),
      };
    }

    // Camera overlay adjust keeps top priority: its handles are an explicit
    // interactive mode and must never be hidden by a decorative ghost.
    if (cameraOverlayAdjustMode && cameraFrame && cameraOverlayTransform) {
      effectiveOverlay = {
        type: 'camera-overlay-adjust' as const,
        widthPx: cameraOverlayWidthPx,
        heightPx: cameraOverlayHeightPx,
        transform: cameraOverlayTransform,
      };
    }

    return effectiveOverlay;
  }, [
    cameraFrame,
    cameraOverlayAdjustMode,
    cameraOverlayHeightPx,
    cameraOverlayTransform,
    cameraOverlayWidthPx,
    offsetPreview,
    offsetPreviewSourceFrame,
    project,
    rulerGuidePreview,
    startPointVertices,
    tool,
    workspaceMode,
  ]);

  const renderBaseScene = useCallback(() => {
    const renderer = baseRendererRef.current;
    if (!renderer || !project) return;
    const prepared = prepareCanvas(baseCanvasRef.current);
    if (!prepared) return;
    if (prepared.resized) renderer.invalidateBaseScene();

    const effectiveOverlay = buildEffectiveOverlay();
    renderer.renderBaseScene({
      workspace: project.workspace,
      objects: project.objects,
      layers: project.layers,
      selectedObjectIds,
      vp,
      gridVisible,
      gridSpacingMm,
      toolOverlay: effectiveOverlay,
      previewData: previewData,
      // Run is a read-only workspace, not an implicit toolpath preview.
      // Canvas preview visibility is controlled independently of workspace mode.
      showPreview,
      cameraOverlay,
      theme,
      antialiasing,
      filledRendering,
      // Display overrides are editor-only. The default mode follows layer
      // operations in both Design and Run, including compound-fill holes.
      useLayerAppearance,
      previewState,
      previewManualRefreshRequired: manualRefreshRequired,
      selectionDashOffset: dashOffsetRef.current,
      selectionIsolationObjectId: selectionIsolationPath[selectionIsolationPath.length - 1] ?? null,
      showLastPosition,
      laserPosition,
      skipObjectId: textEditObjectId,
      displayUnit: (settings?.display_unit === 'inches' ? 'inches' : 'mm') as 'mm' | 'inches',
      transformLocks: workspaceMode === 'run' ? RUN_MODE_SELECTION_LOCKS : selectionTransformLocks,
      flashedLayerId,
      interactionState,
    });
  }, [
    project,
    selectedObjectIds,
    selectionTransformLocks,
    vp,
    gridVisible,
    gridSpacingMm,
    previewData,
    showPreview,
    workspaceMode,
    cameraOverlay,
    theme,
    antialiasing,
    filledRendering,
    useLayerAppearance,
    previewState,
    manualRefreshRequired,
    showLastPosition,
    laserPosition,
    textEditObjectId,
    settings?.display_unit,
    flashedLayerId,
    interactionState,
    selectionIsolationPath,
    prepareCanvas,
    buildEffectiveOverlay,
  ]);

  const renderToolOverlay = useCallback(() => {
    const renderer = overlayRendererRef.current;
    if (!renderer || !project) return;
    const visible = prepareCanvas(overlayCanvasRef.current);
    const buffer = overlayBufferRef.current;
    const preparedBuffer = prepareCanvas(buffer);
    if (!visible || !buffer || !preparedBuffer) return;

    const filteredTabMarkerSnapshots =
      activeTool === 'tabs' && selectedObjectIds[0]
        ? persistentTabMarkers.filter((m) => m.objectId !== selectedObjectIds[0])
        : persistentTabMarkers;
    const objectsById = new Map(project.objects.map((object) => [object.id, object]));
    const filteredTabMarkers = filteredTabMarkerSnapshots.map((marker) => {
      const object = objectsById.get(marker.objectId);
      return object
        ? remapPersistentTabMarker(marker, object.bounds, object.transform)
        : { worldX: marker.worldX, worldY: marker.worldY };
    });

    const effectiveOverlay = buildEffectiveOverlay();
    const renderStartedAt = performance.now();
    renderer.renderToolOverlay({
      workspace: project.workspace,
      objects: project.objects,
      layers: project.layers,
      selectedObjectIds,
      vp,
      gridVisible,
      gridSpacingMm,
      toolOverlay: effectiveOverlay,
      previewData: previewData,
      showPreview,
      theme,
      antialiasing,
      filledRendering,
      useLayerAppearance,
      previewState,
      previewManualRefreshRequired: manualRefreshRequired,
      selectionDashOffset: dashOffsetRef.current,
      selectionIsolationObjectId: selectionIsolationPath[selectionIsolationPath.length - 1] ?? null,
      showLastPosition,
      laserPosition,
      skipObjectId: textEditObjectId,
      persistentTabMarkers: filteredTabMarkers,
      displayUnit: (settings?.display_unit === 'inches' ? 'inches' : 'mm') as 'mm' | 'inches',
      transformLocks: workspaceMode === 'run' ? RUN_MODE_SELECTION_LOCKS : selectionTransformLocks,
      flashedLayerId,
      interactionState,
    });
    presentCanvasBuffer(visible.ctx, buffer);
    const renderDuration = performance.now() - renderStartedAt;
    overlayLastRenderAtRef.current = performance.now();
    if (effectiveOverlay.type === 'mesh-deform' && effectiveOverlay.previewActive) {
      const previousAverage = meshPreviewRenderAverageRef.current;
      const average = previousAverage > 0
        ? previousAverage * 0.8 + renderDuration * 0.2
        : renderDuration;
      meshPreviewRenderAverageRef.current = average;
      meshPreviewFrameIntervalRef.current = adaptiveMeshPreviewFrameInterval(
        meshPreviewFrameIntervalRef.current,
        average,
      );
    } else {
      meshPreviewRenderAverageRef.current = 0;
      meshPreviewFrameIntervalRef.current = 0;
    }
  }, [
    project,
    selectedObjectIds,
    selectionTransformLocks,
    vp,
    gridVisible,
    gridSpacingMm,
    previewData,
    showPreview,
    workspaceMode,
    theme,
    antialiasing,
    filledRendering,
    useLayerAppearance,
    previewState,
    manualRefreshRequired,
    showLastPosition,
    laserPosition,
    textEditObjectId,
    activeTool,
    persistentTabMarkers,
    settings?.display_unit,
    flashedLayerId,
    interactionState,
    selectionIsolationPath,
    prepareCanvas,
    buildEffectiveOverlay,
  ]);

  const renderScene = useCallback(() => {
    renderBaseScene();
    renderToolOverlay();
  }, [renderBaseScene, renderToolOverlay]);

  useEffect(() => {
    cancelAnimationFrame(sceneRafRef.current);
    cancelAnimationFrame(overlayRafRef.current);
    renderScene();
  }, [renderScene]);

  const requestRender = useCallback(() => {
    cancelAnimationFrame(sceneRafRef.current);
    sceneRafRef.current = requestAnimationFrame(() => renderScene());
  }, [renderScene]);

  useEffect(() => {
    baseRendererRef.current?.setRenderCallback(requestRender);
  }, [requestRender]);

  useEffect(() => {
    const renderer = baseRendererRef.current;
    if (!renderer || !cameraFrame || !cameraOverlayAssetUrl) return;
    renderer.ensureCameraOverlayImage(cameraFrame.handle_id, cameraOverlayAssetUrl);
  }, [cameraFrame, cameraOverlayAssetUrl]);

  const requestOverlayRender = useCallback(() => {
    cancelAnimationFrame(overlayRafRef.current);
    const renderWhenReady = (timestamp: number) => {
      const interval = meshPreviewFrameIntervalRef.current;
      if (interval > 0 && timestamp - overlayLastRenderAtRef.current < interval) {
        overlayRafRef.current = requestAnimationFrame(renderWhenReady);
        return;
      }
      overlayRafRef.current = 0;
      renderToolOverlay();
    };
    overlayRafRef.current = requestAnimationFrame(renderWhenReady);
  }, [renderToolOverlay]);

  useEffect(
    () => useMeasurementStore.subscribe(() => requestOverlayRender()),
    [requestOverlayRender],
  );

  // Selection dash animation (marching ants).
  useEffect(() => {
    const shouldAnimate = shouldAnimateSelectionDashes({
      selectedObjectCount: selectedObjectIds.length,
      reduceMotion: settings?.reduce_motion === true,
      hasHeavySelection,
      interactionActive: interactionState.active,
      interactionKind: interactionState.kind,
    });
    if (!shouldAnimate) {
      cancelAnimationFrame(animFrameRef.current);
      dashOffsetRef.current = 0;
      requestOverlayRender();
      return;
    }
    const animate = () => {
      dashOffsetRef.current = (dashOffsetRef.current + 0.3) % 20;
      requestOverlayRender();
      animFrameRef.current = requestAnimationFrame(animate);
    };
    animFrameRef.current = requestAnimationFrame(animate);
    return () => cancelAnimationFrame(animFrameRef.current);
  }, [
    hasHeavySelection,
    interactionState.active,
    interactionState.kind,
    selectedObjectIds.length,
    settings?.reduce_motion,
    requestOverlayRender,
  ]);

  // Load raster image assets into renderer's image cache
  useEffect(() => {
    const renderer = baseRendererRef.current;
    if (!renderer || !project) return;
    let cancelled = false;

    renderer.setRenderCallback(requestRender);

    const loadAssetData = useProjectStore.getState().loadAssetData;
    for (const obj of project.objects) {
      if (obj.data.type === 'raster_image') {
        const assetKey = obj.data.asset_key;
        loadAssetData(assetKey)
          .then((blobUrl) => {
            if (!cancelled) {
              renderer.clearImageLoadError(assetKey);
              renderer.ensureImage(assetKey, blobUrl);
            }
          })
          .catch((error) => {
            if (cancelled) return;
            renderer.markImageLoadError(assetKey, String(error));
            requestRender();
          });
      }
    }
    return () => {
      cancelled = true;
    };
  }, [project, requestRender]);

  // Build mouse event helper
  const buildMouseEvent = useCallback(
    (e: MouseEventLike): CanvasMouseEvent => {
      const canvas = overlayCanvasRef.current;
      if (!canvas) {
        return {
          screenX: 0,
          screenY: 0,
          worldX: 0,
          worldY: 0,
          snappedX: 0,
          snappedY: 0,
          button: 0,
          shiftKey: false,
          ctrlKey: false,
          altKey: false,
        };
      }
      const rect = canvas.getBoundingClientRect();
      const screenX = e.clientX - rect.left;
      const screenY = e.clientY - rect.top;
      const world = screenToWorld({ x: screenX, y: screenY }, vp);
      const ctrlKey = Boolean(e.ctrlKey || e.metaKey);
      const snapResult = resolveCanvasPointerSnap({
        world,
        ctrlKey,
        altKey: e.altKey,
        project,
        zoom: vp.zoom,
        snapEnabled,
        gridVisible,
        effectiveSnapSpacing,
        snapToObjects: snapToObjects || activeTool === 'measure',
        snapThresholdPx: settings?.snap_threshold_px ?? null,
        preferredTargetKey: geometrySnapMemoryRef.current,
        snapToWorkspace: activeTool === 'measure',
      });
      geometrySnapMemoryRef.current = snapResult.nextPreferredTargetKey;

      return {
        screenX,
        screenY,
        worldX: world.x,
        worldY: world.y,
        snappedX: snapResult.snapped.x,
        snappedY: snapResult.snapped.y,
        button: e.button,
        shiftKey: e.shiftKey,
        ctrlKey,
        altKey: e.altKey,
      };
    },
    [vp, snapEnabled, gridVisible, effectiveSnapSpacing, project, settings?.snap_threshold_px, snapToObjects, activeTool],
  );

  const getRulerDragAxis = useCallback(
    (screenX: number, screenY: number): 'horizontal' | 'vertical' | null => {
      const inTop =
        screenY >= 0 &&
        screenY <= RULER_SIZE &&
        screenX >= RULER_SIZE &&
        screenX <= width - RULER_SIZE;
      const inBottom =
        screenY >= height - RULER_SIZE &&
        screenY <= height &&
        screenX >= RULER_SIZE &&
        screenX <= width - RULER_SIZE;
      const inLeft =
        screenX >= 0 &&
        screenX <= RULER_SIZE &&
        screenY >= RULER_SIZE &&
        screenY <= height - RULER_SIZE;
      const inRight =
        screenX >= width - RULER_SIZE &&
        screenX <= width &&
        screenY >= RULER_SIZE &&
        screenY <= height - RULER_SIZE;
      if (inTop || inBottom) return 'vertical';
      if (inLeft || inRight) return 'horizontal';
      return null;
    },
    [height, width],
  );

  const extractGuideValue = useCallback(
    (axis: 'horizontal' | 'vertical', screenX: number, screenY: number) => {
      const world = screenToWorld({ x: screenX, y: screenY }, vp);
      return axis === 'vertical' ? world.x : world.y;
    },
    [vp],
  );

  // Build tool context
  const buildToolContext = useCallback((): ToolContext => {
    return {
      vp,
      workspace: project?.workspace ?? { bed_width_mm: 400, bed_height_mm: 400, origin: 'top_left' },
      objects: project?.objects ?? [],
      selectedObjectIds,
      selectedLayerId,
      layers:
        project?.layers.map((l) => ({
          id: l.id,
          name: l.name,
          color_tag: l.color_tag,
          enabled: l.enabled,
          visible: l.visible,
          operation: l.entries[0]?.operation ?? 'line',
        })) ?? [],
      selectionIsolationPath,
      // TransformLocks is non-optional; default to all-enabled per backend Default impl.
      transformLocks: selectionTransformLocks,
      readOnly: workspaceMode === 'run',
      snapEnabled: snapEnabled && gridVisible,
      snapToObjects,
      gridSpacingMm: effectiveSnapSpacing,
      selectObjects,
      toggleObjectSelection,
      addObject,
      updateObject,
      rotateObjects,
      shearObjects,
      updateObjectBoundsBatch,
      setCursorWorldPos,
      setStatusMessage: (msg: string) => {
        statusMsgRef.current = msg;
      },
      requestRender,
      requestOverlayRender,
      setSelectionIsolationPath,
      openSelectionPicker: (screen, objectIds) => setSelectionPicker({ x: screen.x, y: screen.y, objectIds }),
      closeSelectionPicker: () => setSelectionPicker(null),
    };
  }, [
    vp,
    project,
    selectedObjectIds,
    selectionTransformLocks,
    selectedLayerId,
    workspaceMode,
    snapEnabled,
    snapToObjects,
    gridVisible,
    effectiveSnapSpacing,
    selectObjects,
    toggleObjectSelection,
    addObject,
    updateObject,
    rotateObjects,
    shearObjects,
    updateObjectBoundsBatch,
    setCursorWorldPos,
    requestRender,
    requestOverlayRender,
    selectionIsolationPath,
    setSelectionIsolationPath,
  ]);

  const processPointerMove = useCallback(
    (e: MouseEventLike) => {
      const canvas = overlayCanvasRef.current;
      if (canvas) {
        const rect = canvas.getBoundingClientRect();
        const screenX = e.clientX - rect.left;
        const screenY = e.clientY - rect.top;
        const world = screenToWorld({ x: screenX, y: screenY }, vp);
        setCursorWorldPos(world);
      }

      if (isPanningRef.current) {
        beginInteraction('pan');
        const dx = e.clientX - panStartRef.current.x;
        const dy = e.clientY - panStartRef.current.y;
        panStartRef.current = { x: e.clientX, y: e.clientY };

        const worldDx = screenToWorldDist(dx, zoom);
        const worldDy = screenToWorldDist(dy, zoom);
        setViewportOffset({
          x: viewportOffset.x - worldDx,
          y: viewportOffset.y - worldDy,
        });
        return;
      }

      const cameraDrag = cameraOverlayAdjustDragRef.current;
      if (cameraDrag && cameraFrame && ((e.buttons ?? 0) & 1) === 1 && canvas) {
        const rect = canvas.getBoundingClientRect();
        const currentScreen = {
          x: e.clientX - rect.left,
          y: e.clientY - rect.top,
        };
        let nextTransform: SimilarityTransform;
        if (cameraDrag.target.type === 'move') {
          nextTransform = translateCameraOverlayTransform(
            cameraDrag.startTransform,
            currentScreen.x - cameraDrag.startScreen.x,
            currentScreen.y - cameraDrag.startScreen.y,
            vp,
          );
        } else if (cameraDrag.target.type === 'scale') {
          nextTransform = scaleCameraOverlayTransform(
            cameraOverlayWidthPx,
            cameraOverlayHeightPx,
            cameraDrag.startTransform,
            cameraDrag.startScreen,
            currentScreen,
            vp,
          );
        } else {
          nextTransform = rotateCameraOverlayTransform(
            cameraOverlayWidthPx,
            cameraOverlayHeightPx,
            cameraDrag.startTransform,
            cameraDrag.startScreen,
            currentScreen,
            vp,
          );
        }
        setCameraDraftOverlayTransform(nextTransform, true);
        requestRender();
        return;
      }

      if (workspaceMode !== 'run' && ((e.buttons ?? 0) & 1) === 1 && pointerDragCandidateRef.current) {
        const dx = e.clientX - pointerDragCandidateRef.current.x;
        const dy = e.clientY - pointerDragCandidateRef.current.y;
        if (
          dx * dx + dy * dy >=
          POINTER_DRAG_INTERACTION_THRESHOLD_PX * POINTER_DRAG_INTERACTION_THRESHOLD_PX
        ) {
          beginInteraction('object-drag', pointerDragCandidateRef.current.objectIds);
        }
      }

      if (rulerDragAxisRef.current && canvas) {
        const rect = canvas.getBoundingClientRect();
        const screenX = e.clientX - rect.left;
        const screenY = e.clientY - rect.top;
        setRulerGuidePreview({
          axis: rulerDragAxisRef.current,
          value: extractGuideValue(rulerDragAxisRef.current, screenX, screenY),
        });
        requestRender();
        return;
      }

      if (useUiStore.getState().pendingStartPointObjectId && startPointVertices.length > 0) {
        const me = buildMouseEvent(e);
        const HIT_RADIUS = 8;
        let nearest: number | null = null;
        let bestDist = HIT_RADIUS;
        for (let i = 0; i < startPointVertices.length; i++) {
          const v = startPointVertices[i];
          if (!v.subpathClosed) continue;
          const sp = worldToScreen({ x: v.worldX, y: v.worldY }, vp);
          const dx = me.screenX - sp.x;
          const dy = me.screenY - sp.y;
          const dist = Math.sqrt(dx * dx + dy * dy);
          if (dist < bestDist) {
            bestDist = dist;
            nearest = i;
          }
        }
        if (nearest !== startPointHoveredRef.current) {
          startPointHoveredRef.current = nearest;
          requestRender();
        }
        return;
      }

      const liveUi = useUiStore.getState();
      const me = buildMouseEvent(e);
      const ctx = { ...buildToolContext(), readOnly: liveUi.workspaceMode === 'run' };
      const pointerTool = TOOL_INSTANCES[
        resolveWorkspaceCanvasTool(liveUi.workspaceMode, liveUi.activeTool)
      ];
      pointerTool.onMouseMove(me, ctx);
    },
    [
      beginInteraction,
      buildMouseEvent,
      buildToolContext,
      extractGuideValue,
      vp,
      zoom,
      viewportOffset,
      setViewportOffset,
      setCursorWorldPos,
      startPointVertices,
      cameraFrame,
      cameraOverlayHeightPx,
      cameraOverlayWidthPx,
      setCameraDraftOverlayTransform,
      requestRender,
      workspaceMode,
    ],
  );

  // Pointer handlers (pointer events + capture so drags work beyond canvas bounds)
  const handlePointerDown = useCallback(
    (e: React.PointerEvent) => {
      pendingPointerMoveRef.current = null;
      cancelAnimationFrame(pointerMoveRafRef.current);
      pointerMoveRafRef.current = 0;
      // Capture pointer so move/up events fire even when cursor leaves the canvas
      overlayCanvasRef.current?.setPointerCapture(e.pointerId);

      // Middle-click or space+left-click = pan
      if (e.button === 1 || (e.button === 0 && spaceHeldRef.current)) {
        isPanningRef.current = true;
        panStartRef.current = { x: e.clientX, y: e.clientY };
        pointerDragCandidateRef.current = null;
        beginInteraction('pan');
        e.preventDefault();
        return;
      }

      if (e.button === 0) {
        const liveWorkspaceMode = useUiStore.getState().workspaceMode;
        const adjustCanvas = overlayCanvasRef.current;
        if (cameraOverlayAdjustMode && cameraFrame && cameraOverlayTransform && adjustCanvas) {
          const rect = adjustCanvas.getBoundingClientRect();
          const screen = {
            x: e.clientX - rect.left,
            y: e.clientY - rect.top,
          };
          const hit = hitTestCameraOverlayControls(
            screen,
            cameraOverlayWidthPx,
            cameraOverlayHeightPx,
            cameraOverlayTransform,
            vp,
          );
          pointerDragCandidateRef.current = null;
          if (hit) {
            cameraOverlayAdjustDragRef.current = {
              target: hit,
              startScreen: screen,
              startTransform: cameraOverlayTransform,
              preDragDirty: cameraOverlayDraftDirty,
            };
            beginInteraction('object-drag');
          }
          e.preventDefault();
          return;
        }

        pointerDragCandidateRef.current = tracksObjectDragInteraction(
          liveWorkspaceMode,
          useUiStore.getState().activeTool,
        )
          ? {
              x: e.clientX,
              y: e.clientY,
              objectIds: [...selectedObjectIds],
            }
          : null;

        // Intercept click for Set Start Point pick mode
        const pendingId = useUiStore.getState().pendingStartPointObjectId;
        if (pendingId && liveWorkspaceMode !== 'run') {
          const me = buildMouseEvent(e);
          // Only act when the click is within hit radius of a closed-subpath vertex
          const HIT_RADIUS = 8;
          let hitVertex = false;
          for (const v of startPointVertices) {
            if (!v.subpathClosed) continue;
            const sp = worldToScreen({ x: v.worldX, y: v.worldY }, vp);
            const dx = me.screenX - sp.x;
            const dy = me.screenY - sp.y;
            if (Math.sqrt(dx * dx + dy * dy) < HIT_RADIUS) {
              hitVertex = true;
              break;
            }
          }
          if (!hitVertex) {
            pointerDragCandidateRef.current = null;
            return;
          }
          // Determine mode from modifier keys
          const mode: StartPointMode = me.ctrlKey ? 'reset' : me.shiftKey ? 'set_and_reverse' : 'set';

          // The start-point picker is a one-shot action. Consume it as soon as a
          // valid vertex is chosen so a slow backend refresh cannot leave stale
          // nodes visible or accept a second click.
          exitStartPointPickMode();
          setStartPointVertices([]);
          startPointHoveredRef.current = null;
          requestRender();

          void (async () => {
            try {
              await vectorService.setStartPoint(pendingId, me.worldX, me.worldY, mode);
              const refreshed = await projectService.getProject();
              if (refreshed) {
                useProjectStore.setState({ project: { ...refreshed, dirty: true } });
                usePreviewStore.getState().invalidate();
                await useUndoStore.getState().refresh();
              }
            } catch (err) {
              useNotificationStore.getState().push(wrapBackendError(String(err)), 'error');
            }
          })();
          pointerDragCandidateRef.current = null;
          return;
        }

        // Intercept click for Guide Path pick mode
        const pendingGuideText = useUiStore.getState().pendingGuidePathTextId;
        if (pendingGuideText && liveWorkspaceMode !== 'run') {
          const me = buildMouseEvent(e);
          const screenPt = { x: me.screenX, y: me.screenY };
          const currentProject = useProjectStore.getState().project;
          const objects = currentProject?.objects ?? [];
          const hit = hitTestPoint(screenPt, objects, vp, false, currentProject?.layers);
          const guideTypes = new Set(['shape', 'vector_path', 'polygon', 'star']);
          let hitType = hit?.data.type ?? '';
          if (hit && hitType === 'virtual_clone') {
            let srcId = (hit.data as { source_id: string }).source_id;
            for (let d = 0; d < 10; d++) {
              const src = objects.find((o) => o.id === srcId);
              if (!src || src.data.type !== 'virtual_clone') {
                hitType = src?.data.type ?? '';
                break;
              }
              srcId = (src.data as { source_id: string }).source_id;
            }
          }
          if (hit && guideTypes.has(hitType)) {
            useUiStore.getState().setPendingGuidePathText(null);
            void (async () => {
              try {
                await projectService.setTextGuidePath(pendingGuideText, hit.id);
                const refreshed = await projectService.getProject();
                if (refreshed) {
                  useProjectStore.setState({ project: { ...refreshed, dirty: true } });
                  usePreviewStore.getState().invalidate();
                  await useUndoStore.getState().refresh();
                }
              } catch (err) {
                useNotificationStore.getState().push(wrapBackendError(String(err)), 'error');
              }
            })();
          } else if (hit) {
            useNotificationStore
              .getState()
              .push('Click a vector or shape object to use as guide path', 'warning');
          }
          // Stay in pick mode if click missed or hit invalid target — user can press Escape to cancel
          pointerDragCandidateRef.current = null;
          return;
        }

        const canvas = overlayCanvasRef.current;
        if (canvas && liveWorkspaceMode !== 'run') {
          const rect = canvas.getBoundingClientRect();
          const rulerAxis = getRulerDragAxis(e.clientX - rect.left, e.clientY - rect.top);
          if (rulerAxis) {
            rulerDragAxisRef.current = rulerAxis;
            pointerDragCandidateRef.current = null;
            geometrySnapMemoryRef.current = null;
            setRulerGuidePreview({
              axis: rulerAxis,
              value: extractGuideValue(rulerAxis, e.clientX - rect.left, e.clientY - rect.top),
            });
            requestRender();
            return;
          }
        }

        const liveUi = useUiStore.getState();
        const me = buildMouseEvent(e);
        const ctx = { ...buildToolContext(), readOnly: liveUi.workspaceMode === 'run' };
        const pointerTool = TOOL_INSTANCES[
          resolveWorkspaceCanvasTool(liveUi.workspaceMode, liveUi.activeTool)
        ];
        pointerTool.onMouseDown(me, ctx);
      }
    },
    [
      buildMouseEvent,
      buildToolContext,
      beginInteraction,
      cameraFrame,
      cameraOverlayAdjustMode,
      cameraOverlayDraftDirty,
      cameraOverlayHeightPx,
      cameraOverlayTransform,
      cameraOverlayWidthPx,
      extractGuideValue,
      getRulerDragAxis,
      requestRender,
      selectedObjectIds,
      startPointVertices,
      vp,
    ],
  );

  const handlePointerMove = useCallback(
    (e: React.PointerEvent) => {
      pendingPointerMoveRef.current = {
        clientX: e.clientX,
        clientY: e.clientY,
        button: e.button,
        buttons: e.buttons,
        shiftKey: e.shiftKey,
        ctrlKey: e.ctrlKey,
        altKey: e.altKey,
        metaKey: e.metaKey,
      };
      if (pointerMoveRafRef.current) return;
      pointerMoveRafRef.current = requestAnimationFrame(() => {
        pointerMoveRafRef.current = 0;
        const pending = pendingPointerMoveRef.current;
        pendingPointerMoveRef.current = null;
        if (pending) {
          processPointerMove(pending);
        }
      });
    },
    [processPointerMove],
  );

  const flushPointerMove = useCallback(() => {
    if (pointerMoveRafRef.current) {
      cancelAnimationFrame(pointerMoveRafRef.current);
      pointerMoveRafRef.current = 0;
    }
    const pending = pendingPointerMoveRef.current;
    pendingPointerMoveRef.current = null;
    if (pending) {
      processPointerMove(pending);
    }
  }, [processPointerMove]);

  const handlePointerUp = useCallback(
    (e: React.PointerEvent) => {
      // WebKit can defer every pointermove in a short drag until the same
      // animation frame as pointerup. Commit that final move before releasing
      // the tool gesture; dropping it makes Warp appear completely inert.
      flushPointerMove();
      if (isPanningRef.current) {
        isPanningRef.current = false;
        pointerDragCandidateRef.current = null;
        scheduleInteractionStop();
        return;
      }

      if (e.button === 0) {
        if (cameraOverlayAdjustDragRef.current) {
          cameraOverlayAdjustDragRef.current = null;
          pointerDragCandidateRef.current = null;
          void commitCameraDraftOverlayTransform();
          scheduleInteractionStop();
          return;
        }
        pointerDragCandidateRef.current = null;
        if (rulerDragAxisRef.current) {
          const canvas = overlayCanvasRef.current;
          if (canvas && project) {
            const rect = canvas.getBoundingClientRect();
            const screenX = e.clientX - rect.left;
            const screenY = e.clientY - rect.top;
            const axis = rulerDragAxisRef.current;
            const value = extractGuideValue(axis, screenX, screenY);
            rulerDragAxisRef.current = null;
            setRulerGuidePreview(null);
            requestRender();
            const dropValue = resolveRulerGuideDropValue(axis, value, project.workspace);
            if (dropValue !== null) {
              void useProjectStore.getState().addRulerGuide(axis, dropValue);
            }
          }
          scheduleInteractionStop();
          return;
        }
        const liveUi = useUiStore.getState();
        const me = buildMouseEvent(e);
        const ctx = { ...buildToolContext(), readOnly: liveUi.workspaceMode === 'run' };
        const pointerTool = TOOL_INSTANCES[
          resolveWorkspaceCanvasTool(liveUi.workspaceMode, liveUi.activeTool)
        ];
        pointerTool.onMouseUp(me, ctx);
        scheduleInteractionStop();
      }
    },
    [
      buildMouseEvent,
      buildToolContext,
      extractGuideValue,
      commitCameraDraftOverlayTransform,
      project,
      requestRender,
      scheduleInteractionStop,
      flushPointerMove,
    ],
  );

  const handleWheel = useCallback(
    (e: React.WheelEvent) => {
      e.preventDefault();
      beginInteraction('zoom');

      const canvas = overlayCanvasRef.current;
      if (!canvas) return;

      const rect = canvas.getBoundingClientRect();
      const screenX = e.clientX - rect.left;
      const screenY = e.clientY - rect.top;

      if (settings?.scroll_zoom === false && !e.ctrlKey && !e.metaKey) {
        beginInteraction('pan');
        setViewportOffset({
          x: viewportOffset.x + screenToWorldDist(e.deltaX, zoom),
          y: viewportOffset.y + screenToWorldDist(e.deltaY, zoom),
        });
        return;
      }

      // Zoom centered on cursor
      const worldBefore = screenToWorld({ x: screenX, y: screenY }, vp);
      const factor = e.deltaY < 0 ? ZOOM_FACTOR : 1 / ZOOM_FACTOR;
      const newZoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, Math.round(zoom * factor)));

      const newVp = { ...vp, zoom: newZoom };
      const worldAfter = screenToWorld({ x: screenX, y: screenY }, newVp);

      setZoom(newZoom);
      setViewportOffset({
        x: viewportOffset.x + (worldBefore.x - worldAfter.x),
        y: viewportOffset.y + (worldBefore.y - worldAfter.y),
      });
    },
    [beginInteraction, settings?.scroll_zoom, vp, zoom, viewportOffset, setZoom, setViewportOffset],
  );

  const handleMouseLeave = useCallback(() => {
    setCursorWorldPos(null);
    tool.onMouseLeave?.(buildToolContext());
  }, [buildToolContext, setCursorWorldPos, tool]);

  // Right-click during an active left-drag must cancel the drag the same way
  // Escape does (restore original bounds/transforms, reset tool state) before
  // the context menu opens. Otherwise the tool's drag state stays live and the
  // half-committed drag silently resumes after the menu closes.
  const handleCanvasContextMenu = useCallback(
    (e: React.MouseEvent) => {
      if (pointerDragCandidateRef.current) {
        pointerDragCandidateRef.current = null;
        const ctx = buildToolContext();
        const cancellable = tool as CanvasTool & { cancelDrag?: (ctx: ToolContext) => boolean };
        if (cancellable.cancelDrag) {
          // SelectTool exposes an explicit cancel that restores originals
          // without clearing the selection (right-click preserves selection).
          cancellable.cancelDrag(ctx);
        } else if (tool.onKeyDown) {
          // Tools with an Escape handler: synthesize the same cancellation.
          tool.onKeyDown(new KeyboardEvent('keydown', { key: 'Escape' }), ctx);
        } else {
          // Drawing tools without key handling: drop the in-progress state.
          tool.reset();
        }
        requestRender();
      }
      handleContextMenu(e);
    },
    [tool, buildToolContext, requestRender, handleContextMenu],
  );

  const handleDoubleClick = useCallback(
    (e: React.MouseEvent) => {
      const me = buildMouseEvent(e);
      const ctx = buildToolContext();
      void (async () => {
        if (await beginTextEditFromDoubleClick(me, ctx)) return;
        if (!tool.onDoubleClick) return;
        tool.onDoubleClick(me, ctx);
      })();
    },
    [tool, buildMouseEvent, buildToolContext],
  );

  // Keyboard handlers for space (pan toggle) and tool key forwarding
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Commit text edit on Escape (textarea handles its own Escape, but this is a safety net).
      // Capture content and mode BEFORE committing so we can decide whether to delete.
      if (e.key === 'Escape' && useUiStore.getState().textEditObjectId) {
        const objId = useUiStore.getState().textEditObjectId!;
        const mode = useUiStore.getState().textEditMode;
        const content = getPendingContent();
        const shouldDelete = mode === 'new' && (content == null || content.trim() === '');
        void (async () => {
          const committed = await commitPendingTextEdit();
          if (!committed) return;
          useUiStore.setState({
            textEditObjectId: null,
            textEditClickPos: null,
            textEditMode: null,
            textEditCaretIndex: null,
          });
          if (shouldDelete) {
            await useProjectStore.getState().removeObject(objId);
          }
        })();
        return;
      }

      // When a brand-new edit session opens, object creation and textarea focus
      // can land a frame apart. Buffer printable keys during that gap so rapid
      // typing never loses its first character. Once focused, the textarea
      // handles input normally.
      if (useUiStore.getState().textEditObjectId) {
        const target = e.target as HTMLElement | null;
        const editorFocused = target?.classList?.contains('text-edit-overlay') ?? false;
        if (!editorFocused && useUiStore.getState().textEditMode === 'new' && !e.metaKey && !e.ctrlKey && !e.altKey) {
          const text = e.key === 'Enter' ? '\n' : e.key.length === 1 ? e.key : '';
          if (text) {
            e.preventDefault();
            e.stopPropagation();
            updatePendingContent(`${getPendingContent() ?? ''}${text}`);
          }
        }
        return;
      }

      if (e.key === 'Escape' && useCameraStore.getState().overlayAdjustMode) {
        const cameraDrag = cameraOverlayAdjustDragRef.current;
        if (cameraDrag) {
          useCameraStore
            .getState()
            .setDraftOverlayTransform(cameraDrag.startTransform, cameraDrag.preDragDirty);
          cameraOverlayAdjustDragRef.current = null;
          requestRender();
          return;
        }
        useCameraStore.getState().exitOverlayAdjust();
        requestRender();
        return;
      }

      if (e.code === 'Space' && !e.repeat) {
        spaceHeldRef.current = true;
      }

      // Cancel start-point pick mode on Escape
      if (e.key === 'Escape' && useUiStore.getState().pendingStartPointObjectId) {
        exitStartPointPickMode();
        setStartPointVertices([]);
        startPointHoveredRef.current = null;
        requestRender();
        return;
      }

      // Cancel guide-path pick mode on Escape — revert text to straight if no guide was set
      if (e.key === 'Escape' && useUiStore.getState().pendingGuidePathTextId) {
        void cancelPendingGuidePathSelection();
        return;
      }

      // Exit radius tool on Escape
      if (e.key === 'Escape' && useUiStore.getState().activeTool === 'radius') {
        // Persist radius value on tool deactivation
        const radiusVal = useUiStore.getState().radiusToolValue;
        if (radiusVal !== null) {
          void useAppStore.getState().updateSettings({ last_radius_mm: radiusVal });
        }
        useUiStore.getState().setActiveTool('select');
        return;
      }

      // Forward key events to active tool
      if (tool.onKeyDown) {
        const ctx = buildToolContext();
        tool.onKeyDown(e, ctx);
      }
    };
    const handleKeyUp = (e: KeyboardEvent) => {
      if (e.code === 'Space') {
        spaceHeldRef.current = false;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('keyup', handleKeyUp);
    const handleNodeImmediateAction = (event: Event) => {
      if (activeTool !== 'node') return;
      const action = (event as CustomEvent<NodeImmediateAction>).detail;
      nodeTool.performImmediateAction(action, buildToolContext());
    };
    const handleNodeEditAction = (event: Event) => {
      if (activeTool !== 'node') return;
      const action = (event as CustomEvent<NodeEditAction>).detail;
      nodeTool.performNodeAction(action, buildToolContext());
    };
    window.addEventListener('bb:node-immediate-action', handleNodeImmediateAction as EventListener);
    window.addEventListener('bb:node-edit-action', handleNodeEditAction as EventListener);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('keyup', handleKeyUp);
      window.removeEventListener('bb:node-immediate-action', handleNodeImmediateAction as EventListener);
      window.removeEventListener('bb:node-edit-action', handleNodeEditAction as EventListener);
    };
  }, [activeTool, buildToolContext, requestRender, tool]);

  // Reset tool when switching — also cancel any pending pick modes
  useEffect(() => {
    // Persist radius value when leaving radius tool via any tool switch
    if (prevToolRef.current === 'radius' && activeTool !== 'radius') {
      const rv = useUiStore.getState().radiusToolValue;
      if (rv !== null) {
        void useAppStore.getState().updateSettings({ last_radius_mm: rv });
      }
    }
    prevToolRef.current = activeTool;

    Object.values(TOOL_INSTANCES).forEach((t) => t.reset());
    geometrySnapMemoryRef.current = null;
    rulerDragAxisRef.current = null;
    setRulerGuidePreview(null);
    if (useUiStore.getState().pendingGuidePathTextId) {
      void cancelPendingGuidePathSelection();
    }
    if (useUiStore.getState().pendingStartPointObjectId) {
      useUiStore.getState().setPendingStartPoint(null);
    }
  }, [activeTool, workspaceMode]);

  useEffect(() => {
    if (activeTool !== 'warp') return;
    const warpTool = TOOL_INSTANCES.warp as WarpTool;
    // A slow pointer drag can trigger unrelated canvas rerenders. Never let
    // those effects discard the live mesh gesture before pointer-up commits it.
    if (warpTool.isDragging()) return;
    warpTool.reset();
    warpTool.prepareForSelection(buildToolContext());
    requestRender();
  }, [activeTool, meshDeformMode, selectedObjectIds, project, buildToolContext, requestRender]);

  useEffect(() => {
    if (workspaceMode === 'run' || activeTool !== 'node') return;
    void nodeTool.prepareForSelection(buildToolContext());
  }, [activeTool, selectedObjectIds, project, buildToolContext, workspaceMode]);

  // Tab tool: refresh markers when tool becomes active, selection changes, or geometry changes.
  // Clear cached markers when selection becomes empty or multi-select to avoid stale overlay.
  useEffect(() => {
    if (workspaceMode === 'run' || activeTool !== 'tabs') return;
    if (selectedObjectIds.length !== 1) {
      (TOOL_INSTANCES.tabs as TabTool).reset();
      requestRender();
      return;
    }
    void (TOOL_INSTANCES.tabs as TabTool)
      .refreshMarkers(selectedObjectIds[0])
      .then(() => requestRender());
  }, [activeTool, selectedObjectIds, project, requestRender, workspaceMode]);

  // Radius tool: refresh candidates when tool becomes active, selection changes, or geometry changes.
  useEffect(() => {
    if (workspaceMode === 'run' || activeTool !== 'radius') return;
    if (selectedObjectIds.length !== 1) {
      (TOOL_INSTANCES.radius as RadiusTool).reset();
      requestRender();
      return;
    }
    void (TOOL_INSTANCES.radius as RadiusTool)
      .refreshCandidates(selectedObjectIds[0])
      .then(() => requestRender());
  }, [activeTool, selectedObjectIds, project, requestRender, workspaceMode]);

  const tabbedObjectsKey = useMemo(() => {
    if (!project) return '';
    return project.objects
      .filter((o) => o.tabs && o.tabs.length > 0)
      .map((o) => {
        const b = o.bounds;
        const t = o.transform;
        return `${o.id}:${o.tabs!.map((ta) => `${ta.subpath_index}.${ta.position}`).join(',')}:${b.min.x},${b.min.y},${b.max.x},${b.max.y}:${t.a},${t.b},${t.c},${t.d},${t.tx},${t.ty}`;
      })
      .join('|');
  }, [project]);

  useEffect(() => {
    if (!tabbedObjectsKey || !project) {
      setPersistentTabMarkers([]);
      return;
    }
    let cancelled = false;
    const tabbedIds = project.objects.filter((o) => o.tabs && o.tabs.length > 0).map((o) => o.id);
    void Promise.all(
      tabbedIds.map((id) =>
        vectorService
          .resolveTabMarkers(id)
          .then((markers) => {
            const object = project.objects.find((candidate) => candidate.id === id);
            if (!object) return [];
            const resolvedBounds = {
              min: { ...object.bounds.min },
              max: { ...object.bounds.max },
            };
            const resolvedTransform = { ...object.transform };
            return markers.map((m) => ({
              worldX: m.worldX,
              worldY: m.worldY,
              objectId: id,
              resolvedBounds,
              resolvedTransform,
            }));
          }),
      ),
    ).then((results) => {
      if (!cancelled) setPersistentTabMarkers(results.flat());
    });
    return () => {
      cancelled = true;
    };
  }, [tabbedObjectsKey, project]);

  // Start-point pick mode: fetch vertices when object is set
  const pendingStartPoint = useUiStore((s) => s.pendingStartPointObjectId);
  useEffect(() => {
    if (!pendingStartPoint) {
      setStartPointVertices([]);
      startPointHoveredRef.current = null;
      return;
    }
    const object = project?.objects.find((candidate) => candidate.id === pendingStartPoint);
    if (!object || !isEffectiveVector(object, project?.objects ?? [])) {
      useUiStore.getState().setPendingStartPoint(null);
      setStartPointVertices([]);
      startPointHoveredRef.current = null;
      return;
    }
    let cancelled = false;
    void vectorService
      .getPathVertices(pendingStartPoint)
      .then((verts) => {
        if (!cancelled)
          setStartPointVertices(
            verts.map((v) => ({
              worldX: v.x,
              worldY: v.y,
              isStart: v.isStart,
              subpathIndex: v.subpathIndex,
              vertexIndex: v.vertexIndex,
              subpathClosed: v.subpathClosed,
            })),
          );
      })
      .catch(() => {
        if (!cancelled) setStartPointVertices([]);
      });
    return () => {
      cancelled = true;
    };
  }, [pendingStartPoint, project]);

  // Compute cursor style
  const pendingGuidePath = useUiStore((s) => s.pendingGuidePathTextId);
  const cursorStyle = useMemo(() => {
    if (isPanningRef.current || spaceHeldRef.current) return 'grabbing';
    if (cameraOverlayAdjustMode) {
      return cameraOverlayAdjustDragRef.current ? 'grabbing' : 'move';
    }
    if (pendingStartPoint || pendingGuidePath) return 'crosshair';
    if (workspaceMode !== 'run' && activeTool === 'text') return textBoxModeActive ? 'crosshair' : 'text';
    return tool.getCursor({ vp, readOnly: workspaceMode === 'run' } as ToolContext);
  }, [activeTool, cameraOverlayAdjustMode, tool, vp, pendingStartPoint, pendingGuidePath, textBoxModeActive, workspaceMode]);

  const handleArtLibraryCanvasDragOver = useCallback((e: ReactDragEvent<HTMLElement>) => {
    const liveDragState = resolveCanvasArtLibraryDragState({
      dragState: useArtLibraryStore.getState().dragState,
      dataTransfer: e.dataTransfer,
    });
    if (!liveDragState && !isArtLibraryDragDataTransfer(e.dataTransfer)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = getCanvasArtLibraryDropEffect(e.shiftKey);
    if (liveDragState) {
      setArtLibraryDragState(buildCanvasArtLibraryDragOverState(liveDragState, e.shiftKey));
    }
  }, [setArtLibraryDragState]);

  const handleArtLibraryCanvasDrop = useCallback((e: ReactDragEvent<HTMLElement>) => {
    const liveDragState = resolveCanvasArtLibraryDragState({
      dragState: useArtLibraryStore.getState().dragState,
      dataTransfer: e.dataTransfer,
    });
    if (!liveDragState || !overlayCanvasRef.current) return;
    e.preventDefault();
    const payload = buildCanvasArtLibraryDropPayload({
      dragState: liveDragState,
      clientX: e.clientX,
      clientY: e.clientY,
      canvasRect: overlayCanvasRef.current.getBoundingClientRect(),
      vp,
    });
    // Fire-and-forget is intentional here: overlapping drops serialize in the
    // backend, and the last completed insert owns the final selection state.
    void insertArtLibraryItem(
      payload.libraryId,
      payload.itemId,
      payload.world,
    );
    setArtLibraryDragState(null);
  }, [insertArtLibraryItem, setArtLibraryDragState, vp]);

  return (
    <div
      ref={containerRef}
      className="h-full w-full overflow-hidden relative"
      style={{ cursor: cursorStyle }}
      // Keep zoom on the shared canvas surface rather than the drawing canvas.
      // The inline text editor is layered above the canvas, so wheel events
      // originating over text must bubble here to retain normal canvas zoom.
      onWheel={handleWheel}
    >
      <canvas
        ref={baseCanvasRef}
        style={{
          width,
          height,
          display: 'block',
          position: 'absolute',
          inset: 0,
          pointerEvents: 'none',
          backgroundColor: theme.canvasBg,
        }}
      />
      <canvas
        ref={overlayCanvasRef}
        style={{ width, height, display: 'block', position: 'absolute', inset: 0, touchAction: 'none' }}
        // The overlay canvas covers the full workspace, so handling art-library
        // drops here avoids duplicate inserts from bubbling through the parent container.
        onDragOver={handleArtLibraryCanvasDragOver}
        onDrop={handleArtLibraryCanvasDrop}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onDoubleClick={handleDoubleClick}
        onPointerLeave={handleMouseLeave}
        onContextMenu={handleCanvasContextMenu}
      />
      <TextEditOverlay vp={vp} />
      {project && (
        <SelectionIsolationBreadcrumb
          path={selectionIsolationPath}
          objects={project.objects}
          onNavigate={(depth) => {
            setSelectionIsolationPath(selectionIsolationPath.slice(0, depth));
            selectObjects([]);
            requestRender();
          }}
          onExit={() => {
            setSelectionIsolationPath([]);
            selectObjects([]);
            requestRender();
          }}
        />
      )}
      {selectionPicker && project && (
        <SelectionPickerPopover
          x={selectionPicker.x}
          y={selectionPicker.y}
          objectIds={selectionPicker.objectIds}
          objects={project.objects}
          layers={project.layers}
          viewportWidth={width}
          viewportHeight={height}
          onSelect={(objectId) => {
            selectObjects([objectId]);
            setSelectionPicker(null);
            requestRender();
          }}
          onClose={() => setSelectionPicker(null)}
        />
      )}
      {menuState.visible && (
        <ContextMenu x={menuState.x} y={menuState.y} items={menuState.items} onClose={closeMenu} />
      )}
      {traceImageObjectId && (
        <TraceImageDialog objectId={traceImageObjectId} onClose={closeTraceDialog} />
      )}
      {adjustImageObjectId && (
        <AdjustImageDialog objectId={adjustImageObjectId} onClose={closeAdjustDialog} />
      )}
    </div>
  );
}
