import type {
  TextCirclePlacement,
  TextLayoutMode,
  TextTransformStyle,
} from '../../types/project';

export type TextShapeMode = TextLayoutMode | Exclude<TextTransformStyle, 'none'>;

export interface TextShapeState {
  layout_mode: TextLayoutMode;
  on_path: boolean;
  bend_radius: number;
  transform_style: TextTransformStyle;
  transform_curve: number;
  circle_placement: TextCirclePlacement;
}

export const DEFAULT_TEXT_SHAPE_STRENGTH = 50;
export const DEFAULT_BEND_RADIUS_MM = 50;

export function activeTextShape(value: Pick<TextShapeState, 'layout_mode' | 'on_path' | 'transform_style'>): TextShapeMode {
  if (value.transform_style !== 'none') return value.transform_style;
  if (value.on_path && value.layout_mode === 'straight') return 'path';
  return value.layout_mode;
}

export function patchForTextShape(value: TextShapeState, shape: TextShapeMode): Partial<TextShapeState> {
  if (shape === 'straight') {
    return {
      layout_mode: 'straight',
      on_path: false,
      transform_style: 'none',
    };
  }

  if (shape === 'bend') {
    return {
      layout_mode: 'bend',
      on_path: false,
      transform_style: 'none',
      bend_radius: Math.abs(value.bend_radius) > 1e-9
        ? value.bend_radius
        : DEFAULT_BEND_RADIUS_MM,
    };
  }

  if (shape === 'path') {
    return {
      layout_mode: 'path',
      on_path: true,
      transform_style: 'none',
    };
  }

  return {
    layout_mode: 'straight',
    on_path: false,
    transform_style: shape,
    transform_curve: shape === 'circle'
      ? (value.transform_style === 'circle' ? value.transform_curve : 0)
      : (Math.abs(value.transform_curve) > 1e-9
          ? value.transform_curve
          : DEFAULT_TEXT_SHAPE_STRENGTH),
  };
}

export function signedTextShapeValue(magnitude: number, reverse: boolean, fallback: number): number {
  const safeMagnitude = Math.abs(magnitude) > 1e-9 ? Math.abs(magnitude) : fallback;
  return reverse ? -safeMagnitude : safeMagnitude;
}

export function circlePlacementParts(placement: TextCirclePlacement): {
  vertical: 'top' | 'bottom';
  side: 'outside' | 'inside';
} {
  const [vertical, side] = placement.split('_') as ['top' | 'bottom', 'outside' | 'inside'];
  return { vertical, side };
}

export function circlePlacementFromParts(
  vertical: 'top' | 'bottom',
  side: 'outside' | 'inside',
): TextCirclePlacement {
  return `${vertical}_${side}` as TextCirclePlacement;
}

export function circleCurveToDegrees(curve: number): number {
  const safeCurve = Number.isFinite(curve) ? Math.max(-100, Math.min(100, curve)) : 0;
  return Math.round(180 * (1 + 0.75 * safeCurve / 100));
}

export function circleDegreesToCurve(degrees: number): number {
  const safeDegrees = Number.isFinite(degrees) ? Math.max(45, Math.min(315, degrees)) : 180;
  return ((safeDegrees / 180) - 1) * (100 / 0.75);
}

export function textShapeSupportsDistort(shape: TextShapeMode): boolean {
  return shape === 'bend' || shape === 'path' || shape === 'arch' || shape === 'circle';
}

export function textShapeUsesEnvelopeStrength(shape: TextShapeMode): boolean {
  return shape === 'arch' || shape === 'rise' || shape === 'wave' || shape === 'flag' || shape === 'angle';
}
