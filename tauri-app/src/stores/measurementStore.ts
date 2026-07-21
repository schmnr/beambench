import { create } from 'zustand';
import type {
  MeasurementDragMetrics,
  MeasurementHoverResult,
} from '../canvas/measurement';

export type MeasurementState =
  | { type: 'idle' }
  | ({ type: 'hover' } & MeasurementHoverResult)
  | ({ type: 'drag' } & MeasurementDragMetrics);

interface MeasurementStoreState {
  state: MeasurementState;
  setHover: (result: MeasurementHoverResult) => void;
  setDrag: (result: MeasurementDragMetrics) => void;
  clear: () => void;
}

export const useMeasurementStore = create<MeasurementStoreState>((set) => ({
  state: { type: 'idle' },
  setHover: (result) => set({ state: { type: 'hover', ...result } }),
  setDrag: (result) => set({ state: { type: 'drag', ...result } }),
  clear: () => set({ state: { type: 'idle' } }),
}));
