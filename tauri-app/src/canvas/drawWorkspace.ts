import type { Workspace } from '../types/project';
import type { ViewportParams } from './ViewportTransform';
import { worldToScreen, worldToScreenDist } from './ViewportTransform';
import type { CanvasTheme } from './constants';
import {
  BED_LINE_WIDTH,
  MAJOR_GRID_INTERVAL,
  MIN_GRID_SCREEN_PX,
  MIN_INCH_GRID_SCREEN_PX,
  ORIGIN_COLOR,
  ORIGIN_SIZE,
  ORIGIN_LINE_WIDTH,
  RULER_SIZE,
} from './constants';

const MM_PER_INCH = 25.4;

/**
 * Choose an inch-based tick spacing (in mm) that looks reasonable at the
 * current zoom.  We try common fractions-of-an-inch: 1/16, 1/8, 1/4, 1/2,
 * 1, 2, 4, 6, 12 inches, and pick the smallest whose screen distance ≥
 * MIN_INCH_GRID_SCREEN_PX (higher than the mm threshold so lines are clearly
 * visible).  Grid drawing, ruler ticks, and snap all share this function so
 * they stay aligned.
 */
const INCH_INTERVALS_IN = [1/16, 1/8, 1/4, 1/2, 1, 2, 4, 6, 12];
const INCH_MAJOR_EVERY: Record<number, number> = {
  [1/16]: 16, [1/8]: 8, [1/4]: 4, [1/2]: 2,
  1: 1, 2: 6, 4: 3, 6: 2, 12: 1,
};

export function chooseInchInterval(zoom: number): { spacingMm: number; majorEvery: number } {
  for (const inches of INCH_INTERVALS_IN) {
    const mm = inches * MM_PER_INCH;
    if (worldToScreenDist(mm, zoom) >= MIN_INCH_GRID_SCREEN_PX) {
      return { spacingMm: mm, majorEvery: INCH_MAJOR_EVERY[inches] ?? MAJOR_GRID_INTERVAL };
    }
  }
  // Fallback: largest interval
  const last = INCH_INTERVALS_IN[INCH_INTERVALS_IN.length - 1];
  return { spacingMm: last * MM_PER_INCH, majorEvery: 1 };
}

export function drawBed(
  ctx: CanvasRenderingContext2D,
  workspace: Workspace,
  vp: ViewportParams,
  theme: CanvasTheme,
): void {
  const topLeft = worldToScreen({ x: 0, y: 0 }, vp);
  const bedW = worldToScreenDist(workspace.bed_width_mm, vp.zoom);
  const bedH = worldToScreenDist(workspace.bed_height_mm, vp.zoom);

  ctx.fillStyle = theme.bedFill;
  ctx.fillRect(topLeft.x, topLeft.y, bedW, bedH);

  ctx.strokeStyle = theme.bedStroke;
  ctx.lineWidth = BED_LINE_WIDTH;
  ctx.strokeRect(topLeft.x, topLeft.y, bedW, bedH);
}

export function drawGrid(
  ctx: CanvasRenderingContext2D,
  workspace: Workspace,
  vp: ViewportParams,
  gridSpacingMm: number,
  theme: CanvasTheme,
  displayUnit: 'mm' | 'inches' = 'mm',
): void {
  // In inch mode, use inch-based intervals that match the ruler ticks
  let spacing: number;
  let majorEvery: number;
  if (displayUnit === 'inches') {
    const chosen = chooseInchInterval(vp.zoom);
    spacing = chosen.spacingMm;
    majorEvery = chosen.majorEvery;
  } else {
    spacing = gridSpacingMm;
    majorEvery = MAJOR_GRID_INTERVAL;
  }

  const screenSpacing = worldToScreenDist(spacing, vp.zoom);

  // Don't draw if lines would be too close together
  if (screenSpacing < MIN_GRID_SCREEN_PX) return;

  const bedTopLeft = worldToScreen({ x: 0, y: 0 }, vp);
  const bedW = worldToScreenDist(workspace.bed_width_mm, vp.zoom);
  const bedH = worldToScreenDist(workspace.bed_height_mm, vp.zoom);

  ctx.save();
  ctx.beginPath();
  ctx.rect(bedTopLeft.x, bedTopLeft.y, bedW, bedH);
  ctx.clip();

  ctx.lineWidth = 0.5;

  // Vertical lines
  const xCount = Math.floor(workspace.bed_width_mm / spacing);
  for (let i = 1; i <= xCount; i++) {
    const screenX = bedTopLeft.x + i * screenSpacing;
    ctx.strokeStyle = i % majorEvery === 0 ? theme.gridLineMajor : theme.gridLine;
    ctx.beginPath();
    ctx.moveTo(screenX, bedTopLeft.y);
    ctx.lineTo(screenX, bedTopLeft.y + bedH);
    ctx.stroke();
  }

  // Horizontal lines
  const yCount = Math.floor(workspace.bed_height_mm / spacing);
  for (let i = 1; i <= yCount; i++) {
    const screenY = bedTopLeft.y + i * screenSpacing;
    ctx.strokeStyle = i % majorEvery === 0 ? theme.gridLineMajor : theme.gridLine;
    ctx.beginPath();
    ctx.moveTo(bedTopLeft.x, screenY);
    ctx.lineTo(bedTopLeft.x + bedW, screenY);
    ctx.stroke();
  }

  ctx.restore();
}

export function drawOrigin(
  ctx: CanvasRenderingContext2D,
  workspace: Workspace,
  vp: ViewportParams,
): void {
  const originWorld = workspace.origin === 'bottom_left'
    ? { x: 0, y: workspace.bed_height_mm }
    : { x: 0, y: 0 };
  const origin = worldToScreen(originWorld, vp);
  const half = ORIGIN_SIZE;

  ctx.strokeStyle = ORIGIN_COLOR;
  ctx.lineWidth = ORIGIN_LINE_WIDTH;

  // Horizontal line
  ctx.beginPath();
  ctx.moveTo(origin.x - half, origin.y);
  ctx.lineTo(origin.x + half, origin.y);
  ctx.stroke();

  // Vertical line
  ctx.beginPath();
  ctx.moveTo(origin.x, origin.y - half);
  ctx.lineTo(origin.x, origin.y + half);
  ctx.stroke();
}

/** Compute tick positions for a ruler axis. Exported for testing. */
export function computeRulerTicks(
  gridSpacingMm: number,
  axisLengthMm: number,
  vp: ViewportParams,
  axis: 'x' | 'y',
  displayUnit: 'mm' | 'inches' = 'mm',
  workspaceOrigin: Workspace['origin'] = 'top_left',
): { screenPos: number; label: string; isMajor: boolean }[] {
  if (displayUnit === 'inches') {
    const { spacingMm, majorEvery } = chooseInchInterval(vp.zoom);
    const count = Math.floor(axisLengthMm / spacingMm);
    const ticks: { screenPos: number; label: string; isMajor: boolean }[] = [];

    for (let i = 0; i <= count; i++) {
      const axisMm = i * spacingMm;
      const worldMm =
        axis === 'y' && workspaceOrigin === 'bottom_left'
          ? axisLengthMm - axisMm
          : axisMm;
      const worldPt = axis === 'x' ? { x: worldMm, y: 0 } : { x: 0, y: worldMm };
      const screenPt = worldToScreen(worldPt, vp);
      const screenPos = axis === 'x' ? screenPt.x : screenPt.y;
      const isMajor = i % majorEvery === 0;
      if (isMajor) {
        const inchVal = axisMm / MM_PER_INCH;
        const label = Number.isInteger(inchVal) ? String(inchVal) : inchVal.toFixed(2);
        ticks.push({ screenPos, label, isMajor });
      } else {
        ticks.push({ screenPos, label: '', isMajor });
      }
    }
    return ticks;
  }

  // --- mm mode: use grid spacing directly ---
  const screenSpacing = worldToScreenDist(gridSpacingMm, vp.zoom);
  if (screenSpacing < MIN_GRID_SCREEN_PX) return [];

  const count = Math.floor(axisLengthMm / gridSpacingMm);
  const ticks: { screenPos: number; label: string; isMajor: boolean }[] = [];

  for (let i = 0; i <= count; i++) {
    const axisVal = i * gridSpacingMm;
    const worldVal =
      axis === 'y' && workspaceOrigin === 'bottom_left'
        ? axisLengthMm - axisVal
        : axisVal;
    const worldPt = axis === 'x' ? { x: worldVal, y: 0 } : { x: 0, y: worldVal };
    const screenPt = worldToScreen(worldPt, vp);
    const screenPos = axis === 'x' ? screenPt.x : screenPt.y;
    const isMajor = i % MAJOR_GRID_INTERVAL === 0;
    if (isMajor) {
      ticks.push({ screenPos, label: String(axisVal), isMajor });
    } else {
      ticks.push({ screenPos, label: '', isMajor });
    }
  }

  return ticks;
}

export function drawRulers(
  ctx: CanvasRenderingContext2D,
  workspace: Workspace,
  vp: ViewportParams,
  gridSpacingMm: number,
  theme: CanvasTheme,
  displayUnit: 'mm' | 'inches' = 'mm',
): void {
  const R = RULER_SIZE;

  ctx.save();

  // --- Corner square ---
  ctx.fillStyle = theme.rulerBg;
  ctx.fillRect(0, 0, R, R);

  // --- Top ruler ---
  ctx.fillRect(R, 0, vp.canvasWidth - R, R);

  const xTicks = computeRulerTicks(
    gridSpacingMm,
    workspace.bed_width_mm,
    vp,
    'x',
    displayUnit,
    workspace.origin,
  );
  ctx.strokeStyle = theme.rulerTick;
  ctx.fillStyle = theme.rulerText;
  ctx.font = '9px system-ui, sans-serif';
  ctx.textBaseline = 'top';
  ctx.textAlign = 'center';
  ctx.lineWidth = 1;

  for (const tick of xTicks) {
    if (tick.screenPos < R) continue;
    const tickH = tick.isMajor ? R * 0.6 : R * 0.35;
    ctx.beginPath();
    ctx.moveTo(tick.screenPos, R);
    ctx.lineTo(tick.screenPos, R - tickH);
    ctx.stroke();

    if (tick.label) {
      ctx.fillText(tick.label, tick.screenPos, 2);
    }
  }

  // --- Left ruler ---
  ctx.fillStyle = theme.rulerBg;
  ctx.fillRect(0, R, R, vp.canvasHeight - R);

  const yTicks = computeRulerTicks(
    gridSpacingMm,
    workspace.bed_height_mm,
    vp,
    'y',
    displayUnit,
    workspace.origin,
  );
  ctx.strokeStyle = theme.rulerTick;
  ctx.fillStyle = theme.rulerText;

  for (const tick of yTicks) {
    if (tick.screenPos < R) continue;
    const tickW = tick.isMajor ? R * 0.6 : R * 0.35;
    ctx.beginPath();
    ctx.moveTo(R, tick.screenPos);
    ctx.lineTo(R - tickW, tick.screenPos);
    ctx.stroke();

    if (tick.label) {
      ctx.save();
      ctx.translate(R / 2, tick.screenPos);
      ctx.rotate(-Math.PI / 2);
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(tick.label, 0, 0);
      ctx.restore();
    }
  }

  // --- Bottom ruler ---
  ctx.fillStyle = theme.rulerBg;
  ctx.fillRect(R, vp.canvasHeight - R, vp.canvasWidth - R, R);

  ctx.strokeStyle = theme.rulerTick;
  ctx.fillStyle = theme.rulerText;
  ctx.textBaseline = 'bottom';
  ctx.textAlign = 'center';

  for (const tick of xTicks) {
    if (tick.screenPos < R) continue;
    const tickH = tick.isMajor ? R * 0.6 : R * 0.35;
    ctx.beginPath();
    ctx.moveTo(tick.screenPos, vp.canvasHeight - R);
    ctx.lineTo(tick.screenPos, vp.canvasHeight - R + tickH);
    ctx.stroke();

    if (tick.label) {
      ctx.fillText(tick.label, tick.screenPos, vp.canvasHeight - 2);
    }
  }

  // --- Right ruler ---
  ctx.fillStyle = theme.rulerBg;
  ctx.fillRect(vp.canvasWidth - R, R, R, vp.canvasHeight - R);

  ctx.strokeStyle = theme.rulerTick;
  ctx.fillStyle = theme.rulerText;

  for (const tick of yTicks) {
    if (tick.screenPos < R) continue;
    const tickW = tick.isMajor ? R * 0.6 : R * 0.35;
    ctx.beginPath();
    ctx.moveTo(vp.canvasWidth - R, tick.screenPos);
    ctx.lineTo(vp.canvasWidth - R + tickW, tick.screenPos);
    ctx.stroke();

    if (tick.label) {
      ctx.save();
      ctx.translate(vp.canvasWidth - R / 2, tick.screenPos);
      ctx.rotate(Math.PI / 2);
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(tick.label, 0, 0);
      ctx.restore();
    }
  }

  // --- Additional corner squares ---
  ctx.fillStyle = theme.rulerBg;
  ctx.fillRect(vp.canvasWidth - R, 0, R, R);           // Top-right
  ctx.fillRect(0, vp.canvasHeight - R, R, R);           // Bottom-left
  ctx.fillRect(vp.canvasWidth - R, vp.canvasHeight - R, R, R); // Bottom-right

  ctx.restore();
}
