import type { ProjectObject } from '../types/project';
import type { GridArraySizingMode, GridSpacingMode } from '../types/vector';

export interface GridArraySizingParams {
  rows: number;
  cols: number;
  sizingModeX: GridArraySizingMode;
  sizingModeY: GridArraySizingMode;
  totalWidthMm?: number;
  totalHeightMm?: number;
  hSpacingMm: number;
  vSpacingMm: number;
  spacingMode: GridSpacingMode;
  xColShiftMm?: number;
  yRowShiftMm?: number;
  halfShift?: boolean;
  reverseH?: boolean;
  reverseV?: boolean;
}

interface FootprintBounds {
  width: number;
  height: number;
}

function effectiveSpacing(
  objects: ProjectObject[],
  spacingMode: GridSpacingMode,
  hSpacingMm: number,
  vSpacingMm: number,
): { effH: number; effV: number } {
  const minX = Math.min(...objects.map((object) => object.bounds.min.x));
  const minY = Math.min(...objects.map((object) => object.bounds.min.y));
  const maxX = Math.max(...objects.map((object) => object.bounds.max.x));
  const maxY = Math.max(...objects.map((object) => object.bounds.max.y));
  const width = maxX - minX;
  const height = maxY - minY;
  return {
    effH: spacingMode === 'edgeToEdge' ? hSpacingMm + width : hSpacingMm,
    effV: spacingMode === 'edgeToEdge' ? vSpacingMm + height : vSpacingMm,
  };
}

function gridCellTranslation(
  params: GridArraySizingParams,
  row: number,
  col: number,
  effH: number,
  effV: number,
): { tx: number; ty: number } {
  const effectiveCol = params.reverseH ? -col : col;
  const effectiveRow = params.reverseV ? -row : row;
  let tx = effectiveCol * effH + (params.xColShiftMm ?? 0) * effectiveRow;
  const ty = effectiveRow * effV + (params.yRowShiftMm ?? 0) * effectiveCol;
  if (params.halfShift && row % 2 === 1) {
    tx += effH / 2;
  }
  return { tx, ty };
}

export function computeGridArrayFootprint(
  objects: ProjectObject[],
  params: GridArraySizingParams,
): FootprintBounds {
  if (objects.length === 0) {
    return { width: 0, height: 0 };
  }
  const { effH, effV } = effectiveSpacing(objects, params.spacingMode, params.hSpacingMm, params.vSpacingMm);
  let minX = Number.POSITIVE_INFINITY;
  let minY = Number.POSITIVE_INFINITY;
  let maxX = Number.NEGATIVE_INFINITY;
  let maxY = Number.NEGATIVE_INFINITY;

  for (let row = 0; row < Math.max(1, params.rows); row += 1) {
    for (let col = 0; col < Math.max(1, params.cols); col += 1) {
      const { tx, ty } = gridCellTranslation(params, row, col, effH, effV);
      for (const object of objects) {
        minX = Math.min(minX, object.bounds.min.x + tx);
        minY = Math.min(minY, object.bounds.min.y + ty);
        maxX = Math.max(maxX, object.bounds.max.x + tx);
        maxY = Math.max(maxY, object.bounds.max.y + ty);
      }
    }
  }

  return { width: maxX - minX, height: maxY - minY };
}

function fitsTotals(footprint: FootprintBounds, params: GridArraySizingParams): boolean {
  const epsilon = 1e-9;
  if (params.sizingModeX === 'total' && footprint.width - (params.totalWidthMm ?? 0) > epsilon) {
    return false;
  }
  if (params.sizingModeY === 'total' && footprint.height - (params.totalHeightMm ?? 0) > epsilon) {
    return false;
  }
  return true;
}

function leftoverForCandidate(footprint: FootprintBounds, params: GridArraySizingParams): number {
  let leftover = 0;
  if (params.sizingModeX === 'total') {
    leftover += Math.max(0, (params.totalWidthMm ?? footprint.width) - footprint.width);
  }
  if (params.sizingModeY === 'total') {
    leftover += Math.max(0, (params.totalHeightMm ?? footprint.height) - footprint.height);
  }
  return leftover;
}

export function fitGridArrayCounts(
  objects: ProjectObject[],
  params: GridArraySizingParams,
  maxAxisCount = 100,
): { rows: number; cols: number } {
  if (objects.length === 0 || (params.sizingModeX === 'count' && params.sizingModeY === 'count')) {
    return { rows: Math.max(1, params.rows), cols: Math.max(1, params.cols) };
  }

  let best: { rows: number; cols: number; leftover: number } | null = null;
  const rowValues = params.sizingModeY === 'total'
    ? Array.from({ length: maxAxisCount }, (_, index) => index + 1)
    : [Math.max(1, params.rows)];
  const colValues = params.sizingModeX === 'total'
    ? Array.from({ length: maxAxisCount }, (_, index) => index + 1)
    : [Math.max(1, params.cols)];

  for (const rows of rowValues) {
    for (const cols of colValues) {
      const candidate = { ...params, rows, cols };
      const footprint = computeGridArrayFootprint(objects, candidate);
      if (!fitsTotals(footprint, candidate)) {
        continue;
      }
      const leftover = leftoverForCandidate(footprint, candidate);
      if (!best) {
        best = { rows, cols, leftover };
        continue;
      }
      const candidateCells = rows * cols;
      const bestCells = best.rows * best.cols;
      if (
        candidateCells > bestCells
        || (candidateCells === bestCells && cols > best.cols)
        || (candidateCells === bestCells && cols === best.cols && rows > best.rows)
        || (candidateCells === bestCells
          && cols === best.cols
          && rows === best.rows
          && leftover < best.leftover)
      ) {
        best = { rows, cols, leftover };
      }
    }
  }

  if (best) {
    return { rows: best.rows, cols: best.cols };
  }
  return {
    rows: params.sizingModeY === 'total' ? 1 : Math.max(1, params.rows),
    cols: params.sizingModeX === 'total' ? 1 : Math.max(1, params.cols),
  };
}
