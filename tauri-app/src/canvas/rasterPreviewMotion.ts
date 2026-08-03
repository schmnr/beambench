import type { Point2D } from '../types/project';
import type { RasterRunExtent } from '../types/preview';
import type { RasterSegment } from './previewTimeline';
import { DEFAULT_RAPID_SPEED_MM_MIN } from './previewTimeline';
import { plannerStripPointToWorld } from './rasterTransform';

export interface RasterTravelTrace {
  from: Point2D;
  to: Point2D;
}

export interface RasterMotionProgress {
  visibleRuns: RasterRunExtent[];
  lastCompleteIndex: number;
  headPos: Point2D;
  visibleOverscanRuns: RasterRunExtent[];
  lastOverscanRowComplete: boolean;
  travelTraces: RasterTravelTrace[];
}

interface IndexedRun {
  run: RasterRunExtent;
  index: number;
}

interface MotionRow {
  envelope: RasterRunExtent;
  runs: IndexedRun[];
  direction: 1 | -1;
  feedStart: number;
  feedEnd: number;
}

interface MotionModel {
  rows: MotionRow[];
  phaseDurations: Array<{ feed: number; rapid: number }>;
  totalDuration: number;
}

const POSITION_EPSILON = 1e-7;
const motionModelCache = new WeakMap<RasterSegment, MotionModel>();

function worldPoint(seg: RasterSegment, x: number, y: number): Point2D {
  return plannerStripPointToWorld(y, x, {
    scanAxis: seg.scanAxis,
    scanAngleDeg: seg.scanAngleDeg,
    scanOrigin: seg.scanOrigin,
  });
}

function sameRow(a: number, b: number, lineIntervalMm: number): boolean {
  return Math.abs(a - b) <= Math.max(POSITION_EPSILON, lineIntervalMm * 0.1);
}

function buildMotionRows(seg: RasterSegment): MotionRow[] {
  const envelopes = seg.scanlineExtents ?? [];
  const runs = seg.runExtents ?? [];
  let runIndex = 0;
  return envelopes.map((envelope) => {
    const direction = envelope.direction === 'right_to_left' ? -1 : 1;
    const rowRuns: IndexedRun[] = [];
    while (
      runIndex < runs.length &&
      runs[runIndex].y_mm < envelope.y_mm &&
      !sameRow(runs[runIndex].y_mm, envelope.y_mm, seg.lineIntervalMm)
    ) {
      runIndex++;
    }
    while (
      runIndex < runs.length &&
      sameRow(runs[runIndex].y_mm, envelope.y_mm, seg.lineIntervalMm)
    ) {
      rowRuns.push({ run: runs[runIndex], index: runIndex });
      runIndex++;
    }
    const min = Math.min(envelope.start_x_mm, envelope.end_x_mm);
    const max = Math.max(envelope.start_x_mm, envelope.end_x_mm);
    return {
      envelope,
      runs: rowRuns,
      direction,
      feedStart: direction > 0 ? min - seg.overscanMm : max + seg.overscanMm,
      feedEnd: direction > 0 ? max + seg.overscanMm : min - seg.overscanMm,
    };
  });
}

function buildMotionModel(seg: RasterSegment): MotionModel {
  const cached = motionModelCache.get(seg);
  if (cached) return cached;

  const rows = buildMotionRows(seg);
  const phaseDurations: Array<{ feed: number; rapid: number }> = rows.map((row, index) => {
    const feedDistance = Math.abs(row.feedEnd - row.feedStart);
    const feed = seg.speedMmMin > 0 ? (feedDistance / seg.speedMmMin) * 60 : 0;
    const next = rows[index + 1];
    const rapid = next
      ? rapidDuration(
          worldPoint(seg, row.feedEnd, row.envelope.y_mm),
          worldPoint(seg, next.feedStart, next.envelope.y_mm),
        )
      : 0;
    return { feed, rapid };
  });
  const model = {
    rows,
    phaseDurations,
    totalDuration: phaseDurations.reduce((sum, phase) => sum + phase.feed + phase.rapid, 0),
  };
  motionModelCache.set(seg, model);
  return model;
}

function directedRunBounds(run: RasterRunExtent, direction: 1 | -1): [number, number] {
  const min = Math.min(run.start_x_mm, run.end_x_mm);
  const max = Math.max(run.start_x_mm, run.end_x_mm);
  return direction > 0 ? [min, max] : [max, min];
}

function appendTrace(
  traces: RasterTravelTrace[],
  seg: RasterSegment,
  y: number,
  fromX: number,
  toX: number,
): void {
  if (Math.abs(toX - fromX) <= POSITION_EPSILON) return;
  traces.push({
    from: worldPoint(seg, fromX, y),
    to: worldPoint(seg, toX, y),
  });
}

function appendCompletedRowTravel(
  traces: RasterTravelTrace[],
  seg: RasterSegment,
  row: MotionRow,
): void {
  let cursor = row.feedStart;
  for (const { run } of row.runs) {
    const [burnStart, burnEnd] = directedRunBounds(run, row.direction);
    appendTrace(traces, seg, row.envelope.y_mm, cursor, burnStart);
    cursor = burnEnd;
  }
  appendTrace(traces, seg, row.envelope.y_mm, cursor, row.feedEnd);
}

function appendPartialRowTravel(
  traces: RasterTravelTrace[],
  seg: RasterSegment,
  row: MotionRow,
  headX: number,
): void {
  let cursor = row.feedStart;
  for (const { run } of row.runs) {
    const [burnStart, burnEnd] = directedRunBounds(run, row.direction);
    const reachedBurnStart = row.direction > 0 ? headX > burnStart : headX < burnStart;
    const passedBurnEnd = row.direction > 0 ? headX >= burnEnd : headX <= burnEnd;
    if (!reachedBurnStart) {
      appendTrace(traces, seg, row.envelope.y_mm, cursor, headX);
      return;
    }
    appendTrace(traces, seg, row.envelope.y_mm, cursor, burnStart);
    if (!passedBurnEnd) return;
    cursor = burnEnd;
  }
  appendTrace(traces, seg, row.envelope.y_mm, cursor, headX);
}

function appendVisibleRuns(
  visibleRuns: RasterRunExtent[],
  row: MotionRow,
  headX: number | null,
): number {
  let lastComplete = -1;
  for (const { run, index } of row.runs) {
    const [burnStart, burnEnd] = directedRunBounds(run, row.direction);
    if (headX === null) {
      visibleRuns.push(run);
      lastComplete = Math.max(lastComplete, index);
      continue;
    }
    const passedBurnEnd = row.direction > 0 ? headX >= burnEnd : headX <= burnEnd;
    const reachedBurnStart = row.direction > 0 ? headX > burnStart : headX < burnStart;
    if (passedBurnEnd) {
      visibleRuns.push(run);
      lastComplete = Math.max(lastComplete, index);
      continue;
    }
    if (reachedBurnStart) {
      visibleRuns.push({
        ...run,
        start_x_mm: Math.min(burnStart, headX),
        end_x_mm: Math.max(burnStart, headX),
      });
    }
    break;
  }
  return lastComplete;
}

function rapidDuration(from: Point2D, to: Point2D): number {
  const distance = Math.hypot(to.x - from.x, to.y - from.y);
  return DEFAULT_RAPID_SPEED_MM_MIN > 0 ? (distance / DEFAULT_RAPID_SPEED_MM_MIN) * 60 : 0;
}

/**
 * Resolve raster playback against the planner's real motion envelope:
 * continuous feed sweeps through burn islands and gaps, overscan at both row
 * ends, and rapid positioning between emitted rows.
 */
export function computeRasterMotionProgress(
  seg: RasterSegment,
  progress: number,
  includeTravelTraces = true,
): RasterMotionProgress | null {
  const { rows, phaseDurations, totalDuration } = buildMotionModel(seg);
  if (rows.length === 0) return null;
  let remaining = Math.max(0, Math.min(1, progress)) * totalDuration;
  const visibleRuns: RasterRunExtent[] = [];
  const visibleOverscanRuns: RasterRunExtent[] = [];
  const travelTraces: RasterTravelTrace[] = [];
  let lastCompleteIndex = -1;
  let lastOverscanRowComplete = false;
  let headPos = worldPoint(seg, rows[0].feedStart, rows[0].envelope.y_mm);

  if (totalDuration <= 0) {
    for (const row of rows) {
      lastCompleteIndex = Math.max(lastCompleteIndex, appendVisibleRuns(visibleRuns, row, null));
      visibleOverscanRuns.push(row.envelope);
      if (includeTravelTraces) appendCompletedRowTravel(travelTraces, seg, row);
      headPos = worldPoint(seg, row.feedEnd, row.envelope.y_mm);
    }
    return {
      visibleRuns,
      lastCompleteIndex,
      headPos,
      visibleOverscanRuns,
      lastOverscanRowComplete: true,
      travelTraces,
    };
  }

  for (let index = 0; index < rows.length; index++) {
    const row = rows[index];
    const phase = phaseDurations[index];

    if (remaining <= 0) break;
    visibleOverscanRuns.push(row.envelope);
    lastOverscanRowComplete = false;

    if (phase.feed <= 0 || remaining >= phase.feed) {
      lastCompleteIndex = Math.max(lastCompleteIndex, appendVisibleRuns(visibleRuns, row, null));
      if (includeTravelTraces) appendCompletedRowTravel(travelTraces, seg, row);
      headPos = worldPoint(seg, row.feedEnd, row.envelope.y_mm);
      lastOverscanRowComplete = true;
      remaining -= phase.feed;
    } else {
      const fraction = remaining / phase.feed;
      const headX = row.feedStart + (row.feedEnd - row.feedStart) * fraction;
      lastCompleteIndex = Math.max(lastCompleteIndex, appendVisibleRuns(visibleRuns, row, headX));
      if (includeTravelTraces) appendPartialRowTravel(travelTraces, seg, row, headX);
      headPos = worldPoint(seg, headX, row.envelope.y_mm);
      return {
        visibleRuns,
        lastCompleteIndex,
        headPos,
        visibleOverscanRuns,
        lastOverscanRowComplete,
        travelTraces,
      };
    }

    const next = rows[index + 1];
    if (!next) continue;
    const rapidFrom = worldPoint(seg, row.feedEnd, row.envelope.y_mm);
    const rapidTo = worldPoint(seg, next.feedStart, next.envelope.y_mm);
    if (phase.rapid <= 0 || remaining >= phase.rapid) {
      if (
        includeTravelTraces &&
        Math.hypot(rapidTo.x - rapidFrom.x, rapidTo.y - rapidFrom.y) > POSITION_EPSILON
      ) {
        travelTraces.push({ from: rapidFrom, to: rapidTo });
      }
      headPos = rapidTo;
      remaining -= phase.rapid;
    } else {
      const fraction = remaining / phase.rapid;
      const partial = {
        x: rapidFrom.x + (rapidTo.x - rapidFrom.x) * fraction,
        y: rapidFrom.y + (rapidTo.y - rapidFrom.y) * fraction,
      };
      if (includeTravelTraces) travelTraces.push({ from: rapidFrom, to: partial });
      headPos = partial;
      return {
        visibleRuns,
        lastCompleteIndex,
        headPos,
        visibleOverscanRuns,
        lastOverscanRowComplete,
        travelTraces,
      };
    }
  }

  return {
    visibleRuns,
    lastCompleteIndex,
    headPos,
    visibleOverscanRuns,
    lastOverscanRowComplete,
    travelTraces,
  };
}
