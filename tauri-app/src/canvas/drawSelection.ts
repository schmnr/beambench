import type { ProjectObject, Point2D, TransformLocks } from '../types/project';
import type { ViewportParams } from './ViewportTransform';
import type { HandleId } from '../types/canvas';
import type { EditablePath, NodeId, NodeSelectionTarget } from '../types/vector';
import type { CanvasTheme } from './constants';
import { worldToScreen } from './ViewportTransform';
import {
  buildPolygonPoints,
  buildStarPath,
  getShapeCommandBounds,
  type ShapeCmd,
} from './drawObjects';
import { computeVisualBoundsWorld, isRulerGuideObject } from './alignment';
import {
  SELECTION_DASH,
  HANDLE_SIZE,
  HANDLE_HIT_SIZE,
  ROTATION_CORNER_OFFSET,
  SHEAR_HANDLE_OFFSET,
  ROTATION_ARC_RADIUS,
  RUBBER_BAND_STROKE,
  RUBBER_BAND_FILL,
  SHAPE_PREVIEW_STROKE,
  SHAPE_PREVIEW_FILL,
  NODE_SIZE,
  NODE_FILL,
  NODE_STROKE,
  NODE_SELECTED_FILL,
  NODE_SELECTED_STROKE,
  HANDLE_POINT_SIZE,
  HANDLE_POINT_FILL,
  HANDLE_POINT_STROKE,
  HANDLE_LINE_STROKE,
  HANDLE_LINE_WIDTH,
  LINE_PREVIEW_STROKE,
  MEASURE_LINE_STROKE,
  MEASURE_TEXT_COLOR,
  CROSSING_RUBBER_BAND_FILL,
  CROSSING_RUBBER_BAND_STROKE,
} from './constants';

export interface HandleInfo {
  id: HandleId;
  screenX: number;
  screenY: number;
}

export interface MeshDeformHandleInfo {
  screen: Point2D;
  active?: boolean;
  hovered?: boolean;
}

const MIN_HANDLE_LAYOUT_SPAN_PX = HANDLE_HIT_SIZE * 2;

/** Draw selection highlight and handles for selected objects */
export function drawSelectionHighlight(
  ctx: CanvasRenderingContext2D,
  objects: ProjectObject[],
  vp: ViewportParams,
  theme: CanvasTheme,
  dashOffset?: number,
  transformLocks?: TransformLocks,
  allObjects?: ProjectObject[],
): void {
  const anyLocked = objects.some((o) => o.locked);

  for (const obj of objects) {
    // Draw selection outline from visual bounds (matches handle positions)
    const vb = computeVisualBoundsWorld(obj, allObjects);
    const vtl = worldToScreen(vb.min, vp);
    const vbr = worldToScreen(vb.max, vp);
    const vw = vbr.x - vtl.x;
    const vh = vbr.y - vtl.y;

    ctx.save();
    if (obj.locked) {
      ctx.globalAlpha = 0.5;
    }
    ctx.strokeStyle = theme.selectionStroke;
    ctx.lineWidth = 1;
    ctx.setLineDash(SELECTION_DASH);
    ctx.lineDashOffset = dashOffset ?? 0;
    ctx.strokeRect(vtl.x, vtl.y, vw, vh);
    ctx.setLineDash([]);
    ctx.restore();
  }

  // Draw handles around the combined bounding box if any objects selected.
  // Locked selections remain selectable for unlock, but never transformable.
  if (objects.length > 0 && !anyLocked) {
    const handles = getSelectionHandles(objects, vp, transformLocks, allObjects);
    for (const handle of handles) {
      drawHandle(ctx, handle, theme);
    }
  }
}

function drawHandle(ctx: CanvasRenderingContext2D, handle: HandleInfo, theme: CanvasTheme): void {
  const half = HANDLE_SIZE / 2;

  if (handle.id.startsWith('rotate_')) {
    // Rotation handle: arc with arrowhead
    drawRotationArrow(ctx, handle);
  } else {
    // Resize, center, shear handles: squares
    ctx.fillStyle = theme.handleFill;
    ctx.fillRect(handle.screenX - half, handle.screenY - half, HANDLE_SIZE, HANDLE_SIZE);
    ctx.strokeStyle = theme.handleStroke;
    ctx.lineWidth = 1.5;
    ctx.strokeRect(handle.screenX - half, handle.screenY - half, HANDLE_SIZE, HANDLE_SIZE);
  }
}

function drawRotationArrow(ctx: CanvasRenderingContext2D, handle: HandleInfo): void {
  const r = ROTATION_ARC_RADIUS;
  const PI = Math.PI;

  // 110° arc centered on the diagonal away from the selection box,
  // with arrowheads on both ends suggesting bidirectional rotation.
  const halfSweep = (110 / 2) * PI / 180; // 55° in radians
  let midAngle: number;
  switch (handle.id) {
    case 'rotate_nw': midAngle = 5 * PI / 4; break;   // upper-left diagonal
    case 'rotate_ne': midAngle = -PI / 4; break;       // upper-right diagonal
    case 'rotate_se': midAngle = PI / 4; break;         // lower-right diagonal
    case 'rotate_sw': midAngle = 3 * PI / 4; break;    // lower-left diagonal
    default: midAngle = 5 * PI / 4;
  }
  const startAngle = midAngle - halfSweep;
  const endAngle = midAngle + halfSweep;

  const cx = handle.screenX;
  const cy = handle.screenY;
  const headLen = 5;
  const spread = 0.45;

  ctx.save();
  ctx.strokeStyle = '#888888';
  ctx.lineWidth = 1.5;
  ctx.lineCap = 'round';

  // Draw the 110° arc (CW = default canvas direction)
  ctx.beginPath();
  ctx.arc(cx, cy, r, startAngle, endAngle);
  ctx.stroke();

  // Arrowhead at start end — tip at start, arrow points outward (CCW away from arc),
  // wings extend into the arc along CW tangent
  {
    const tipX = cx + r * Math.cos(startAngle);
    const tipY = cy + r * Math.sin(startAngle);
    const wingAngle = Math.atan2(Math.cos(startAngle), -Math.sin(startAngle));
    ctx.beginPath();
    ctx.moveTo(tipX, tipY);
    ctx.lineTo(tipX + headLen * Math.cos(wingAngle - spread), tipY + headLen * Math.sin(wingAngle - spread));
    ctx.moveTo(tipX, tipY);
    ctx.lineTo(tipX + headLen * Math.cos(wingAngle + spread), tipY + headLen * Math.sin(wingAngle + spread));
    ctx.stroke();
  }

  // Arrowhead at end — tip at end, arrow points outward (CW away from arc),
  // wings extend into the arc along CCW tangent
  {
    const tipX = cx + r * Math.cos(endAngle);
    const tipY = cy + r * Math.sin(endAngle);
    const wingAngle = Math.atan2(-Math.cos(endAngle), Math.sin(endAngle));
    ctx.beginPath();
    ctx.moveTo(tipX, tipY);
    ctx.lineTo(tipX + headLen * Math.cos(wingAngle - spread), tipY + headLen * Math.sin(wingAngle - spread));
    ctx.moveTo(tipX, tipY);
    ctx.lineTo(tipX + headLen * Math.cos(wingAngle + spread), tipY + headLen * Math.sin(wingAngle + spread));
    ctx.stroke();
  }

  ctx.restore();
}

export function drawMeshDeformOverlay(
  ctx: CanvasRenderingContext2D,
  handles: MeshDeformHandleInfo[],
  gridSize: number,
): void {
  if (handles.length !== gridSize * gridSize) return;

  ctx.save();
  ctx.strokeStyle = 'rgba(34, 192, 238, 0.75)';
  ctx.lineWidth = 1.25;
  ctx.setLineDash([5, 4]);

  for (let row = 0; row < gridSize; row++) {
    ctx.beginPath();
    for (let col = 0; col < gridSize; col++) {
      const point = handles[row * gridSize + col].screen;
      if (col === 0) ctx.moveTo(point.x, point.y);
      else ctx.lineTo(point.x, point.y);
    }
    ctx.stroke();
  }

  for (let col = 0; col < gridSize; col++) {
    ctx.beginPath();
    for (let row = 0; row < gridSize; row++) {
      const point = handles[row * gridSize + col].screen;
      if (row === 0) ctx.moveTo(point.x, point.y);
      else ctx.lineTo(point.x, point.y);
    }
    ctx.stroke();
  }

  ctx.setLineDash([]);
  for (const handle of handles) {
    const size = handle.active || handle.hovered ? HANDLE_SIZE + 3 : HANDLE_SIZE + 1;
    const half = size / 2;
    ctx.fillStyle = handle.active
      ? 'rgb(34, 192, 238)'
      : handle.hovered
        ? 'rgb(125, 211, 252)'
        : 'rgb(17, 24, 39)';
    ctx.strokeStyle = handle.active || handle.hovered ? '#ffffff' : 'rgb(34, 192, 238)';
    ctx.lineWidth = 1.5;
    ctx.fillRect(handle.screen.x - half, handle.screen.y - half, size, size);
    ctx.strokeRect(handle.screen.x - half, handle.screen.y - half, size, size);
  }

  ctx.restore();
}

/** Get handle positions for the combined bounding box of selected objects */
export function getSelectionHandles(
  objects: ProjectObject[],
  vp: ViewportParams,
  transformLocks?: TransformLocks,
  allObjects?: ProjectObject[],
): HandleInfo[] {
  if (objects.length === 0) return [];
  const singleGuideSelected = objects.length === 1 && isRulerGuideObject(objects[0]);

  // Compute axis-aligned screen bbox using visual bounds (tight, transform-aware)
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const obj of objects) {
    const vb = computeVisualBoundsWorld(obj, allObjects);
    const corners = [
      worldToScreen(vb.min, vp),
      worldToScreen({ x: vb.max.x, y: vb.min.y }, vp),
      worldToScreen(vb.max, vp),
      worldToScreen({ x: vb.min.x, y: vb.max.y }, vp),
    ];
    for (const c of corners) {
      minX = Math.min(minX, c.x);
      minY = Math.min(minY, c.y);
      maxX = Math.max(maxX, c.x);
      maxY = Math.max(maxY, c.y);
    }
  }

  ({ minX, minY, maxX, maxY } = expandDegenerateHandleBounds(minX, minY, maxX, maxY));

  const cx = (minX + maxX) / 2;
  const cy = (minY + maxY) / 2;

  const handles: HandleInfo[] = [];

  // Resize handles (8) — hidden when size_enabled === false
  if (!singleGuideSelected && transformLocks?.size_enabled !== false) {
    handles.push(
      { id: 'nw', screenX: minX, screenY: minY },
      { id: 'n', screenX: cx, screenY: minY },
      { id: 'ne', screenX: maxX, screenY: minY },
      { id: 'w', screenX: minX, screenY: cy },
      { id: 'e', screenX: maxX, screenY: cy },
      { id: 'sw', screenX: minX, screenY: maxY },
      { id: 's', screenX: cx, screenY: maxY },
      { id: 'se', screenX: maxX, screenY: maxY },
    );
  }

  // Center handle — hidden when move_enabled === false
  if (transformLocks?.move_enabled !== false) {
    handles.push({ id: 'center', screenX: cx, screenY: cy });
  }

  // Rotation handles (4 corners) — hidden when rotate_enabled === false
  if (!singleGuideSelected && transformLocks?.rotate_enabled !== false) {
    handles.push(
      { id: 'rotate_nw', screenX: minX - ROTATION_CORNER_OFFSET, screenY: minY - ROTATION_CORNER_OFFSET },
      { id: 'rotate_ne', screenX: maxX + ROTATION_CORNER_OFFSET, screenY: minY - ROTATION_CORNER_OFFSET },
      { id: 'rotate_sw', screenX: minX - ROTATION_CORNER_OFFSET, screenY: maxY + ROTATION_CORNER_OFFSET },
      { id: 'rotate_se', screenX: maxX + ROTATION_CORNER_OFFSET, screenY: maxY + ROTATION_CORNER_OFFSET },
    );
  }

  // Shear handles (2) — hidden when shear_enabled === false
  // Positioned at edge centers, outside the selection box.
  if (!singleGuideSelected && transformLocks?.shear_enabled !== false) {
    handles.push(
      { id: 'shear_n', screenX: cx, screenY: minY - SHEAR_HANDLE_OFFSET },
      { id: 'shear_e', screenX: maxX + SHEAR_HANDLE_OFFSET, screenY: cy },
    );
  }

  return handles;
}

function expandDegenerateHandleBounds(
  minX: number,
  minY: number,
  maxX: number,
  maxY: number,
): { minX: number; minY: number; maxX: number; maxY: number } {
  const width = maxX - minX;
  const height = maxY - minY;

  if (width >= MIN_HANDLE_LAYOUT_SPAN_PX && height >= MIN_HANDLE_LAYOUT_SPAN_PX) {
    return { minX, minY, maxX, maxY };
  }

  const cx = (minX + maxX) / 2;
  const cy = (minY + maxY) / 2;
  const halfWidth = Math.max(width, MIN_HANDLE_LAYOUT_SPAN_PX) / 2;
  const halfHeight = Math.max(height, MIN_HANDLE_LAYOUT_SPAN_PX) / 2;

  return {
    minX: cx - halfWidth,
    minY: cy - halfHeight,
    maxX: cx + halfWidth,
    maxY: cy + halfHeight,
  };
}

/** Draw a rubber-band selection rectangle */
export function drawRubberBand(
  ctx: CanvasRenderingContext2D,
  startScreen: Point2D,
  endScreen: Point2D,
  crossing?: boolean,
): void {
  const x = Math.min(startScreen.x, endScreen.x);
  const y = Math.min(startScreen.y, endScreen.y);
  const w = Math.abs(endScreen.x - startScreen.x);
  const h = Math.abs(endScreen.y - startScreen.y);

  ctx.fillStyle = crossing ? CROSSING_RUBBER_BAND_FILL : RUBBER_BAND_FILL;
  ctx.fillRect(x, y, w, h);

  ctx.strokeStyle = crossing ? CROSSING_RUBBER_BAND_STROKE : RUBBER_BAND_STROKE;
  ctx.lineWidth = 1;
  ctx.setLineDash(crossing ? [6, 3] : [4, 3]);
  ctx.strokeRect(x, y, w, h);
  ctx.setLineDash([]);
}

/** Draw a shape preview during creation (rect or ellipse tool) */
export function drawShapePreview(
  ctx: CanvasRenderingContext2D,
  startScreen: Point2D,
  endScreen: Point2D,
  kind: 'rectangle' | 'ellipse',
  cornerRadius = 0,
): void {
  const x = Math.min(startScreen.x, endScreen.x);
  const y = Math.min(startScreen.y, endScreen.y);
  const w = Math.abs(endScreen.x - startScreen.x);
  const h = Math.abs(endScreen.y - startScreen.y);

  ctx.fillStyle = SHAPE_PREVIEW_FILL;
  ctx.strokeStyle = SHAPE_PREVIEW_STROKE;
  ctx.lineWidth = 1.5;

  if (kind === 'rectangle') {
    if (cornerRadius > 0) {
      const r = Math.min(cornerRadius, w / 2, h / 2);
      ctx.beginPath();
      ctx.roundRect(x, y, w, h, r);
      ctx.fill();
      ctx.stroke();
    } else {
      ctx.fillRect(x, y, w, h);
      ctx.strokeRect(x, y, w, h);
    }
  } else {
    ctx.beginPath();
    ctx.ellipse(x + w / 2, y + h / 2, w / 2, h / 2, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();
  }
}

/** Draw node editing handles for VectorPath objects */
export function drawNodeHandles(
  ctx: CanvasRenderingContext2D,
  paths: EditablePath[],
  selectedTargets: NodeSelectionTarget[],
  vp: ViewportParams,
  nodeToWorld?: (pos: Point2D) => Point2D,
  joinTargetNodeId?: NodeId | null,
): void {
  const mapPos = nodeToWorld ?? ((p: Point2D) => p);
  const selectedKeys = new Set(
    selectedTargets.map((target) =>
      target.kind === 'node'
        ? `node:${target.nodeId.subpath_idx}:${target.nodeId.command_idx}`
        : `handle:${target.nodeId.subpath_idx}:${target.nodeId.command_idx}:${target.handleType}`,
    ),
  );

  for (const path of paths) {
    for (const node of path.nodes) {
      const screenPos = worldToScreen(mapPos(node.position), vp);

      const isNodeSelected = selectedKeys.has(`node:${node.id.subpath_idx}:${node.id.command_idx}`);
      const inHandleKey = `handle:${node.id.subpath_idx}:${node.id.command_idx}:in`;
      const outHandleKey = `handle:${node.id.subpath_idx}:${node.id.command_idx}:out`;
      const isInHandleSelected = selectedKeys.has(inHandleKey);
      const isOutHandleSelected = selectedKeys.has(outHandleKey);
      const isJoinTarget =
        joinTargetNodeId != null &&
        joinTargetNodeId.subpath_idx === node.id.subpath_idx &&
        joinTargetNodeId.command_idx === node.id.command_idx;

      // Draw handle arms (lines from node to control points)
      if (isNodeSelected || isInHandleSelected || isOutHandleSelected) {
        if (node.handle_in) {
          const handleScreen = worldToScreen(mapPos(node.handle_in), vp);
          drawHandleArm(
            ctx,
            screenPos,
            handleScreen,
            isInHandleSelected,
          );
        }
        if (node.handle_out) {
          const handleScreen = worldToScreen(mapPos(node.handle_out), vp);
          drawHandleArm(
            ctx,
            screenPos,
            handleScreen,
            isOutHandleSelected,
          );
        }
      }

      // Draw node square
      const half = NODE_SIZE / 2;
      ctx.fillStyle = isJoinTarget ? 'rgba(34, 197, 94, 0.8)' : isNodeSelected ? NODE_SELECTED_FILL : NODE_FILL;
      ctx.strokeStyle = isJoinTarget ? 'rgba(22, 163, 74, 1)' : isNodeSelected ? NODE_SELECTED_STROKE : NODE_STROKE;
      ctx.lineWidth = 1.5;
      ctx.fillRect(screenPos.x - half, screenPos.y - half, NODE_SIZE, NODE_SIZE);
      ctx.strokeRect(screenPos.x - half, screenPos.y - half, NODE_SIZE, NODE_SIZE);
    }
  }
}

function drawHandleArm(
  ctx: CanvasRenderingContext2D,
  nodeScreen: Point2D,
  handleScreen: Point2D,
  selected = false,
): void {
  // Line from node to handle point
  ctx.beginPath();
  ctx.moveTo(nodeScreen.x, nodeScreen.y);
  ctx.lineTo(handleScreen.x, handleScreen.y);
  ctx.strokeStyle = HANDLE_LINE_STROKE;
  ctx.lineWidth = HANDLE_LINE_WIDTH;
  ctx.stroke();

  // Handle control point (circle)
  const half = HANDLE_POINT_SIZE / 2;
  ctx.beginPath();
  ctx.arc(handleScreen.x, handleScreen.y, half, 0, Math.PI * 2);
  ctx.fillStyle = selected ? NODE_SELECTED_FILL : HANDLE_POINT_FILL;
  ctx.fill();
  ctx.strokeStyle = selected ? NODE_SELECTED_STROKE : HANDLE_POINT_STROKE;
  ctx.lineWidth = 1;
  ctx.stroke();
}

/** Draw a highlighted segment and insert preview dot */
export function drawHoveredSegment(
  ctx: CanvasRenderingContext2D,
  paths: EditablePath[],
  hoveredNodeId: NodeId,
  t: number,
  vp: ViewportParams,
  nodeToWorld?: (pos: Point2D) => Point2D,
): void {
  const mapPos = nodeToWorld ?? ((p: Point2D) => p);

  // Find the hovered node and its predecessor
  for (const path of paths) {
    const nodeIdx = path.nodes.findIndex(
      (n) => n.id.subpath_idx === hoveredNodeId.subpath_idx && n.id.command_idx === hoveredNodeId.command_idx,
    );

    let prevNode: typeof path.nodes[0] | null = null;
    let currNode: typeof path.nodes[0] | null = null;

    if (nodeIdx > 0) {
      prevNode = path.nodes[nodeIdx - 1];
      currNode = path.nodes[nodeIdx];
    } else if (nodeIdx === -1 && path.closed && path.nodes.length >= 2) {
      // Closing segment: hoveredNodeId targets Close command (cmd_idx > last node's cmd_idx)
      const firstNode = path.nodes[0];
      const lastNode = path.nodes[path.nodes.length - 1];
      if (
        firstNode &&
        lastNode &&
        firstNode.id.subpath_idx === hoveredNodeId.subpath_idx &&
        hoveredNodeId.command_idx === lastNode.id.command_idx + 1
      ) {
        prevNode = lastNode;
        currNode = firstNode;
      }
    }

    if (!prevNode || !currNode) continue;

    const prevScreen = worldToScreen(mapPos(prevNode.position), vp);
    const currScreen = worldToScreen(mapPos(currNode.position), vp);

    // Draw highlighted segment
    ctx.beginPath();
    ctx.moveTo(prevScreen.x, prevScreen.y);

    const hasHandles = currNode.handle_in !== null || prevNode.handle_out !== null;
    if (hasHandles) {
      const c1 = prevNode.handle_out ? worldToScreen(mapPos(prevNode.handle_out), vp) : prevScreen;
      const c2 = currNode.handle_in ? worldToScreen(mapPos(currNode.handle_in), vp) : currScreen;
      ctx.bezierCurveTo(c1.x, c1.y, c2.x, c2.y, currScreen.x, currScreen.y);
    } else {
      ctx.lineTo(currScreen.x, currScreen.y);
    }

    ctx.strokeStyle = 'rgba(59, 130, 246, 0.7)';
    ctx.lineWidth = 3;
    ctx.stroke();

    // Draw preview dot at parameter t
    const u = 1 - t;
    let dotX: number, dotY: number;
    if (hasHandles) {
      const c1 = prevNode.handle_out ? worldToScreen(mapPos(prevNode.handle_out), vp) : prevScreen;
      const c2 = currNode.handle_in ? worldToScreen(mapPos(currNode.handle_in), vp) : currScreen;
      dotX = u * u * u * prevScreen.x + 3 * u * u * t * c1.x + 3 * u * t * t * c2.x + t * t * t * currScreen.x;
      dotY = u * u * u * prevScreen.y + 3 * u * u * t * c1.y + 3 * u * t * t * c2.y + t * t * t * currScreen.y;
    } else {
      dotX = prevScreen.x + (currScreen.x - prevScreen.x) * t;
      dotY = prevScreen.y + (currScreen.y - prevScreen.y) * t;
    }

    ctx.beginPath();
    ctx.arc(dotX, dotY, 4, 0, Math.PI * 2);
    ctx.fillStyle = 'rgb(59, 130, 246)';
    ctx.fill();
    ctx.strokeStyle = 'white';
    ctx.lineWidth = 1.5;
    ctx.stroke();

    return;
  }
}

/** Draw a pen-tool bezier preview (cubic curves through placed points + handles) */
export function drawPenPreview(
  ctx: CanvasRenderingContext2D,
  screenPoints: { anchor: Point2D; handleIn?: Point2D; handleOut?: Point2D }[],
  currentScreen: Point2D,
  dragging: boolean,
  closed: boolean,
): void {
  if (screenPoints.length === 0) return;

  ctx.save();
  ctx.strokeStyle = LINE_PREVIEW_STROKE;
  ctx.lineWidth = 1.5;

  // 1. Draw committed cubic bezier segments
  if (screenPoints.length >= 2) {
    ctx.beginPath();
    ctx.moveTo(screenPoints[0].anchor.x, screenPoints[0].anchor.y);
    for (let i = 1; i < screenPoints.length; i++) {
      const prev = screenPoints[i - 1];
      const cur = screenPoints[i];
      const cp1 = prev.handleOut ?? prev.anchor;
      const cp2 = cur.handleIn ?? cur.anchor;
      ctx.bezierCurveTo(cp1.x, cp1.y, cp2.x, cp2.y, cur.anchor.x, cur.anchor.y);
    }
    if (closed) {
      const last = screenPoints[screenPoints.length - 1];
      const first = screenPoints[0];
      const cp1 = last.handleOut ?? last.anchor;
      const cp2 = first.handleIn ?? first.anchor;
      ctx.bezierCurveTo(cp1.x, cp1.y, cp2.x, cp2.y, first.anchor.x, first.anchor.y);
      ctx.closePath();
    }
    ctx.stroke();
  }

  // 2. Dashed preview curve from last point to cursor (only when placing, not closed)
  if (!closed && !dragging && screenPoints.length >= 1) {
    const last = screenPoints[screenPoints.length - 1];
    const cp1 = last.handleOut ?? last.anchor;
    ctx.setLineDash([6, 4]);
    ctx.beginPath();
    ctx.moveTo(last.anchor.x, last.anchor.y);
    ctx.bezierCurveTo(cp1.x, cp1.y, currentScreen.x, currentScreen.y, currentScreen.x, currentScreen.y);
    ctx.stroke();
    ctx.setLineDash([]);
  }

  // 3. Handle lines (thin gray lines from anchor to handle points)
  ctx.strokeStyle = HANDLE_LINE_STROKE;
  ctx.lineWidth = HANDLE_LINE_WIDTH;
  for (const pt of screenPoints) {
    if (pt.handleIn) {
      ctx.beginPath();
      ctx.moveTo(pt.anchor.x, pt.anchor.y);
      ctx.lineTo(pt.handleIn.x, pt.handleIn.y);
      ctx.stroke();
    }
    if (pt.handleOut) {
      ctx.beginPath();
      ctx.moveTo(pt.anchor.x, pt.anchor.y);
      ctx.lineTo(pt.handleOut.x, pt.handleOut.y);
      ctx.stroke();
    }
  }

  // 4. Anchor dots
  for (const pt of screenPoints) {
    ctx.beginPath();
    ctx.arc(pt.anchor.x, pt.anchor.y, 3, 0, Math.PI * 2);
    ctx.fillStyle = LINE_PREVIEW_STROKE;
    ctx.fill();
  }

  // 5. Handle diamonds (small unfilled diamonds at handle points)
  ctx.strokeStyle = HANDLE_POINT_STROKE;
  ctx.fillStyle = HANDLE_POINT_FILL;
  ctx.lineWidth = 1;
  const hs = HANDLE_POINT_SIZE / 2;
  for (const pt of screenPoints) {
    for (const h of [pt.handleIn, pt.handleOut]) {
      if (!h) continue;
      ctx.beginPath();
      ctx.moveTo(h.x, h.y - hs);
      ctx.lineTo(h.x + hs, h.y);
      ctx.lineTo(h.x, h.y + hs);
      ctx.lineTo(h.x - hs, h.y);
      ctx.closePath();
      ctx.fill();
      ctx.stroke();
    }
  }

  // 6. Close indicator — if cursor is near first point, draw circle around it
  if (!closed && screenPoints.length >= 2) {
    const first = screenPoints[0].anchor;
    const dx = currentScreen.x - first.x;
    const dy = currentScreen.y - first.y;
    if (Math.sqrt(dx * dx + dy * dy) < 8) {
      ctx.beginPath();
      ctx.arc(first.x, first.y, 6, 0, Math.PI * 2);
      ctx.strokeStyle = LINE_PREVIEW_STROKE;
      ctx.lineWidth = 1.5;
      ctx.stroke();
    }
  }

  ctx.restore();
}

/** Draw a trim preview overlay — dashed red polyline showing the segment to be removed */
export function drawTrimPreview(
  ctx: CanvasRenderingContext2D,
  screenPoints: Point2D[],
): void {
  if (screenPoints.length < 2) return;
  ctx.save();
  ctx.strokeStyle = '#ff4444';
  ctx.lineWidth = 2.5;
  ctx.setLineDash([6, 4]);
  ctx.globalAlpha = 0.8;
  ctx.beginPath();
  ctx.moveTo(screenPoints[0].x, screenPoints[0].y);
  for (let i = 1; i < screenPoints.length; i++) {
    ctx.lineTo(screenPoints[i].x, screenPoints[i].y);
  }
  ctx.stroke();
  ctx.restore();
}

/** Draw a measurement line with distance and angle labels */
export function drawMeasureLine(
  ctx: CanvasRenderingContext2D,
  startScreen: Point2D,
  endScreen: Point2D,
  distanceMm: number,
  angleDeg: number,
  displayUnit: 'mm' | 'inches' = 'mm',
): void {
  ctx.save();
  ctx.strokeStyle = MEASURE_LINE_STROKE;
  ctx.lineWidth = 1.5;
  ctx.setLineDash([6, 3]);

  ctx.beginPath();
  ctx.moveTo(startScreen.x, startScreen.y);
  ctx.lineTo(endScreen.x, endScreen.y);
  ctx.stroke();
  ctx.setLineDash([]);

  // Endpoints
  for (const pt of [startScreen, endScreen]) {
    ctx.beginPath();
    ctx.arc(pt.x, pt.y, 3, 0, Math.PI * 2);
    ctx.fillStyle = MEASURE_LINE_STROKE;
    ctx.fill();
  }

  // Label at midpoint
  const mx = (startScreen.x + endScreen.x) / 2;
  const my = (startScreen.y + endScreen.y) / 2;
  const distDisplay = displayUnit === 'inches' ? distanceMm / 25.4 : distanceMm;
  const distLabel = displayUnit === 'inches' ? `${distDisplay.toFixed(3)} in` : `${distDisplay.toFixed(1)} mm`;
  const label = `${distLabel}  ${angleDeg.toFixed(1)}°`;

  ctx.font = 'bold 11px system-ui, sans-serif';
  ctx.fillStyle = MEASURE_TEXT_COLOR;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'bottom';
  ctx.fillText(label, mx, my - 6);

  ctx.restore();
}

/** Draw a regular polygon preview inscribed in the bounding rectangle */
export function drawPolygonPreview(
  ctx: CanvasRenderingContext2D,
  startScreen: Point2D,
  endScreen: Point2D,
  sides: number,
): void {
  const width = Math.abs(endScreen.x - startScreen.x);
  const height = Math.abs(endScreen.y - startScreen.y);
  if (width < 1 || height < 1 || sides < 3) return;

  ctx.save();
  ctx.fillStyle = SHAPE_PREVIEW_FILL;
  ctx.strokeStyle = SHAPE_PREVIEW_STROKE;
  ctx.lineWidth = 1.5;

  const commands = buildPolygonPoints(sides, 50).map((pt, index) =>
    index === 0
      ? { type: 'move' as const, x: pt.x, y: pt.y }
      : { type: 'line' as const, x: pt.x, y: pt.y },
  );
  drawNormalizedShapePreviewPath(ctx, startScreen, endScreen, commands);

  ctx.restore();
}

export function drawStarPreview(
  ctx: CanvasRenderingContext2D,
  startScreen: Point2D,
  endScreen: Point2D,
  points: number,
  bulge = 0,
  ratio = 0.5,
  dualRadius = false,
  ratio2?: number,
): void {
  const width = Math.abs(endScreen.x - startScreen.x);
  const height = Math.abs(endScreen.y - startScreen.y);
  if (width < 1 || height < 1) return;

  const n = Math.max(3, points);
  const ratioA = Math.min(0.95, Math.max(0.05, ratio));
  const ratioB = Math.min(1, Math.max(0.05, ratio2 ?? 0.7));

  ctx.save();
  ctx.fillStyle = SHAPE_PREVIEW_FILL;
  ctx.strokeStyle = SHAPE_PREVIEW_STROKE;
  ctx.lineWidth = 1.5;
  const cmds = buildStarPath(n, bulge, ratioA, dualRadius, ratioB);
  drawNormalizedShapePreviewPath(ctx, startScreen, endScreen, cmds);

  ctx.restore();
}

function drawNormalizedShapePreviewPath(
  ctx: CanvasRenderingContext2D,
  startScreen: Point2D,
  endScreen: Point2D,
  cmds: ShapeCmd[],
): void {
  if (cmds.length < 3) return;

  const bbox = getShapeCommandBounds(cmds);
  if (!bbox) return;

  const pathW = bbox.maxX - bbox.minX;
  const pathH = bbox.maxY - bbox.minY;
  if (pathW <= 0 || pathH <= 0) return;

  const minX = Math.min(startScreen.x, endScreen.x);
  const minY = Math.min(startScreen.y, endScreen.y);
  const width = Math.abs(endScreen.x - startScreen.x);
  const height = Math.abs(endScreen.y - startScreen.y);
  const sx = width / pathW;
  const sy = height / pathH;
  const toScreen = (x: number, y: number) => ({
    x: minX + (x - bbox.minX) * sx,
    y: minY + (y - bbox.minY) * sy,
  });

  ctx.beginPath();
  for (const cmd of cmds) {
    const point = toScreen(cmd.x, cmd.y);
    if (cmd.type === 'move') {
      ctx.moveTo(point.x, point.y);
    } else if (cmd.type === 'line') {
      ctx.lineTo(point.x, point.y);
    } else {
      const control = toScreen(cmd.qx, cmd.qy);
      ctx.quadraticCurveTo(control.x, control.y, point.x, point.y);
    }
  }
  ctx.closePath();
  ctx.fill();
  ctx.stroke();
}

export function drawTabMarkers(
  ctx: CanvasRenderingContext2D,
  markers: { screen: Point2D; hovered: boolean }[],
): void {
  for (const m of markers) {
    ctx.save();
    const r = m.hovered ? 6 : 5;
    ctx.beginPath();
    ctx.arc(m.screen.x, m.screen.y, r, 0, Math.PI * 2);
    ctx.fillStyle = m.hovered ? '#FF6666' : '#FF3333';
    ctx.fill();
    ctx.strokeStyle = '#CC0000';
    ctx.lineWidth = 1.5;
    ctx.stroke();
    ctx.restore();
  }
}

/** Draw diamond markers for radius corner candidates.
 *  Already-filleted corners use a circle instead of a diamond. */
export function drawRadiusCornerMarkers(
  ctx: CanvasRenderingContext2D,
  markers: { screen: Point2D; hovered: boolean; alreadyFilleted: boolean }[],
): void {
  for (const m of markers) {
    ctx.save();
    const r = m.hovered ? 7 : 5;
    ctx.translate(m.screen.x, m.screen.y);
    ctx.beginPath();
    if (m.alreadyFilleted) {
      // Circle for already-filleted corners
      ctx.arc(0, 0, r, 0, Math.PI * 2);
      ctx.fillStyle = m.hovered ? '#FFA64D' : '#E88B3B';
      ctx.fill();
      ctx.strokeStyle = '#CF6A1A';
    } else {
      // Diamond for unfilleted corners
      ctx.rotate(Math.PI / 4);
      ctx.rect(-r, -r, r * 2, r * 2);
      ctx.fillStyle = m.hovered ? '#5C8AFF' : '#3B6FE8';
      ctx.fill();
      ctx.strokeStyle = '#1A4FCF';
    }
    ctx.lineWidth = 1.5;
    ctx.stroke();
    ctx.restore();
  }
}

const START_POINT_CUSTOM_COLOR = '#22c0ee';
const START_POINT_DEFAULT_COLOR = '#adb5bd';
const ARROW_HEAD_SIZE = 8;

/** Draw start-point pick overlay — vertices, start markers, and direction arrows */
export function drawStartPointOverlay(
  ctx: CanvasRenderingContext2D,
  vertices: { screenX: number; screenY: number; isStart: boolean; subpathClosed: boolean }[],
  subpathArrows: { fromX: number; fromY: number; toX: number; toY: number; isCustom: boolean }[],
  hoveredIndex: number | null,
): void {
  ctx.save();

  // 1. Draw vertices — only closed-subpath vertices are interactive
  const half = NODE_SIZE / 2;
  for (let i = 0; i < vertices.length; i++) {
    const v = vertices[i];

    // Skip open-subpath vertices entirely — they aren't selectable
    if (!v.subpathClosed) continue;

    const isHovered = hoveredIndex === i;

    if (v.isStart) {
      // Start vertex: larger filled circle
      const r = isHovered ? 6 : 5;
      ctx.beginPath();
      ctx.arc(v.screenX, v.screenY, r, 0, Math.PI * 2);
      ctx.fillStyle = START_POINT_CUSTOM_COLOR;
      ctx.fill();
      ctx.strokeStyle = '#fff';
      ctx.lineWidth = 1.5;
      ctx.stroke();
    } else {
      // Regular vertex: small square
      ctx.fillStyle = isHovered ? NODE_SELECTED_FILL : NODE_FILL;
      ctx.strokeStyle = isHovered ? NODE_SELECTED_STROKE : NODE_STROKE;
      ctx.lineWidth = 1.5;
      ctx.fillRect(v.screenX - half, v.screenY - half, NODE_SIZE, NODE_SIZE);
      ctx.strokeRect(v.screenX - half, v.screenY - half, NODE_SIZE, NODE_SIZE);
    }
  }

  // 2. Draw direction arrows for each closed subpath
  for (const arrow of subpathArrows) {
    const dx = arrow.toX - arrow.fromX;
    const dy = arrow.toY - arrow.fromY;
    const len = Math.sqrt(dx * dx + dy * dy);
    if (len < 2) continue;

    const nx = dx / len;
    const ny = dy / len;

    // Arrow color: blue if custom, gray if default
    ctx.strokeStyle = arrow.isCustom ? START_POINT_CUSTOM_COLOR : START_POINT_DEFAULT_COLOR;
    ctx.lineWidth = 2;

    // Draw line segment from start toward second vertex (capped at 20px)
    const drawLen = Math.min(len, 20);
    ctx.beginPath();
    ctx.moveTo(arrow.fromX, arrow.fromY);
    ctx.lineTo(arrow.fromX + nx * drawLen, arrow.fromY + ny * drawLen);
    ctx.stroke();

    // Arrowhead at the tip
    const tipX = arrow.fromX + nx * drawLen;
    const tipY = arrow.fromY + ny * drawLen;
    ctx.fillStyle = arrow.isCustom ? START_POINT_CUSTOM_COLOR : START_POINT_DEFAULT_COLOR;
    ctx.beginPath();
    ctx.moveTo(tipX, tipY);
    ctx.lineTo(tipX - nx * ARROW_HEAD_SIZE + ny * ARROW_HEAD_SIZE * 0.4, tipY - ny * ARROW_HEAD_SIZE - nx * ARROW_HEAD_SIZE * 0.4);
    ctx.lineTo(tipX - nx * ARROW_HEAD_SIZE - ny * ARROW_HEAD_SIZE * 0.4, tipY - ny * ARROW_HEAD_SIZE + nx * ARROW_HEAD_SIZE * 0.4);
    ctx.closePath();
    ctx.fill();
  }

  ctx.restore();
}
