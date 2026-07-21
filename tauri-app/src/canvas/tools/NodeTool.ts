import type { CanvasTool, CanvasMouseEvent, ToolContext } from './types';
import type { ToolOverlay } from '../CanvasRenderer';
import type { EditablePath, NodeBatchUpdate, NodeId, NodeSelectionTarget } from '../../types/vector';
import type { Point2D, Bounds, ProjectObject } from '../../types/project';
import { vectorService } from '../../services/vectorService';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { useUndoStore } from '../../stores/undoStore';
import { usePreviewStore } from '../../stores/previewStore';
import { worldToScreen } from '../ViewportTransform';
import { parsePathData, computePathBBox, mapPathCoordToBounds, type PathBBox } from '../drawObjects';
import { DRAG_THRESHOLD_PX, NODE_HIT_SIZE } from '../constants';
import { resolveCanvasPointerSnap } from '../pointerSnap';
import { hitTestPointAll } from '../hitTest';
import { wrapBackendError } from '../../i18n/errors';
import i18n from '../../i18n';

export type NodeImmediateAction = 'midpoint' | 'align' | 'trim' | 'extend' | 'close_open' | 'auto_join';

type DragPointSnapshot = {
  target: NodeSelectionTarget;
  world: Point2D;
};

type LoadEditablePathOptions = {
  preserveSelection?: boolean;
  selectTargets?: NodeSelectionTarget[];
  primaryTarget?: NodeSelectionTarget | null;
};

type NodeSelectionModifierMode = 'replace' | 'add' | 'toggle' | 'remove';

function nodeSelectionModifierMode(shiftKey: boolean, ctrlOrCmd: boolean): NodeSelectionModifierMode {
  if (shiftKey && ctrlOrCmd) return 'remove';
  if (ctrlOrCmd) return 'toggle';
  if (shiftKey) return 'add';
  return 'replace';
}

type NodeToolState =
  | { type: 'idle' }
  | {
      type: 'maybe-drag';
      target: NodeSelectionTarget;
      startScreen: Point2D;
      startWorld: Point2D;
      initialPoints: DragPointSnapshot[];
      excludedPoints: Point2D[];
    }
  | {
      type: 'maybe-drag-segment';
      segment: { nodeId: NodeId; t: number };
      startScreen: Point2D;
    }
  | {
      type: 'rubber-band';
      startScreen: Point2D;
      currentScreen: Point2D;
      selectionMode: NodeSelectionModifierMode;
      crossing: boolean;
    }
  | {
      type: 'dragging';
      target: NodeSelectionTarget;
      startWorld: Point2D;
      initialPoints: DragPointSnapshot[];
      excludedPoints: Point2D[];
      preferredTargetKey: string | null;
      mirroredTarget: NodeSelectionTarget | null;
    };

export class NodeTool implements CanvasTool {
  name = 'node';
  private state: NodeToolState = { type: 'idle' };
  private editablePaths: EditablePath[] = [];
  private objectId: string | null = null;
  private activeSubpathIdx: number | null = null;
  private objectBounds: Bounds | null = null;
  private pathBBox: PathBBox | null = null;
  private loadedSignature: string | null = null;
  /** Hovered segment for visual feedback in insert/delete_segment modes */
  private hoveredSegment: { nodeId: NodeId; t: number } | null = null;
  private hoveredEndpoint: NodeId | null = null;
  private joinTargetNodeId: NodeId | null = null;
  private selectedTargets: NodeSelectionTarget[] = [];
  private primaryTarget: NodeSelectionTarget | null = null;
  private pendingNodeCommit: Promise<void> | null = null;
  private loadRequestId = 0;
  private localNodeDirty = false;

  onMouseDown(e: CanvasMouseEvent, ctx: ToolContext): void {
    this.handleMouseDown(e, ctx, true);
  }

  private handleMouseDown(e: CanvasMouseEvent, ctx: ToolContext, allowObjectSwitch: boolean): void {
    const screenPt = { x: e.screenX, y: e.screenY };
    const subMode = useUiStore.getState().nodeSubMode;

    if (ctx.selectedObjectIds.length === 0) {
      ctx.setStatusMessage(i18n.t('canvas_status.select_vector_path_nodes'));
      return;
    }

    const objId = this.objectId && ctx.selectedObjectIds.includes(this.objectId)
      ? this.objectId
      : ctx.selectedObjectIds[0];
    const obj = ctx.objects.find((o) => o.id === objId);
    if (!obj) return;

    // Convert supported objects lazily, then replay this click
    if (obj.data.type !== 'vector_path') {
      void this.prepareForSelection(ctx).then(() => {
        if (this.objectId && this.editablePaths.length > 0) this.onMouseDown(e, ctx);
      });
      return;
    }

    // Load editable path if not already loaded, then replay this click
    if (this.objectId !== objId) {
      void this.prepareForSelection(ctx).then(() => {
        if (this.objectId === objId && this.editablePaths.length > 0) {
          this.handleMouseDown(e, ctx, false);
        }
      });
      return;
    }

    if (allowObjectSwitch && !this.hasEditableHitAt(screenPt, ctx)) {
      const switchTarget = this.findSelectedVectorObjectAtPoint(screenPt, ctx, objId);
      if (switchTarget) {
        void this.loadEditablePath(switchTarget.id, ctx).then(() => {
          if (this.objectId === switchTarget.id && this.editablePaths.length > 0) {
            this.handleMouseDown(e, ctx, false);
          }
        });
        return;
      }
    }

    switch (subMode) {
      case 'select':
        this.handleSelectMouseDown(screenPt, ctx, e.shiftKey, e.ctrlKey);
        break;
      case 'insert':
        this.handleInsertClick(screenPt, ctx);
        break;
      case 'insert_midpoint':
        this.handleInsertMidpointClick(screenPt, ctx);
        break;
      case 'delete_node':
        this.handleDeleteNodeClick(screenPt, ctx);
        break;
      case 'break':
        this.handleBreakClick(screenPt, ctx);
        break;
      case 'delete_segment':
        this.handleDeleteSegmentClick(screenPt, ctx);
        break;
      case 'to_line':
        this.handleToLineClick(screenPt, ctx);
        break;
      case 'to_smooth':
        this.handleToSmoothClick(screenPt, ctx);
        break;
      case 'to_corner':
        this.handleToCornerClick(screenPt, ctx);
        break;
      case 'align':
        this.handleAlignClick(screenPt, ctx);
        break;
      case 'trim':
        this.handleTrimClick(screenPt, ctx);
        break;
      case 'extend':
        this.handleExtendClick(screenPt, ctx);
        break;
      case 'close_open':
        this.handleCloseOpenClick(ctx);
        break;
      case 'auto_join':
        this.handleAutoJoinClick(ctx);
        break;
    }
  }

  onMouseMove(e: CanvasMouseEvent, ctx: ToolContext): void {
    const screenPt = { x: e.screenX, y: e.screenY };
    const rawWorld = { x: e.worldX, y: e.worldY };

    if (this.state.type === 'maybe-drag') {
      const dx = screenPt.x - this.state.startScreen.x;
      const dy = screenPt.y - this.state.startScreen.y;
      if (Math.sqrt(dx * dx + dy * dy) > DRAG_THRESHOLD_PX) {
        this.state = {
          type: 'dragging',
          target: this.state.target,
          startWorld: this.state.startWorld,
          initialPoints: this.state.initialPoints,
          excludedPoints: this.state.excludedPoints,
          preferredTargetKey: null,
          mirroredTarget: null,
        };
      }
    }

    if (this.state.type === 'maybe-drag-segment') {
      const dx = screenPt.x - this.state.startScreen.x;
      const dy = screenPt.y - this.state.startScreen.y;
      if (Math.sqrt(dx * dx + dy * dy) > DRAG_THRESHOLD_PX) {
        const dragTarget = this.beginSegmentCurveDrag(this.state.segment, rawWorld);
        if (dragTarget) {
          this.state = {
            type: 'dragging',
            target: dragTarget.target,
            startWorld: dragTarget.startWorld,
            initialPoints: dragTarget.initialPoints,
            excludedPoints: dragTarget.excludedPoints,
            preferredTargetKey: null,
            mirroredTarget: dragTarget.mirroredTarget,
          };
        } else {
          this.state = { type: 'idle' };
        }
      }
    }

    if (this.state.type === 'rubber-band') {
      this.state.currentScreen = screenPt;
      this.state.crossing = screenPt.x < this.state.startScreen.x;
      ctx.requestRender();
      return;
    }

    if (this.state.type === 'dragging') {
      const project = useProjectStore.getState().project;
      const snapResult = resolveCanvasPointerSnap({
        world: rawWorld,
        ctrlKey: e.ctrlKey,
        altKey: e.altKey,
        project,
        zoom: ctx.vp.zoom,
        snapEnabled: ctx.snapEnabled,
        gridVisible: true,
        effectiveSnapSpacing: ctx.gridSpacingMm,
        snapToObjects: ctx.snapToObjects,
        preferredTargetKey: this.state.preferredTargetKey,
        excludedPoints: this.state.excludedPoints,
      });
      this.state.preferredTargetKey = snapResult.nextPreferredTargetKey;
      const snappedWorld = snapResult.snapped;
      let deltaX = snappedWorld.x - this.state.startWorld.x;
      let deltaY = snappedWorld.y - this.state.startWorld.y;
      if (e.shiftKey) {
        const angle = Math.atan2(deltaY, deltaX);
        const snappedAngle = Math.round(angle / (Math.PI / 4)) * (Math.PI / 4);
        const length = Math.hypot(deltaX, deltaY);
        deltaX = Math.cos(snappedAngle) * length;
        deltaY = Math.sin(snappedAngle) * length;
      }

      for (const snapshot of this.state.initialPoints) {
        if (
          this.state.mirroredTarget &&
          this.targetKey(snapshot.target) === this.targetKey(this.state.mirroredTarget) &&
          this.state.target.kind === 'handle' &&
          snapshot.target.kind === 'handle'
        ) {
          const mirroredWorld = this.computeMirroredHandleWorld(
            this.state.target,
            { x: snappedWorld.x, y: snappedWorld.y },
            snapshot.target,
          );
          if (mirroredWorld) {
            this.moveHandleLocally(
              snapshot.target.nodeId,
              snapshot.target.handleType,
              mirroredWorld,
              true,
            );
          }
          continue;
        }
        if (snapshot.target.kind === 'node') {
          this.setNodeWorldPosition(
            snapshot.target.nodeId,
            { x: snapshot.world.x + deltaX, y: snapshot.world.y + deltaY },
          );
        } else {
          this.moveHandleLocally(
            snapshot.target.nodeId,
            snapshot.target.handleType,
            { x: snapshot.world.x + deltaX, y: snapshot.world.y + deltaY },
          );
        }
      }

      if (
        this.state.target.kind === 'node' &&
        this.isEndpointNode(this.state.target.nodeId)
      ) {
        this.joinTargetNodeId = this.findJoinCandidate(
          this.state.target.nodeId,
          this.getTargetWorld(this.state.target),
        );
      } else {
        this.joinTargetNodeId = null;
      }
      ctx.requestRender();
    }

    // Hover feedback for segment-targeting sub-modes and immediate hover actions
    const subMode = useUiStore.getState().nodeSubMode;
    this.hoveredEndpoint = this.hitTestNodes(screenPt, ctx);
    const segmentTargetModes = new Set([
      'select',
      'insert',
      'insert_midpoint',
      'delete_segment',
      'to_line',
      'to_smooth',
      'align',
      'trim',
    ]);
    if (segmentTargetModes.has(subMode)) {
      const hit = this.hitTestSegment(screenPt, ctx);
      const changed = hit?.nodeId.command_idx !== this.hoveredSegment?.nodeId.command_idx
        || hit?.nodeId.subpath_idx !== this.hoveredSegment?.nodeId.subpath_idx
        || Math.abs((hit?.t ?? -1) - (this.hoveredSegment?.t ?? -1)) > 1e-6;
      if (changed) {
        this.hoveredSegment = hit;
        ctx.requestRender();
      }
    } else if (this.hoveredSegment) {
      this.hoveredSegment = null;
      ctx.requestRender();
    }
  }

  onMouseUp(_e: CanvasMouseEvent, ctx: ToolContext): void {
    switch (this.state.type) {
      case 'maybe-drag':
      case 'maybe-drag-segment':
        this.state = { type: 'idle' };
        ctx.requestRender();
        return;
      case 'rubber-band': {
        const { startScreen, currentScreen, selectionMode } = this.state;
        this.state = { type: 'idle' };
        const moved = Math.hypot(
          currentScreen.x - startScreen.x,
          currentScreen.y - startScreen.y,
        ) > DRAG_THRESHOLD_PX;
        if (!moved) {
          if (selectionMode === 'replace') this.selectOnly(null);
          ctx.requestRender();
          return;
        }

        const hits = this.nodeTargetsInScreenRect(startScreen, currentScreen, ctx);
        this.applySelectionTargetsModifier(hits, selectionMode);
        ctx.requestRender();
        return;
      }
      case 'dragging': {
        const objectId = this.objectId;
        const joinTarget = this.joinTargetNodeId;
        const dragTarget = this.state.target;
        this.state = { type: 'idle' };
        this.joinTargetNodeId = null;
        if (!objectId) return;
        if (dragTarget.kind === 'node' && joinTarget && this.isEndpointNode(dragTarget.nodeId)) {
          const updates = this.buildBatchUpdates();
          const updateBeforeJoin = updates.length > 0
            ? vectorService.updateNodesBatch(objectId, updates)
            : Promise.resolve(null);
          const commit = updateBeforeJoin
            .then(async (updated) => {
              if (updated) {
                this.applyUpdatedObject(updated);
                this.localNodeDirty = false;
              }
              const joined = await vectorService.joinSubpaths(objectId, dragTarget.nodeId, joinTarget);
              this.applyUpdatedObject(joined);
              await useUndoStore.getState().refresh();
              await this.loadEditablePath(objectId, ctx, { preserveSelection: true });
            })
            .catch((err) => ctx.setStatusMessage(wrapBackendError(String(err))));
          this.trackNodeCommit(commit);
          return;
        }
        const updates = this.buildBatchUpdates();
        if (updates.length === 0) return;
        const commit = vectorService
          .updateNodesBatch(objectId, updates)
            .then(async (updated) => {
              this.applyUpdatedObject(updated);
              void useUndoStore.getState().refresh();
              await this.loadEditablePath(objectId, ctx, { preserveSelection: true });
              this.localNodeDirty = false;
            })
          .catch((err) => ctx.setStatusMessage(wrapBackendError(String(err))));
        this.trackNodeCommit(commit);
        return;
      }
      default:
        return;
    }
  }

  onKeyDown(e: KeyboardEvent, ctx: ToolContext): void {
    const subMode = useUiStore.getState().nodeSubMode;
    const setSubMode = useUiStore.getState().setNodeSubMode;
    const hasSelection = this.selectedTargets.length > 0 && this.objectId;

    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'a') {
      e.preventDefault();
      e.stopImmediatePropagation();
      this.selectTargets(this.allNodeTargets());
      ctx.requestRender();
      return;
    }

    // Sub-mode shortcuts — immediate action when a node is selected,
    // otherwise switch mode
    switch (e.key.toLowerCase()) {
      case 'escape':
        e.preventDefault();
        e.stopImmediatePropagation();
        setSubMode('select');
        this.selectOnly(null);
        this.state = { type: 'idle' };
        ctx.requestRender();
        return;
      case 'm':
        this.performImmediateAction('midpoint', ctx);
        return;
      case 'a':
        this.performImmediateAction('align', ctx);
        return;
      case 't':
        this.performImmediateAction('trim', ctx);
        return;
      case 'e':
        this.performImmediateAction('extend', ctx);
        return;
      case 'i':
        if (this.hoveredSegment) {
          this.insertNodeAtSegment(this.hoveredSegment, this.hoveredSegment.t, ctx);
        } else {
          setSubMode('insert');
        }
        return;
      case 'd':
        if (this.hoveredEndpoint) {
          this.deleteNodeById(this.hoveredEndpoint, ctx);
        } else if (this.hoveredSegment) {
          this.deleteSegmentByHit(this.hoveredSegment, ctx);
        } else if (hasSelection) {
          this.deleteSelectedNodes(ctx);
        } else {
          setSubMode('delete_node');
        }
        return;
      case 'b':
        if (this.hoveredEndpoint) {
          this.breakPathAtNode(this.hoveredEndpoint, ctx);
        } else if (hasSelection) {
          const nodeTarget = this.selectedTargets.find((target) => target.kind === 'node');
          if (nodeTarget) {
            this.breakPathAtNode(nodeTarget.nodeId, ctx);
          }
        } else {
          setSubMode('break');
        }
        return;
      case 'x':
        setSubMode('delete_segment');
        return;
      case 'l':
        if (this.hoveredSegment && !this.isStraightSegment(this.hoveredSegment.nodeId)) {
          this.convertSegmentToLine(this.hoveredSegment, ctx);
        } else if (hasSelection) {
          const nodeTarget = this.selectedTargets.find((target) => target.kind === 'node');
          if (nodeTarget) {
            this.convertNodeInboundSegmentToLine(nodeTarget.nodeId, ctx);
          }
        } else {
          setSubMode('to_line');
        }
        return;
      case 's':
        if (this.hoveredEndpoint) {
          this.setNodeTypeById(this.hoveredEndpoint, 'smooth', ctx);
        } else if (this.hoveredSegment && this.isStraightSegment(this.hoveredSegment.nodeId)) {
          this.convertSegmentToCurve(this.hoveredSegment, ctx);
        } else if (hasSelection) {
          const nodeTarget = this.selectedTargets.find((target) => target.kind === 'node');
          if (nodeTarget) {
            this.setNodeTypeById(nodeTarget.nodeId, 'smooth', ctx);
          }
        } else {
          setSubMode('to_smooth');
        }
        return;
      case 'c':
        if (this.hoveredEndpoint) {
          this.setNodeTypeById(this.hoveredEndpoint, 'corner', ctx);
        } else if (hasSelection) {
          const nodeTarget = this.selectedTargets.find((target) => target.kind === 'node');
          if (nodeTarget) {
            this.setNodeTypeById(nodeTarget.nodeId, 'corner', ctx);
          }
        } else {
          setSubMode('to_corner');
        }
        return;
    }

    // Delete/Backspace deletes node in select mode
    if (e.key === 'Delete' || e.key === 'Backspace') {
      if (subMode === 'select' && this.objectId) {
        e.preventDefault();
        e.stopImmediatePropagation();
        this.deleteSelectedNodes(ctx);
      }
      return;
    }

    if (this.objectId && this.selectedTargets.length > 0) {
      const ui = useUiStore.getState();
      const ctrlOrCmd = e.ctrlKey || e.metaKey;
      const step = ctrlOrCmd && e.shiftKey
        ? ui.nudgeStepFineMm / 10
        : ctrlOrCmd
        ? ui.nudgeStepFineMm
        : e.shiftKey
          ? ui.nudgeStepCoarseMm
          : ui.nudgeStepMm;
      let dx = 0;
      let dy = 0;
      switch (e.key) {
        case 'ArrowLeft':
          dx = -step;
          break;
        case 'ArrowRight':
          dx = step;
          break;
        case 'ArrowUp':
          dy = -step;
          break;
        case 'ArrowDown':
          dy = step;
          break;
        default:
          return;
      }
      e.preventDefault();
      for (const target of this.selectedTargets) {
        if (target.kind === 'node') {
          const world = this.getTargetWorld(target);
          if (world) {
            this.setNodeWorldPosition(target.nodeId, { x: world.x + dx, y: world.y + dy });
          }
        } else {
          const world = this.getTargetWorld(target);
          if (world) {
            this.moveHandleLocally(target.nodeId, target.handleType, { x: world.x + dx, y: world.y + dy });
          }
        }
      }
      const updates = this.buildBatchUpdates();
      const commit = vectorService
        .updateNodesBatch(this.objectId, updates)
        .then(async (updated) => {
          this.applyUpdatedObject(updated);
          void useUndoStore.getState().refresh();
          await this.loadEditablePath(this.objectId!, ctx, { preserveSelection: true });
          this.localNodeDirty = false;
        })
        .catch((err) => ctx.setStatusMessage(wrapBackendError(String(err))));
      this.trackNodeCommit(commit);
    }
  }

  getCursor(): string {
    const subMode = useUiStore.getState().nodeSubMode;
    switch (this.state.type) {
      case 'dragging':
        return 'move';
      case 'rubber-band':
        return 'crosshair';
      default:
        switch (subMode) {
          case 'insert':
          case 'insert_midpoint':
            return 'crosshair';
          case 'delete_node':
          case 'delete_segment':
          case 'to_line':
          case 'to_smooth':
          case 'to_corner':
          case 'break':
          case 'align':
          case 'trim':
          case 'extend':
            return 'pointer';
          default:
            return 'default';
        }
    }
  }

  getOverlay(): ToolOverlay {
    if (this.editablePaths.length > 0 && this.objectId) {
      // Refresh objectBounds from current store state so nodeToWorld stays
      // in sync when bounds change via the properties panel (Bug 2 fix)
      const project = useProjectStore.getState().project;
      const obj = project?.objects.find((o) => o.id === this.objectId);
      if (obj && obj.data.type === 'vector_path') {
        this.objectBounds = { min: { ...obj.bounds.min }, max: { ...obj.bounds.max } };
      }

      return {
        type: 'node-edit',
        paths: this.editablePaths,
        selectedTargets: this.selectedTargets,
        primaryTarget: this.primaryTarget,
        nodeToWorld: (pos: Point2D) => this.nodeToWorld(pos),
        objectId: this.objectId,
        suspendPreview:
          this.state.type === 'dragging' ||
          this.state.type === 'maybe-drag' ||
          this.state.type === 'rubber-band',
        hoveredSegment: this.hoveredSegment,
        hoveredEndpoint: this.hoveredEndpoint,
        joinTargetNodeId: this.joinTargetNodeId,
        selectionRect: this.state.type === 'rubber-band'
          ? {
              startScreen: this.state.startScreen,
              endScreen: this.state.currentScreen,
              crossing: this.state.crossing,
            }
          : null,
      };
    }
    return { type: 'none' };
  }

  reset(): void {
    this.state = { type: 'idle' };
    this.editablePaths = [];
    this.objectId = null;
    this.activeSubpathIdx = null;
    this.objectBounds = null;
    this.pathBBox = null;
    this.loadedSignature = null;
    this.hoveredSegment = null;
    this.hoveredEndpoint = null;
    this.joinTargetNodeId = null;
    this.selectedTargets = [];
    this.primaryTarget = null;
    this.loadRequestId++;
    this.localNodeDirty = false;
    useUiStore.getState().setNodeSubMode('select');
    useUiStore.getState().setNodeEditNodeCount(0);
  }

  /** Get the total number of editable nodes */
  getNodeCount(): number {
    let count = 0;
    for (const path of this.editablePaths) {
      count += path.nodes.length;
    }
    return count;
  }

  performImmediateAction(
    action: NodeImmediateAction,
    ctx: ToolContext,
  ): void {
    switch (action) {
      case 'midpoint':
        if (!this.objectId) return;
        if (!this.hoveredSegment) return;
        this.insertNodeAtSegment(this.hoveredSegment, 0.5, ctx);
        return;
      case 'align':
        if (!this.objectId) return;
        if (!this.hoveredSegment) return;
        this.alignSelectionToSegment(this.hoveredSegment, ctx);
        return;
      case 'trim':
        if (!this.objectId) return;
        if (!this.hoveredSegment) return;
        this.trimSegmentToIntersection(this.hoveredSegment, ctx);
        return;
      case 'extend':
        if (!this.objectId) return;
        if (!this.hoveredEndpoint || !this.isEndpointNode(this.hoveredEndpoint)) return;
        this.extendEndpointToIntersection(this.hoveredEndpoint, ctx);
        return;
      case 'close_open':
        this.handleCloseOpenClick(ctx);
        return;
      case 'auto_join':
        this.handleAutoJoinClick(ctx);
    }
  }

  // --- Private helpers ---

  /** Map a path_data-space position to world (bounds) space */
  private nodeToWorld(pos: Point2D): Point2D {
    if (!this.pathBBox || !this.objectBounds) return pos;
    const b = this.objectBounds;
    const boundsW = b.max.x - b.min.x;
    const boundsH = b.max.y - b.min.y;
    return mapPathCoordToBounds(pos.x, pos.y, this.pathBBox, b.min.x, b.min.y, boundsW, boundsH);
  }

  /** Map a world (bounds) space position back to path_data space */
  private worldToNode(pos: Point2D): Point2D {
    if (!this.pathBBox || !this.objectBounds) return pos;
    const b = this.objectBounds;
    const bbox = this.pathBBox;
    const boundsW = b.max.x - b.min.x;
    const boundsH = b.max.y - b.min.y;
    const nx = boundsW > 0 ? (pos.x - b.min.x) / boundsW * bbox.width + bbox.minX : bbox.minX + bbox.width / 2;
    const ny = boundsH > 0 ? (pos.y - b.min.y) / boundsH * bbox.height + bbox.minY : bbox.minY + bbox.height / 2;
    return { x: nx, y: ny };
  }

  async prepareForSelection(ctx: ToolContext): Promise<void> {
    if (ctx.selectedObjectIds.length === 0) {
      this.reset();
      ctx.setStatusMessage(i18n.t('canvas_status.select_vector_path_nodes'));
      return;
    }

    const objId = this.objectId && ctx.selectedObjectIds.includes(this.objectId)
      ? this.objectId
      : ctx.selectedObjectIds[0];
    const obj = useProjectStore.getState().project?.objects.find((o) => o.id === objId)
      ?? ctx.objects.find((o) => o.id === objId);
    if (!obj) {
      this.reset();
      return;
    }

    if (obj.locked) {
      this.reset();
      ctx.setStatusMessage(i18n.t('canvas_status.unlock_before_nodes'));
      return;
    }

    if (
      obj.data.type === 'shape' ||
      obj.data.type === 'polygon' ||
      obj.data.type === 'star' ||
      obj.data.type === 'text' ||
      (obj.data.type === 'vector_path' && !this.isIdentityTransform(obj.transform))
    ) {
      try {
        const updated = await vectorService.convertToPath(obj.id);
        this.applyUpdatedObject(updated);
      } catch (err) {
        this.reset();
        ctx.setStatusMessage(wrapBackendError(String(err)));
        return;
      }
    } else if (obj.data.type !== 'vector_path') {
      this.reset();
      ctx.setStatusMessage(i18n.t('canvas_status.nodes_vector_only'));
      return;
    }

    const currentObj = useProjectStore.getState().project?.objects.find((o) => o.id === objId)
      ?? ctx.objects.find((o) => o.id === objId);
    const currentSignature = currentObj ? this.objectSignature(currentObj) : null;

    if (
      this.objectId !== objId ||
      this.editablePaths.length === 0 ||
      this.loadedSignature !== currentSignature
    ) {
      const preserveSelection = this.objectId === objId && this.editablePaths.length > 0;
      await this.loadEditablePath(objId, ctx, { preserveSelection });
    }
  }

  private async loadEditablePath(
    objId: string,
    ctx: ToolContext,
    options: LoadEditablePathOptions = {},
  ): Promise<void> {
    const requestId = ++this.loadRequestId;
    const requestedTargets = options.selectTargets
      ? [...options.selectTargets]
      : options.preserveSelection
        ? [...this.selectedTargets]
        : [];
    const requestedPrimary = options.primaryTarget !== undefined
      ? options.primaryTarget
      : options.preserveSelection
        ? this.primaryTarget
        : null;

    try {
      const paths = await vectorService.getEditablePath(objId);
      if (requestId !== this.loadRequestId) return;
      this.objectId = objId;
      this.editablePaths = paths;
      this.state = { type: 'idle' };
      const nextSelection = this.normalizeSelectionForPaths(
        paths,
        requestedTargets,
        requestedPrimary,
      );
      this.selectedTargets = nextSelection.targets;
      this.primaryTarget = nextSelection.primaryTarget;
      this.joinTargetNodeId = null;

      // Store bounds and compute pathBBox from the editable path returned by
      // the backend. Bounds edits rebake vector path data server-side, and the
      // frontend project can briefly hold stale path_data while the fresh
      // editable nodes are already loaded.
      const obj = useProjectStore.getState().project?.objects.find((o) => o.id === objId)
        ?? ctx.objects.find((o) => o.id === objId);
      if (obj && obj.data.type === 'vector_path') {
        this.objectBounds = { min: { ...obj.bounds.min }, max: { ...obj.bounds.max } };
        this.pathBBox = this.computeEditablePathBBox(paths)
          ?? computePathBBox(parsePathData(obj.data.path_data));
        this.loadedSignature = this.objectSignature(obj);
      } else {
        this.loadedSignature = null;
      }

      useUiStore.getState().setNodeEditNodeCount(this.getNodeCount());
      ctx.requestRender();
    } catch (err) {
      if (requestId !== this.loadRequestId) return;
      ctx.setStatusMessage(wrapBackendError(String(err)));
    }
  }

  /** For the first node of a closed path (MoveTo at cmd 0), the inbound segment
   *  is the closing segment (Close command). Returns the correct command index. */
  private resolveInboundCmdIdx(nodeId: NodeId): number {
    if (nodeId.command_idx !== 0) return nodeId.command_idx;
    const path = this.editablePaths.find((p) =>
      p.nodes.some(
        (n) => n.id.subpath_idx === nodeId.subpath_idx && n.id.command_idx === nodeId.command_idx,
      ),
    );
    if (path?.closed && path.nodes.length > 0) {
      const lastNode = path.nodes[path.nodes.length - 1];
      return lastNode.id.command_idx + 1; // Close command index
    }
    return nodeId.command_idx;
  }

  private hitTestNodes(
    screenPt: Point2D,
    ctx: ToolContext,
  ): NodeId | null {
    const half = NODE_HIT_SIZE / 2;

    for (const path of this.editablePaths) {
      for (const node of path.nodes) {
        const worldPos = this.nodeToWorld(node.position);
        const screenPos = worldToScreen(worldPos, ctx.vp);

        if (
          screenPt.x >= screenPos.x - half &&
          screenPt.x <= screenPos.x + half &&
          screenPt.y >= screenPos.y - half &&
          screenPt.y <= screenPos.y + half
        ) {
          return node.id;
        }
      }
    }

    return null;
  }

  private hitTestHandles(
    screenPt: Point2D,
    nodeId: NodeId,
    ctx: ToolContext,
  ): 'in' | 'out' | null {
    const node = this.findNode(nodeId);
    if (!node) return null;

    const half = NODE_HIT_SIZE / 2;

    if (node.handle_in) {
      const handleWorld = this.nodeToWorld(node.handle_in);
      const handleScreen = worldToScreen(handleWorld, ctx.vp);
      if (
        screenPt.x >= handleScreen.x - half &&
        screenPt.x <= handleScreen.x + half &&
        screenPt.y >= handleScreen.y - half &&
        screenPt.y <= handleScreen.y + half
      ) {
        return 'in';
      }
    }

    if (node.handle_out) {
      const handleWorld = this.nodeToWorld(node.handle_out);
      const handleScreen = worldToScreen(handleWorld, ctx.vp);
      if (
        screenPt.x >= handleScreen.x - half &&
        screenPt.x <= handleScreen.x + half &&
        screenPt.y >= handleScreen.y - half &&
        screenPt.y <= handleScreen.y + half
      ) {
        return 'out';
      }
    }

    return null;
  }

  private hitTestHandlesAny(
    screenPt: Point2D,
    ctx: ToolContext,
  ): { nodeId: NodeId; handleType: 'in' | 'out' } | null {
    for (const path of this.editablePaths) {
      for (const node of path.nodes) {
        const handleHit = this.hitTestHandles(screenPt, node.id, ctx);
        if (handleHit) {
          return { nodeId: node.id, handleType: handleHit };
        }
      }
    }
    return null;
  }

  private hasEditableHitAt(screenPt: Point2D, ctx: ToolContext): boolean {
    return this.hitTestHandlesAny(screenPt, ctx) !== null ||
      this.hitTestNodes(screenPt, ctx) !== null ||
      this.hitTestSegment(screenPt, ctx) !== null;
  }

  private findSelectedVectorObjectAtPoint(
    screenPt: Point2D,
    ctx: ToolContext,
    currentObjectId: string,
  ): ProjectObject | null {
    const selectedVectors = ctx.objects.filter(
      (object) =>
        object.id !== currentObjectId &&
        ctx.selectedObjectIds.includes(object.id) &&
        object.data.type === 'vector_path',
    );
    if (selectedVectors.length === 0) return null;
    return hitTestPointAll(screenPt, selectedVectors, ctx.vp)[0] ?? null;
  }

  private findNode(nodeId: NodeId) {
    for (const path of this.editablePaths) {
      for (const node of path.nodes) {
        if (
          node.id.subpath_idx === nodeId.subpath_idx &&
          node.id.command_idx === nodeId.command_idx
        ) {
          return node;
        }
      }
    }
    return null;
  }

  private targetKey(target: NodeSelectionTarget): string {
    return target.kind === 'node'
      ? `node:${target.nodeId.subpath_idx}:${target.nodeId.command_idx}`
      : `handle:${target.nodeId.subpath_idx}:${target.nodeId.command_idx}:${target.handleType}`;
  }

  private nodeKey(nodeId: NodeId): string {
    return `node:${nodeId.subpath_idx}:${nodeId.command_idx}`;
  }

  private isTargetSelected(target: NodeSelectionTarget): boolean {
    const key = this.targetKey(target);
    return this.selectedTargets.some((entry) => this.targetKey(entry) === key);
  }

  private selectOnly(target: NodeSelectionTarget | null): void {
    this.selectedTargets = target ? [target] : [];
    this.primaryTarget = target;
  }

  private selectTargets(targets: NodeSelectionTarget[]): void {
    const normalized = this.normalizeSelectionForPaths(
      this.editablePaths,
      targets,
      targets[targets.length - 1] ?? null,
    );
    this.selectedTargets = normalized.targets;
    this.primaryTarget = normalized.primaryTarget;
  }

  private addSelectionTargets(targets: NodeSelectionTarget[]): void {
    this.selectTargets([...this.selectedTargets, ...targets]);
  }

  private applySelectionTargetModifier(target: NodeSelectionTarget, mode: NodeSelectionModifierMode): void {
    switch (mode) {
      case 'add':
        if (!this.isTargetSelected(target)) {
          this.addSelectionTargets([target]);
        }
        return;
      case 'toggle':
        this.toggleSelection(target);
        return;
      case 'remove': {
        const key = this.targetKey(target);
        this.selectTargets(this.selectedTargets.filter((entry) => this.targetKey(entry) !== key));
        return;
      }
      case 'replace':
      default:
        this.selectOnly(target);
    }
  }

  private applySelectionTargetsModifier(targets: NodeSelectionTarget[], mode: NodeSelectionModifierMode): void {
    switch (mode) {
      case 'add':
        this.addSelectionTargets(targets);
        return;
      case 'toggle': {
        const targetKeys = new Set(targets.map((target) => this.targetKey(target)));
        const selectedKeys = new Set(this.selectedTargets.map((target) => this.targetKey(target)));
        const kept = this.selectedTargets.filter((target) => !targetKeys.has(this.targetKey(target)));
        const added = targets.filter((target) => !selectedKeys.has(this.targetKey(target)));
        this.selectTargets([...kept, ...added]);
        return;
      }
      case 'remove': {
        const targetKeys = new Set(targets.map((target) => this.targetKey(target)));
        this.selectTargets(this.selectedTargets.filter((target) => !targetKeys.has(this.targetKey(target))));
        return;
      }
      case 'replace':
      default:
        this.selectTargets(targets);
    }
  }

  private normalizeSelectionForPaths(
    paths: EditablePath[],
    targets: NodeSelectionTarget[],
    primaryTarget: NodeSelectionTarget | null,
  ): { targets: NodeSelectionTarget[]; primaryTarget: NodeSelectionTarget | null } {
    const seen = new Set<string>();
    const validTargets = targets.filter((target) => {
      if (!this.targetExistsInPaths(paths, target)) return false;
      const key = this.targetKey(target);
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });

    if (primaryTarget && validTargets.some((target) => this.targetKey(target) === this.targetKey(primaryTarget))) {
      return { targets: validTargets, primaryTarget };
    }

    return {
      targets: validTargets,
      primaryTarget: validTargets[validTargets.length - 1] ?? null,
    };
  }

  private targetExistsInPaths(paths: EditablePath[], target: NodeSelectionTarget): boolean {
    const node = this.findNodeInPaths(paths, target.nodeId);
    if (!node) return false;
    if (target.kind === 'node') return true;
    return target.handleType === 'in'
      ? node.handle_in !== null
      : node.handle_out !== null;
  }

  private findNodeInPaths(paths: EditablePath[], nodeId: NodeId): EditablePath['nodes'][number] | null {
    for (const path of paths) {
      for (const node of path.nodes) {
        if (
          node.id.subpath_idx === nodeId.subpath_idx &&
          node.id.command_idx === nodeId.command_idx
        ) {
          return node;
        }
      }
    }
    return null;
  }

  private toggleSelection(target: NodeSelectionTarget): void {
    const key = this.targetKey(target);
    if (this.selectedTargets.some((entry) => this.targetKey(entry) === key)) {
      this.selectedTargets = this.selectedTargets.filter((entry) => this.targetKey(entry) !== key);
      if (this.primaryTarget && this.targetKey(this.primaryTarget) === key) {
        this.primaryTarget = this.selectedTargets[this.selectedTargets.length - 1] ?? null;
      }
      return;
    }
    this.selectedTargets = [...this.selectedTargets, target];
    this.primaryTarget = target;
  }

  private selectedNodeIds(): NodeId[] {
    return this.selectedTargets
      .filter((target): target is Extract<NodeSelectionTarget, { kind: 'node' }> => target.kind === 'node')
      .map((target) => target.nodeId);
  }

  private allNodeTargets(): NodeSelectionTarget[] {
    return this.editablePaths.flatMap((path) =>
      path.nodes.map((node) => ({ kind: 'node' as const, nodeId: node.id })),
    );
  }

  private isDeletingEveryEditableNode(nodeIds: NodeId[]): boolean {
    const allTargets = this.allNodeTargets();
    if (allTargets.length === 0 || nodeIds.length < allTargets.length) return false;

    const selectedNodeKeys = new Set(nodeIds.map((nodeId) => this.nodeKey(nodeId)));
    return allTargets.every((target) => selectedNodeKeys.has(this.nodeKey(target.nodeId)));
  }

  private nodeTargetsInScreenRect(
    startScreen: Point2D,
    endScreen: Point2D,
    ctx: ToolContext,
  ): NodeSelectionTarget[] {
    const minX = Math.min(startScreen.x, endScreen.x);
    const maxX = Math.max(startScreen.x, endScreen.x);
    const minY = Math.min(startScreen.y, endScreen.y);
    const maxY = Math.max(startScreen.y, endScreen.y);

    return this.editablePaths.flatMap((path) =>
      path.nodes.flatMap((node) => {
        const screenPos = worldToScreen(this.nodeToWorld(node.position), ctx.vp);
        if (
          screenPos.x < minX ||
          screenPos.x > maxX ||
          screenPos.y < minY ||
          screenPos.y > maxY
        ) {
          return [];
        }
        return [{ kind: 'node' as const, nodeId: node.id }];
      }),
    );
  }

  private getTargetWorld(target: NodeSelectionTarget): Point2D | null {
    const node = this.findNode(target.nodeId);
    if (!node) return null;
    if (target.kind === 'node') return this.nodeToWorld(node.position);
    const handlePos = target.handleType === 'in' ? node.handle_in : node.handle_out;
    return handlePos ? this.nodeToWorld(handlePos) : null;
  }

  private buildSelectedDragPoints(): DragPointSnapshot[] {
    return this.selectedTargets
      .map((target) => {
        const world = this.getTargetWorld(target);
        return world ? { target, world } : null;
      })
      .filter((entry): entry is DragPointSnapshot => entry !== null);
  }

  private buildBatchUpdates(): NodeBatchUpdate[] {
    return this.selectedTargets.flatMap<NodeBatchUpdate>((target) => {
      const node = this.findNode(target.nodeId);
      if (!node) return [];
      if (target.kind === 'node') {
        return [{
          node_id: target.nodeId,
          x: node.position.x,
          y: node.position.y,
          handle_type: null,
        }];
      }
      const handlePos = target.handleType === 'in' ? node.handle_in : node.handle_out;
      if (!handlePos) return [];
      return [{
        node_id: target.nodeId,
        x: handlePos.x,
        y: handlePos.y,
        handle_type: target.handleType,
      }];
    });
  }

  private setNodeWorldPosition(nodeId: NodeId, worldPos: Point2D): void {
    const node = this.findNode(nodeId);
    if (!node) return;
    const currentWorld = this.nodeToWorld(node.position);
    this.moveNodeLocally(nodeId, worldPos.x - currentWorld.x, worldPos.y - currentWorld.y);
  }

  private isEndpointNode(nodeId: NodeId): boolean {
    const path = this.editablePaths.find((entry) => entry.nodes.some((node) =>
      node.id.subpath_idx === nodeId.subpath_idx && node.id.command_idx === nodeId.command_idx,
    ));
    if (!path || path.closed) return false;
    return nodeId.command_idx === path.nodes[0]?.id.command_idx
      || nodeId.command_idx === path.nodes[path.nodes.length - 1]?.id.command_idx;
  }

  private findJoinCandidate(sourceNodeId: NodeId, sourceWorld: Point2D | null): NodeId | null {
    if (!sourceWorld) return null;
    let best: { nodeId: NodeId; dist: number } | null = null;
    for (const path of this.editablePaths) {
      for (const node of path.nodes) {
        if (
          node.id.subpath_idx === sourceNodeId.subpath_idx &&
          node.id.command_idx === sourceNodeId.command_idx
        ) {
          continue;
        }
        if (!this.isEndpointNode(node.id)) continue;
        const world = this.nodeToWorld(node.position);
        const dist = Math.hypot(world.x - sourceWorld.x, world.y - sourceWorld.y);
        if (dist > 0.5) continue;
        if (!best || dist < best.dist) {
          best = { nodeId: node.id, dist };
        }
      }
    }
    return best?.nodeId ?? null;
  }

  private isStraightSegment(nodeId: NodeId): boolean {
    const segment = this.getSegmentNodes({ nodeId, t: 0 });
    return Boolean(segment && segment.prevNode.handle_out === null && segment.currNode.handle_in === null);
  }

  private beginSegmentCurveDrag(
    segment: { nodeId: NodeId; t: number },
    rawWorld: Point2D,
  ): {
    target: NodeSelectionTarget;
    startWorld: Point2D;
    initialPoints: DragPointSnapshot[];
    excludedPoints: Point2D[];
    mirroredTarget: NodeSelectionTarget;
  } | null {
    const path = this.editablePaths.find((entry) =>
      entry.nodes.some(
        (candidate) =>
          candidate.id.subpath_idx === segment.nodeId.subpath_idx &&
          candidate.id.command_idx === segment.nodeId.command_idx,
      ),
    );
    if (!path) return null;
    const nodeIndex = path.nodes.findIndex(
      (candidate) =>
        candidate.id.subpath_idx === segment.nodeId.subpath_idx &&
        candidate.id.command_idx === segment.nodeId.command_idx,
    );

    let prevNode: typeof path.nodes[0] | null = null;
    let currNode: typeof path.nodes[0] | null = null;
    if (nodeIndex > 0) {
      prevNode = path.nodes[nodeIndex - 1];
      currNode = path.nodes[nodeIndex];
    } else if (path.closed && nodeIndex === -1 && path.nodes.length >= 2) {
      prevNode = path.nodes[path.nodes.length - 1];
      currNode = path.nodes[0];
    }
    if (!prevNode || !currNode) return null;

    const prevWorld = this.nodeToWorld(prevNode.position);
    const currWorld = this.nodeToWorld(currNode.position);
    const handleOutWorld = {
      x: prevWorld.x + (currWorld.x - prevWorld.x) / 3,
      y: prevWorld.y + (currWorld.y - prevWorld.y) / 3,
    };
    const handleInWorld = {
      x: prevWorld.x + ((currWorld.x - prevWorld.x) * 2) / 3,
      y: prevWorld.y + ((currWorld.y - prevWorld.y) * 2) / 3,
    };
    this.moveHandleLocally(prevNode.id, 'out', handleOutWorld, true);
    this.moveHandleLocally(currNode.id, 'in', handleInWorld, true);

    const drivenHandle: NodeSelectionTarget =
      segment.t <= 0.5
        ? { kind: 'handle', nodeId: prevNode.id, handleType: 'out' }
        : { kind: 'handle', nodeId: currNode.id, handleType: 'in' };
    const mirroredHandle: NodeSelectionTarget =
      drivenHandle.nodeId.command_idx === prevNode.id.command_idx
        ? { kind: 'handle', nodeId: currNode.id, handleType: 'in' }
        : { kind: 'handle', nodeId: prevNode.id, handleType: 'out' };

    this.selectedTargets = [drivenHandle, mirroredHandle];
    this.primaryTarget = drivenHandle;

    const drivenWorld = this.getTargetWorld(drivenHandle);
    const mirroredWorld = this.getTargetWorld(mirroredHandle);
    if (!drivenWorld || !mirroredWorld) return null;

    return {
      target: drivenHandle,
      startWorld: rawWorld,
      initialPoints: [
        { target: drivenHandle, world: drivenWorld },
        { target: mirroredHandle, world: mirroredWorld },
      ],
      excludedPoints: [drivenWorld],
      mirroredTarget: mirroredHandle,
    };
  }

  private computeMirroredHandleWorld(
    drivenTarget: NodeSelectionTarget,
    drivenWorld: Point2D,
    mirroredTarget: NodeSelectionTarget,
  ): Point2D | null {
    if (drivenTarget.kind !== 'handle' || mirroredTarget.kind !== 'handle') return null;
    const drivenNode = this.findNode(drivenTarget.nodeId);
    const mirroredNode = this.findNode(mirroredTarget.nodeId);
    if (!drivenNode || !mirroredNode) return null;
    const drivenNodeWorld = this.nodeToWorld(drivenNode.position);
    const mirroredNodeWorld = this.nodeToWorld(mirroredNode.position);
    const midpoint = {
      x: (drivenNodeWorld.x + mirroredNodeWorld.x) / 2,
      y: (drivenNodeWorld.y + mirroredNodeWorld.y) / 2,
    };
    return {
      x: midpoint.x - (drivenWorld.x - midpoint.x),
      y: midpoint.y - (drivenWorld.y - midpoint.y),
    };
  }

  private segmentScreenToWorld(
    hovered: { nodeId: NodeId; t: number },
    _ctx: ToolContext,
  ): Point2D {
    const segment = this.getSegmentNodes(hovered);
    if (!segment) return { x: 0, y: 0 };
    const { prevNode, currNode } = segment;
    const p0 = this.nodeToWorld(prevNode.position);
    const p3 = this.nodeToWorld(currNode.position);
    const u = 1 - hovered.t;
    if (currNode.handle_in || prevNode.handle_out) {
      const c1 = prevNode.handle_out ? this.nodeToWorld(prevNode.handle_out) : p0;
      const c2 = currNode.handle_in ? this.nodeToWorld(currNode.handle_in) : p3;
      return {
        x:
          u * u * u * p0.x +
          3 * u * u * hovered.t * c1.x +
          3 * u * hovered.t * hovered.t * c2.x +
          hovered.t * hovered.t * hovered.t * p3.x,
        y:
          u * u * u * p0.y +
          3 * u * u * hovered.t * c1.y +
          3 * u * hovered.t * hovered.t * c2.y +
          hovered.t * hovered.t * hovered.t * p3.y,
      };
    }
    return {
      x: p0.x + (p3.x - p0.x) * hovered.t,
      y: p0.y + (p3.y - p0.y) * hovered.t,
    };
  }

  private moveNodeLocally(nodeId: NodeId, dx: number, dy: number): void {
    const node = this.findNode(nodeId);
    if (!node) return;

    // Convert world-space delta to path_data-space delta using bounds/pathBBox ratio
    let pdx = dx;
    let pdy = dy;
    if (this.pathBBox && this.objectBounds) {
      const boundsW = this.objectBounds.max.x - this.objectBounds.min.x;
      const boundsH = this.objectBounds.max.y - this.objectBounds.min.y;
      pdx = boundsW > 0 ? dx / boundsW * this.pathBBox.width : 0;
      pdy = boundsH > 0 ? dy / boundsH * this.pathBBox.height : 0;
    }

    node.position.x += pdx;
    node.position.y += pdy;
    this.localNodeDirty = true;
    // Move handles with the node
    if (node.handle_in) {
      node.handle_in.x += pdx;
      node.handle_in.y += pdy;
    }
    if (node.handle_out) {
      node.handle_out.x += pdx;
      node.handle_out.y += pdy;
    }
  }

  private moveHandleLocally(
    nodeId: NodeId,
    handleType: 'in' | 'out',
    worldPos: Point2D,
    allowCreate = false,
  ): void {
    const node = this.findNode(nodeId);
    if (!node) return;

    // Convert world position to path_data space
    const pathPos = this.worldToNode(worldPos);
    if (handleType === 'in' && (node.handle_in || allowCreate)) {
      node.handle_in ??= { ...node.position };
      node.handle_in.x = pathPos.x;
      node.handle_in.y = pathPos.y;
      this.localNodeDirty = true;
    } else if (handleType === 'out' && (node.handle_out || allowCreate)) {
      node.handle_out ??= { ...node.position };
      node.handle_out.x = pathPos.x;
      node.handle_out.y = pathPos.y;
      this.localNodeDirty = true;
    }
  }

  private getSegmentNodes(segment: { nodeId: NodeId; t: number }): {
    path: EditablePath;
    prevNode: EditablePath['nodes'][number];
    currNode: EditablePath['nodes'][number];
  } | null {
    const path = this.editablePaths.find(
      (entry) => entry.nodes[0]?.id.subpath_idx === segment.nodeId.subpath_idx,
    );
    if (!path) return null;

    const nodeIndex = path.nodes.findIndex(
      (candidate) =>
        candidate.id.subpath_idx === segment.nodeId.subpath_idx &&
        candidate.id.command_idx === segment.nodeId.command_idx,
    );

    if (nodeIndex > 0) {
      return {
        path,
        prevNode: path.nodes[nodeIndex - 1],
        currNode: path.nodes[nodeIndex],
      };
    }

    if (nodeIndex === -1 && path.closed && path.nodes.length >= 2) {
      return {
        path,
        prevNode: path.nodes[path.nodes.length - 1],
        currNode: path.nodes[0],
      };
    }

    return null;
  }

  private getSegmentEndpointsWorld(segment: { nodeId: NodeId; t: number }): {
    start: Point2D;
    end: Point2D;
    midpoint: Point2D;
  } | null {
    const nodes = this.getSegmentNodes(segment);
    if (!nodes) return null;
    const start = this.nodeToWorld(nodes.prevNode.position);
    const end = this.nodeToWorld(nodes.currNode.position);
    return {
      start,
      end,
      midpoint: {
        x: (start.x + end.x) / 2,
        y: (start.y + end.y) / 2,
      },
    };
  }

  private insertNodeAtSegment(
    segment: { nodeId: NodeId; t: number },
    t: number,
    ctx: ToolContext,
  ): void {
    const objectId = this.objectId;
    if (!objectId) return;
    this.activeSubpathIdx = segment.nodeId.subpath_idx;

    vectorService
      .insertNode(objectId, segment.nodeId.subpath_idx, segment.nodeId.command_idx, t)
      .then((updated) => {
        this.applyUpdatedObject(updated);
        void useUndoStore.getState().refresh();
        void this.loadEditablePath(objectId, ctx, { preserveSelection: true });
      })
      .catch((err) => ctx.setStatusMessage(wrapBackendError(String(err))));
  }

  private deleteNodeById(nodeId: NodeId, ctx: ToolContext): void {
    const objectId = this.objectId;
    if (!objectId) return;
    if (this.isDeletingEveryEditableNode([nodeId])) {
      this.removeEditedObject(objectId, ctx);
      return;
    }
    this.activeSubpathIdx = nodeId.subpath_idx;

    vectorService
      .deleteNode(objectId, nodeId.subpath_idx, nodeId.command_idx)
      .then((updated) => {
        this.applyUpdatedObject(updated);
        this.state = { type: 'idle' };
        void useUndoStore.getState().refresh();
        void this.loadEditablePath(objectId, ctx);
      })
      .catch((err) => ctx.setStatusMessage(wrapBackendError(String(err))));
  }

  private deleteSelectedNodes(ctx: ToolContext): void {
    const objectId = this.objectId;
    if (!objectId) return;
    const nodeIds = this.selectedNodeIds();
    if (nodeIds.length === 0) return;

    if (this.isDeletingEveryEditableNode(nodeIds)) {
      this.removeEditedObject(objectId, ctx);
      return;
    }

    vectorService
      .deleteNodes(objectId, nodeIds)
      .then((updated) => {
        this.applyUpdatedObject(updated);
        this.selectOnly(null);
        this.state = { type: 'idle' };
        void useUndoStore.getState().refresh();
        void this.loadEditablePath(objectId, ctx);
      })
      .catch((err) => ctx.setStatusMessage(wrapBackendError(String(err))));
  }

  private removeEditedObject(objectId: string, ctx: ToolContext): void {
    const remove = useProjectStore
      .getState()
      .removeObject(objectId)
      .then(() => {
        this.reset();
        ctx.requestRender();
      })
      .catch((err) => ctx.setStatusMessage(wrapBackendError(String(err))));
    this.trackNodeCommit(remove);
  }

  private deleteSegmentByHit(segment: { nodeId: NodeId; t: number }, ctx: ToolContext): void {
    const objectId = this.objectId;
    if (!objectId) return;
    this.activeSubpathIdx = segment.nodeId.subpath_idx;

    const commit = vectorService
      .deleteSegment(objectId, segment.nodeId.subpath_idx, segment.nodeId.command_idx)
      .then(async (updated) => {
        this.applyUpdatedObject(updated);
        this.state = { type: 'idle' };
        await useUndoStore.getState().refresh();
        await this.loadEditablePath(objectId, ctx);
      })
      .catch((err) => ctx.setStatusMessage(wrapBackendError(String(err))));
    this.trackNodeCommit(commit);
  }

  private breakPathAtNode(nodeId: NodeId, ctx: ToolContext): void {
    const objectId = this.objectId;
    if (!objectId) return;
    this.activeSubpathIdx = nodeId.subpath_idx;

    vectorService
      .breakPathAtNode(objectId, nodeId.subpath_idx, nodeId.command_idx)
      .then((updated) => {
        this.applyUpdatedObject(updated);
        this.state = { type: 'idle' };
        void useUndoStore.getState().refresh();
        void this.loadEditablePath(objectId, ctx);
      })
      .catch((err) => ctx.setStatusMessage(wrapBackendError(String(err))));
  }

  private convertSegmentToLine(segment: { nodeId: NodeId; t: number }, ctx: ToolContext): void {
    const objectId = this.objectId;
    if (!objectId) return;
    this.activeSubpathIdx = segment.nodeId.subpath_idx;

    vectorService
      .convertSegmentToLine(objectId, segment.nodeId.subpath_idx, segment.nodeId.command_idx)
      .then((updated) => {
        this.applyUpdatedObject(updated);
        this.state = { type: 'idle' };
        void useUndoStore.getState().refresh();
        void this.loadEditablePath(objectId, ctx);
      })
      .catch((err) => ctx.setStatusMessage(wrapBackendError(String(err))));
  }

  private convertNodeInboundSegmentToLine(nodeId: NodeId, ctx: ToolContext): void {
    const cmdIdx = this.resolveInboundCmdIdx(nodeId);
    this.convertSegmentToLine({ nodeId: { ...nodeId, command_idx: cmdIdx }, t: 1 }, ctx);
  }

  private convertSegmentToCurve(segment: { nodeId: NodeId; t: number }, ctx: ToolContext): void {
    const objectId = this.objectId;
    if (!objectId) return;
    this.activeSubpathIdx = segment.nodeId.subpath_idx;
    const nodes = this.getSegmentNodes(segment);
    const targets: NodeSelectionTarget[] = nodes
      ? [
          { kind: 'node', nodeId: nodes.prevNode.id },
          { kind: 'node', nodeId: nodes.currNode.id },
        ]
      : [{ kind: 'node', nodeId: segment.nodeId }];

    vectorService
      .convertSegmentToCurve(objectId, segment.nodeId.subpath_idx, segment.nodeId.command_idx)
      .then((updated) => {
        this.applyUpdatedObject(updated);
        this.state = { type: 'idle' };
        void useUndoStore.getState().refresh();
        void this.loadEditablePath(objectId, ctx, {
          selectTargets: targets,
          primaryTarget: targets[targets.length - 1] ?? null,
        });
      })
      .catch((err) => ctx.setStatusMessage(wrapBackendError(String(err))));
  }

  private setNodeTypeById(
    nodeId: NodeId,
    nodeType: 'smooth' | 'corner',
    ctx: ToolContext,
  ): void {
    const objectId = this.objectId;
    if (!objectId) return;
    this.activeSubpathIdx = nodeId.subpath_idx;
    const target: NodeSelectionTarget = { kind: 'node', nodeId };

    vectorService
      .setNodeType(objectId, nodeId.subpath_idx, nodeId.command_idx, nodeType)
      .then((updated) => {
        this.applyUpdatedObject(updated);
        this.selectOnly(target);
        this.state = { type: 'idle' };
        void useUndoStore.getState().refresh();
        void this.loadEditablePath(objectId, ctx, {
          selectTargets: [target],
          primaryTarget: target,
        });
      })
      .catch((err) => ctx.setStatusMessage(wrapBackendError(String(err))));
  }

  private trimSegmentToIntersection(segment: { nodeId: NodeId; t: number }, ctx: ToolContext): void {
    const objectId = this.objectId;
    if (!objectId) return;
    const world = this.segmentScreenToWorld(segment, ctx);
    this.activeSubpathIdx = segment.nodeId.subpath_idx;

    vectorService
      .trimSegmentToIntersection(
        objectId,
        segment.nodeId.subpath_idx,
        segment.nodeId.command_idx,
        world.x,
        world.y,
      )
      .then((updated) => {
        this.applyUpdatedObject(updated);
        void useUndoStore.getState().refresh();
        void this.loadEditablePath(objectId, ctx);
      })
      .catch((err) => ctx.setStatusMessage(wrapBackendError(String(err))));
  }

  private extendEndpointToIntersection(nodeId: NodeId, ctx: ToolContext): void {
    const objectId = this.objectId;
    if (!objectId) return;
    this.activeSubpathIdx = nodeId.subpath_idx;

    vectorService
      .extendEndpointToIntersection(objectId, nodeId)
      .then((updated) => {
        this.applyUpdatedObject(updated);
        void useUndoStore.getState().refresh();
        void this.loadEditablePath(objectId, ctx);
      })
      .catch((err) => ctx.setStatusMessage(wrapBackendError(String(err))));
  }

  private alignSelectionToSegment(segment: { nodeId: NodeId; t: number }, ctx: ToolContext): void {
    const objectId = this.objectId;
    if (!objectId) return;
    if (!this.isStraightSegment(segment.nodeId)) {
      ctx.setStatusMessage(i18n.t('canvas_status.align_requires_straight'));
      return;
    }
    const endpoints = this.getSegmentEndpointsWorld(segment);
    if (!endpoints) {
      ctx.setStatusMessage(i18n.t('canvas_status.click_segment_not_endpoint'));
      return;
    }
    const angleDeg = Math.atan2(
      endpoints.end.y - endpoints.start.y,
      endpoints.end.x - endpoints.start.x,
    ) * 180 / Math.PI;
    const targetDeg = Math.round(angleDeg / 45) * 45;
    const deltaDeg = targetDeg - angleDeg;
    if (Math.abs(deltaDeg) < 0.05) {
      ctx.setStatusMessage(i18n.t('canvas_status.segment_already_aligned', { deg: targetDeg.toFixed(0) }));
      return;
    }
    const objectIds = ctx.selectedObjectIds.length > 0 ? ctx.selectedObjectIds : [objectId];

    useProjectStore.getState().rotateObjectsAndBakeActivePath(
      objectIds,
      deltaDeg,
      endpoints.midpoint,
      objectId,
    )
      .then(() => {
        ctx.setStatusMessage(i18n.t('canvas_status.aligned_selection', { deg: targetDeg.toFixed(0) }));
        void this.loadEditablePath(objectId, ctx);
      })
      .catch((err) => ctx.setStatusMessage(wrapBackendError(String(err))));
  }

  // --- Sub-mode handlers ---

  private handleSelectMouseDown(screenPt: Point2D, ctx: ToolContext, shiftKey: boolean, ctrlKey: boolean): void {
    const selectionMode = nodeSelectionModifierMode(shiftKey, ctrlKey);
    const handleHit = this.hitTestHandlesAny(screenPt, ctx);
    if (handleHit) {
      const target: NodeSelectionTarget = {
        kind: 'handle',
        nodeId: handleHit.nodeId,
        handleType: handleHit.handleType,
      };
      if (selectionMode !== 'replace') {
        this.applySelectionTargetModifier(target, selectionMode);
        this.state = { type: 'idle' };
        ctx.requestRender();
        return;
      }
      if (!this.isTargetSelected(target)) {
        this.selectOnly(target);
      } else {
        this.primaryTarget = target;
      }
      this.activeSubpathIdx = handleHit.nodeId.subpath_idx;
      this.state = {
        type: 'maybe-drag',
        target,
        startScreen: screenPt,
        startWorld: this.getTargetWorld(target) ?? { x: 0, y: 0 },
        initialPoints: this.buildSelectedDragPoints(),
        excludedPoints: [],
      };
      ctx.requestRender();
      return;
    }

    const hitNode = this.hitTestNodes(screenPt, ctx);
    if (hitNode) {
      const target: NodeSelectionTarget = { kind: 'node', nodeId: hitNode };
      if (selectionMode !== 'replace') {
        this.applySelectionTargetModifier(target, selectionMode);
        this.state = { type: 'idle' };
        ctx.requestRender();
        return;
      }
      if (!this.isTargetSelected(target)) {
        this.selectOnly(target);
      } else {
        this.primaryTarget = target;
      }
      this.activeSubpathIdx = hitNode.subpath_idx;
      this.state = {
        type: 'maybe-drag',
        target,
        startScreen: screenPt,
        startWorld: this.getTargetWorld(target) ?? { x: 0, y: 0 },
        initialPoints: this.buildSelectedDragPoints(),
        excludedPoints: [{ ...this.nodeToWorld(this.findNode(hitNode)?.position ?? { x: 0, y: 0 }) }],
      };
      ctx.requestRender();
      return;
    }

    const hitSegment = this.hitTestSegment(screenPt, ctx);
    if (hitSegment && this.isStraightSegment(hitSegment.nodeId)) {
      this.activeSubpathIdx = hitSegment.nodeId.subpath_idx;
      this.state = {
        type: 'maybe-drag-segment',
        segment: hitSegment,
        startScreen: screenPt,
      };
      return;
    }

    if (selectionMode === 'replace') this.selectOnly(null);
    this.state = {
      type: 'rubber-band',
      startScreen: screenPt,
      currentScreen: screenPt,
      selectionMode,
      crossing: false,
    };
    ctx.requestRender();
  }

  private handleInsertClick(screenPt: Point2D, ctx: ToolContext): void {
    const hit = this.hitTestSegment(screenPt, ctx);
    if (!hit) return;
    this.insertNodeAtSegment(hit, hit.t, ctx);
  }

  private handleInsertMidpointClick(screenPt: Point2D, ctx: ToolContext): void {
    const hit = this.hitTestSegment(screenPt, ctx);
    if (!hit) return;
    this.insertNodeAtSegment(hit, 0.5, ctx);
  }

  private handleDeleteNodeClick(screenPt: Point2D, ctx: ToolContext): void {
    const hitNode = this.hitTestNodes(screenPt, ctx);
    if (!hitNode) return;
    const target: NodeSelectionTarget = { kind: 'node', nodeId: hitNode };
    if (this.selectedNodeIds().length > 1 && this.isTargetSelected(target)) {
      this.deleteSelectedNodes(ctx);
      return;
    }
    this.deleteNodeById(hitNode, ctx);
  }

  private handleBreakClick(screenPt: Point2D, ctx: ToolContext): void {
    const hitNode = this.hitTestNodes(screenPt, ctx);
    if (!hitNode) return;
    this.breakPathAtNode(hitNode, ctx);
  }

  private handleDeleteSegmentClick(screenPt: Point2D, ctx: ToolContext): void {
    const hit = this.hitTestSegment(screenPt, ctx);
    if (!hit) return;
    this.deleteSegmentByHit(hit, ctx);
  }

  private handleToLineClick(screenPt: Point2D, ctx: ToolContext): void {
    const hitSegment = this.hitTestSegment(screenPt, ctx);
    if (hitSegment && !this.isStraightSegment(hitSegment.nodeId)) {
      this.convertSegmentToLine(hitSegment, ctx);
      return;
    }

    const hitNode = this.hitTestNodes(screenPt, ctx);
    if (!hitNode) return;
    this.convertNodeInboundSegmentToLine(hitNode, ctx);
  }

  private handleToSmoothClick(screenPt: Point2D, ctx: ToolContext): void {
    const hitSegment = this.hitTestSegment(screenPt, ctx);
    const hitNode = this.hitTestNodes(screenPt, ctx);

    if (!hitNode && hitSegment && this.isStraightSegment(hitSegment.nodeId)) {
      this.convertSegmentToCurve(hitSegment, ctx);
      return;
    }

    if (!hitNode) return;
    this.setNodeTypeById(hitNode, 'smooth', ctx);
  }

  private handleToCornerClick(screenPt: Point2D, ctx: ToolContext): void {
    const hitNode = this.hitTestNodes(screenPt, ctx);
    if (!hitNode) return;
    this.setNodeTypeById(hitNode, 'corner', ctx);
  }

  private handleAlignClick(screenPt: Point2D, ctx: ToolContext): void {
    const hit = this.hitTestSegment(screenPt, ctx);
    if (!hit) {
      ctx.setStatusMessage(i18n.t('canvas_status.click_segment_45'));
      return;
    }
    this.alignSelectionToSegment(hit, ctx);
  }

  private handleTrimClick(screenPt: Point2D, ctx: ToolContext): void {
    const hit = this.hitTestSegment(screenPt, ctx);
    if (!hit) return;
    this.trimSegmentToIntersection(hit, ctx);
  }

  private handleExtendClick(screenPt: Point2D, ctx: ToolContext): void {
    const hitNode = this.hitTestNodes(screenPt, ctx);
    if (!hitNode || !this.isEndpointNode(hitNode)) return;
    this.extendEndpointToIntersection(hitNode, ctx);
  }

  private handleCloseOpenClick(ctx: ToolContext): void {
    // One-shot action: reset mode immediately, not after async completion
    this.state = { type: 'idle' };
    useUiStore.getState().setNodeSubMode('select');

    if (!this.objectId || this.editablePaths.length === 0) {
      ctx.setStatusMessage(i18n.t('canvas_status.select_path_to_close'));
      return;
    }
    const objectId = this.objectId;

    const subpathIdx = this.resolveOpenSubpathForClose();
    if (subpathIdx === null) {
      ctx.setStatusMessage(i18n.t('canvas_status.no_open_paths'));
      ctx.requestRender();
      return;
    }

    vectorService
      .togglePathClosed(objectId, subpathIdx)
      .then((updated) => {
        this.applyUpdatedObject(updated);
        void useUndoStore.getState().refresh();
        void this.loadEditablePath(objectId, ctx);
      })
      .catch((err) => ctx.setStatusMessage(wrapBackendError(String(err))));
  }

  private resolveOpenSubpathForClose(): number | null {
    const candidateIndexes = [
      this.primaryTarget?.nodeId.subpath_idx,
      ...this.selectedTargets.map((target) => target.nodeId.subpath_idx),
      this.hoveredEndpoint?.subpath_idx,
      this.hoveredSegment?.nodeId.subpath_idx,
      this.activeSubpathIdx,
    ];
    const seen = new Set<number>();
    for (const idx of candidateIndexes) {
      if (idx === null || idx === undefined || seen.has(idx)) continue;
      seen.add(idx);
      if (this.isOpenSubpath(idx)) return idx;
    }

    for (let i = 0; i < this.editablePaths.length; i++) {
      if (!this.editablePaths[i]?.closed) {
        return this.editablePaths[i].nodes[0]?.id.subpath_idx ?? i;
      }
    }

    return null;
  }

  private isOpenSubpath(subpathIdx: number): boolean {
    const path = this.editablePaths.find(
      (candidate) => candidate.nodes.some((node) => node.id.subpath_idx === subpathIdx),
    ) ?? this.editablePaths[subpathIdx];
    return Boolean(path && !path.closed);
  }

  private handleAutoJoinClick(ctx: ToolContext): void {
    // One-shot action: reset mode immediately, not after async completion
    this.state = { type: 'idle' };
    useUiStore.getState().setNodeSubMode('select');

    const objectIds = ctx.selectedObjectIds.length > 0
      ? ctx.selectedObjectIds
      : this.objectId
        ? [this.objectId]
        : [];
    if (objectIds.length === 0) {
      ctx.setStatusMessage(i18n.t('canvas_status.select_paths_auto_join'));
      return;
    }

    void this.runAutoJoin(objectIds, ctx);
  }

  private async runAutoJoin(objectIds: string[], ctx: ToolContext): Promise<void> {
    try {
      const beforeNodeCount = this.objectId && objectIds.includes(this.objectId)
        ? this.getNodeCount()
        : null;
      const pendingCommit = this.pendingNodeCommit;
      if (pendingCommit) {
        await pendingCommit;
      }
      await this.flushLocalNodeEdits(ctx);

      const result = await useProjectStore.getState().closeAndJoin(
        objectIds,
        0.5,
        { warnIfOpen: false },
      );
      if (!result) {
        ctx.requestRender();
        return;
      }

      const projectState = useProjectStore.getState();
      const joinedId = projectState.selectedObjectIds[0];
      if (joinedId) {
        await this.loadEditablePath(joinedId, ctx);
      }
      const afterNodeCount = beforeNodeCount === null ? null : this.getNodeCount();
      ctx.setStatusMessage(
        result.fullyClosed
          ? 'Auto-Join complete'
          : beforeNodeCount !== null && afterNodeCount !== null && afterNodeCount >= beforeNodeCount
            ? 'Auto-Join found no endpoints within tolerance'
            : 'Auto-Join complete; endpoints remain open',
      );
      ctx.requestRender();
    } catch (err) {
      ctx.setStatusMessage(wrapBackendError(String(err)));
      ctx.requestRender();
    }
  }

  private async flushLocalNodeEdits(ctx: ToolContext): Promise<void> {
    if (!this.localNodeDirty || !this.objectId) return;
    const objectId = this.objectId;
    const updates = this.buildBatchUpdates();
    if (updates.length === 0) {
      this.localNodeDirty = false;
      return;
    }

    const updated = await vectorService.updateNodesBatch(objectId, updates);
    this.applyUpdatedObject(updated);
    await useUndoStore.getState().refresh();
    await this.loadEditablePath(objectId, ctx, { preserveSelection: true });
    this.localNodeDirty = false;
  }

  // --- Segment hit-testing ---

  /** Hit-test path segments. Returns the nearest segment within threshold. */
  private hitTestSegment(
    screenPt: Point2D,
    ctx: ToolContext,
  ): { nodeId: NodeId; t: number } | null {
    const threshold = NODE_HIT_SIZE;
    let bestNodeId: NodeId | null = null;
    let bestT = 0;
    let bestDist = Infinity;

    const consider = (nodeId: NodeId, t: number, dist: number) => {
      if (dist <= threshold && dist < bestDist) {
        bestNodeId = nodeId;
        bestT = t;
        bestDist = dist;
      }
    };

    for (const path of this.editablePaths) {
      for (let i = 1; i < path.nodes.length; i++) {
        const prevNode = path.nodes[i - 1];
        const currNode = path.nodes[i];

        const prevScreen = worldToScreen(this.nodeToWorld(prevNode.position), ctx.vp);
        const currScreen = worldToScreen(this.nodeToWorld(currNode.position), ctx.vp);

        const hasHandles = currNode.handle_in !== null || prevNode.handle_out !== null;

        if (hasHandles) {
          const p0 = prevScreen;
          const p1 = prevNode.handle_out
            ? worldToScreen(this.nodeToWorld(prevNode.handle_out), ctx.vp)
            : prevScreen;
          const p2 = currNode.handle_in
            ? worldToScreen(this.nodeToWorld(currNode.handle_in), ctx.vp)
            : currScreen;
          const p3 = currScreen;

          const result = this.nearestPointOnCubic(screenPt, p0, p1, p2, p3);
          consider(currNode.id, result.t, result.dist);
        } else {
          const result = this.nearestPointOnLine(screenPt, prevScreen, currScreen);
          consider(currNode.id, result.t, result.dist);
        }
      }

      // Closing segment for closed paths
      if (path.closed && path.nodes.length >= 2) {
        const lastNode = path.nodes[path.nodes.length - 1];
        const firstNode = path.nodes[0];
        const lastScreen = worldToScreen(this.nodeToWorld(lastNode.position), ctx.vp);
        const firstScreen = worldToScreen(this.nodeToWorld(firstNode.position), ctx.vp);

        const closeNodeId: NodeId = {
          subpath_idx: lastNode.id.subpath_idx,
          command_idx: lastNode.id.command_idx + 1,
        };

        const hasHandles = firstNode.handle_in !== null || lastNode.handle_out !== null;

        if (hasHandles) {
          const p0 = lastScreen;
          const p1 = lastNode.handle_out
            ? worldToScreen(this.nodeToWorld(lastNode.handle_out), ctx.vp)
            : lastScreen;
          const p2 = firstNode.handle_in
            ? worldToScreen(this.nodeToWorld(firstNode.handle_in), ctx.vp)
            : firstScreen;
          const p3 = firstScreen;
          const result = this.nearestPointOnCubic(screenPt, p0, p1, p2, p3);
          consider(closeNodeId, result.t, result.dist);
        } else {
          const result = this.nearestPointOnLine(screenPt, lastScreen, firstScreen);
          consider(closeNodeId, result.t, result.dist);
        }
      }
    }

    return bestNodeId ? { nodeId: bestNodeId, t: bestT } : null;
  }

  private nearestPointOnLine(
    pt: Point2D,
    a: Point2D,
    b: Point2D,
  ): { dist: number; t: number } {
    const dx = b.x - a.x;
    const dy = b.y - a.y;
    const lenSq = dx * dx + dy * dy;
    if (lenSq < 1e-10) {
      const d = Math.sqrt((pt.x - a.x) ** 2 + (pt.y - a.y) ** 2);
      return { dist: d, t: 0 };
    }
    let t = ((pt.x - a.x) * dx + (pt.y - a.y) * dy) / lenSq;
    t = Math.max(0, Math.min(1, t));
    const projX = a.x + t * dx;
    const projY = a.y + t * dy;
    const dist = Math.sqrt((pt.x - projX) ** 2 + (pt.y - projY) ** 2);
    return { dist, t };
  }

  private nearestPointOnCubic(
    pt: Point2D,
    p0: Point2D,
    p1: Point2D,
    p2: Point2D,
    p3: Point2D,
  ): { dist: number; t: number } {
    // Coarse pass: sample 20 points
    let bestT = 0;
    let bestDist = Infinity;
    const steps = 20;
    for (let i = 0; i <= steps; i++) {
      const t = i / steps;
      const pos = this.evalCubic(p0, p1, p2, p3, t);
      const d = Math.sqrt((pt.x - pos.x) ** 2 + (pt.y - pos.y) ** 2);
      if (d < bestDist) {
        bestDist = d;
        bestT = t;
      }
    }
    // Refine with ternary search around best t
    let lo = Math.max(0, bestT - 1 / steps);
    let hi = Math.min(1, bestT + 1 / steps);
    for (let iter = 0; iter < 10; iter++) {
      const tA = (2 * lo + hi) / 3;
      const tB = (lo + 2 * hi) / 3;
      const dA = this.distToCubicAt(pt, p0, p1, p2, p3, tA);
      const dB = this.distToCubicAt(pt, p0, p1, p2, p3, tB);
      if (dA < dB) {
        hi = tB;
      } else {
        lo = tA;
      }
    }
    const finalT = (lo + hi) / 2;
    const finalPos = this.evalCubic(p0, p1, p2, p3, finalT);
    const finalDist = Math.sqrt((pt.x - finalPos.x) ** 2 + (pt.y - finalPos.y) ** 2);
    return { dist: finalDist, t: finalT };
  }

  private evalCubic(p0: Point2D, p1: Point2D, p2: Point2D, p3: Point2D, t: number): Point2D {
    const u = 1 - t;
    return {
      x: u * u * u * p0.x + 3 * u * u * t * p1.x + 3 * u * t * t * p2.x + t * t * t * p3.x,
      y: u * u * u * p0.y + 3 * u * u * t * p1.y + 3 * u * t * t * p2.y + t * t * t * p3.y,
    };
  }

  private distToCubicAt(
    pt: Point2D,
    p0: Point2D,
    p1: Point2D,
    p2: Point2D,
    p3: Point2D,
    t: number,
  ): number {
    const pos = this.evalCubic(p0, p1, p2, p3, t);
    return Math.sqrt((pt.x - pos.x) ** 2 + (pt.y - pos.y) ** 2);
  }

  private computeEditablePathBBox(paths: EditablePath[]): PathBBox | null {
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;

    const include = (pt: Point2D) => {
      minX = Math.min(minX, pt.x);
      minY = Math.min(minY, pt.y);
      maxX = Math.max(maxX, pt.x);
      maxY = Math.max(maxY, pt.y);
    };

    const includeQuadratic = (p0: Point2D, p1: Point2D, p2: Point2D) => {
      for (let i = 0; i <= 32; i++) {
        const t = i / 32;
        const u = 1 - t;
        include({
          x: u * u * p0.x + 2 * u * t * p1.x + t * t * p2.x,
          y: u * u * p0.y + 2 * u * t * p1.y + t * t * p2.y,
        });
      }
    };

    const includeCubic = (p0: Point2D, p1: Point2D, p2: Point2D, p3: Point2D) => {
      for (let i = 0; i <= 48; i++) {
        include(this.evalCubic(p0, p1, p2, p3, i / 48));
      }
    };

    const includeSegment = (prev: EditablePath['nodes'][number], curr: EditablePath['nodes'][number]) => {
      if (prev.handle_out && curr.handle_in) {
        includeCubic(prev.position, prev.handle_out, curr.handle_in, curr.position);
      } else if (prev.handle_out || curr.handle_in) {
        const control = prev.handle_out ?? curr.handle_in;
        if (control) includeQuadratic(prev.position, control, curr.position);
      } else {
        include(prev.position);
        include(curr.position);
      }
    };

    for (const path of paths) {
      const { nodes } = path;
      if (nodes.length === 0) continue;
      include(nodes[0].position);
      for (let i = 1; i < nodes.length; i++) {
        includeSegment(nodes[i - 1], nodes[i]);
      }
      if (path.closed && nodes.length > 1) {
        includeSegment(nodes[nodes.length - 1], nodes[0]);
      }
    }

    if (!isFinite(minX)) return null;
    return { minX, minY, maxX, maxY, width: maxX - minX, height: maxY - minY };
  }

  private applyUpdatedObject(updated: ProjectObject): void {
    useProjectStore.setState((state) => {
      const project = state.project;
      if (!project) return state;
      return {
        project: {
          ...project,
          objects: project.objects.map((o) => (o.id === updated.id ? updated : o)),
          dirty: true,
        },
      };
    });
    usePreviewStore.getState().invalidate();
  }

  private trackNodeCommit(commit: Promise<void>): void {
    const tracked = commit.finally(() => {
      if (this.pendingNodeCommit === tracked) {
        this.pendingNodeCommit = null;
      }
    });
    this.pendingNodeCommit = tracked;
  }

  private objectSignature(obj: ProjectObject): string {
    return JSON.stringify({
      data: obj.data,
      bounds: obj.bounds,
      transform: obj.transform,
    });
  }

  private isIdentityTransform(transform: ProjectObject['transform']): boolean {
    return (
      transform.a === 1 &&
      transform.b === 0 &&
      transform.c === 0 &&
      transform.d === 1 &&
      transform.tx === 0 &&
      transform.ty === 0
    );
  }
}
