import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { MeasurementPropertiesSection } from '../MeasurementPropertiesSection';
import { useProjectStore } from '../../../stores/projectStore';
import { useAppStore } from '../../../stores/appStore';
import { useMeasurementStore } from '../../../stores/measurementStore';
import { buildLinearMeasurement } from '../../../canvas/measurement';
import { makeAppSettings, makeProject, makeWorkspace } from '../../../test-utils/projectFixtures';

afterEach(() => {
  cleanup();
  useMeasurementStore.getState().clear();
  vi.restoreAllMocks();
});

describe('measurement workspace coordinates', () => {
  it('shows and copies the same bottom-left coordinates as Properties without changing distances', async () => {
    useProjectStore.setState({ project: makeProject({ workspace: makeWorkspace({ origin: 'bottom_left' }) }) });
    useAppStore.setState({ settings: makeAppSettings({ display_unit: 'mm' }) });
    useMeasurementStore.getState().setResult(buildLinearMeasurement({ x: 100, y: 140 }, { x: 300, y: 140 }));
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
    render(<MeasurementPropertiesSection />);

    expect(screen.getByRole('button', { name: '100.00, 260.00 mm' })).toBeTruthy();
    expect(screen.getByRole('button', { name: '300.00, 260.00 mm' })).toBeTruthy();
    expect(screen.getByRole('button', { name: '200.00, 260.00 mm' })).toBeTruthy();
    expect(screen.getAllByRole('button', { name: '200.00 mm' })).toHaveLength(2);
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '100.00, 260.00 mm' }));
    });
    expect(writeText).toHaveBeenCalledWith('100.00, 260.00 mm');

    act(() => useProjectStore.setState({ project: makeProject({ workspace: makeWorkspace({ origin: 'top_left' }) }) }));
    expect(screen.getByRole('button', { name: '300.00, 140.00 mm' })).toBeTruthy();
  });

  it.each([true, false])('converts a radius center before formatting inches, circular=%s', (circular) => {
    useProjectStore.setState({ project: makeProject({ workspace: makeWorkspace({ origin: 'bottom_left', bed_height_mm: 254 }) }) });
    useAppStore.setState({ settings: makeAppSettings({ display_unit: 'inches' }) });
    useMeasurementStore.getState().setResult({
      kind: 'radius', objectId: 'circle', objectName: 'Circle', center: { x: 25.4, y: 50.8 },
      edgePoint: { x: 50.8, y: 50.8 }, radiusXmm: 25.4, radiusYmm: 25.4,
      diameterXmm: 50.8, diameterYmm: 50.8, circular,
    });
    render(<MeasurementPropertiesSection />);
    expect(screen.getByRole('button', { name: '1.000, 8.000 in' })).toBeTruthy();
  });
});
