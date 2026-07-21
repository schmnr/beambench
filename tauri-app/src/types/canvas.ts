import type { Point2D } from './project';

export interface Viewport {
  offset: Point2D;
  zoom: number;
}

export type HandleId =
  | 'nw' | 'n' | 'ne'
  | 'w'  | 'center' | 'e'
  | 'sw' | 's' | 'se'
  | 'rotate_nw' | 'rotate_ne' | 'rotate_sw' | 'rotate_se'
  | 'shear_n' | 'shear_e';

export interface DragState {
  type: 'move' | 'resize' | 'rotate' | 'shear' | 'rubber-band' | 'create-shape';
  startWorld: Point2D;
  startScreen: Point2D;
  currentWorld: Point2D;
  currentScreen: Point2D;
  handleId?: HandleId;
}

export interface SnapResult {
  snapped: Point2D;
  didSnap: boolean;
}
