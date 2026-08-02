import { create } from 'zustand';
import type {
  MeasurementDragMetrics,
  MeasurementHoverResult,
  MeasurementMode,
  MeasurementPending,
  MeasurementResult,
} from '../canvas/measurement';

interface MeasurementStoreState {
  mode: MeasurementMode;
  hover: MeasurementHoverResult | null;
  draft: MeasurementDragMetrics | null;
  pending: MeasurementPending | null;
  result: MeasurementResult | null;
  setMode: (mode: MeasurementMode) => void;
  setHover: (result: MeasurementHoverResult | null) => void;
  setDraft: (result: MeasurementDragMetrics | null) => void;
  setPending: (pending: MeasurementPending | null) => void;
  setResult: (result: MeasurementResult | null) => void;
  clearMeasurement: () => void;
  clear: () => void;
}

export const useMeasurementStore = create<MeasurementStoreState>((set) => ({
  mode: 'linear',
  hover: null,
  draft: null,
  pending: null,
  result: null,
  setMode: (mode) => set({ mode, draft: null, pending: null, result: null }),
  setHover: (hover) => set({ hover }),
  setDraft: (draft) => set({ draft }),
  setPending: (pending) => set({ pending, draft: null, result: null }),
  setResult: (result) => set({ result, draft: null, pending: null }),
  clearMeasurement: () => set({ draft: null, pending: null, result: null }),
  clear: () => set({ hover: null, draft: null, pending: null, result: null }),
}));
