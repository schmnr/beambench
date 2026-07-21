import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { MeasurementPanel } from '../MeasurementPanel';
import { useAppStore } from '../../../stores/appStore';
import { useMeasurementStore } from '../../../stores/measurementStore';
import { makeAppSettings } from '../../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));

const initialAppState = useAppStore.getState();
const initialMeasurementState = useMeasurementStore.getState();

afterEach(() => {
  cleanup();
  useAppStore.setState(initialAppState, true);
  useMeasurementStore.setState(initialMeasurementState, true);
});

describe('MeasurementPanel', () => {
  it('separates object metrics from hovered segment metrics', () => {
    useAppStore.setState({ settings: makeAppSettings({ display_unit: 'mm' }) });
    useMeasurementStore.getState().setHover({
      objectId: 'rect-1',
      objectMetrics: {
        objectId: 'rect-1',
        objectName: 'Measured Rect',
        nodes: 4,
        lines: 4,
        curves: 0,
        perimeterMm: 60,
        widthMm: 20,
        heightMm: 10,
        center: { x: 10, y: 5 },
        closed: true,
        areaMm2: 200,
      },
      segment: {
        start: { x: 0, y: 0 },
        end: { x: 20, y: 0 },
        dxMm: 20,
        dyMm: 0,
        lengthMm: 20,
        angleDeg: 0,
        segmentIndex: 0,
        t: 0.5,
      },
    });

    render(<MeasurementPanel />);

    expect(screen.getByText('Measured Rect')).toBeDefined();
    expect(screen.getByText('Object')).toBeDefined();
    expect(screen.getByText('Segment')).toBeDefined();
    expect(screen.getByText('200.00 mm^2')).toBeDefined();
    expect(screen.getByText('10.00, 5.00 mm')).toBeDefined();
    expect(screen.getByText('10.00, 0.00 mm')).toBeDefined();
    expect(screen.queryByText('Arc Radius')).toBeNull();
  });

  it('marks area as not applicable for open geometry', () => {
    useAppStore.setState({ settings: makeAppSettings({ display_unit: 'mm' }) });
    useMeasurementStore.getState().setHover({
      objectId: 'line-1',
      objectMetrics: {
        objectId: 'line-1',
        objectName: 'Open Line',
        nodes: 2,
        lines: 1,
        curves: 0,
        perimeterMm: 10,
        widthMm: 10,
        heightMm: 0,
        center: { x: 5, y: 0 },
        closed: false,
        areaMm2: null,
      },
      segment: {
        start: { x: 0, y: 0 },
        end: { x: 10, y: 0 },
        dxMm: 10,
        dyMm: 0,
        lengthMm: 10,
        angleDeg: 0,
        segmentIndex: 0,
        t: 0.5,
      },
    });

    render(<MeasurementPanel />);

    expect(screen.getByText('Open Line')).toBeDefined();
    expect(screen.getByText('Area').parentElement?.textContent).toContain('N/A');
  });

  it('unit toggle updates the global display unit', () => {
    const updateSettings = vi.fn().mockResolvedValue(undefined);
    useAppStore.setState({
      settings: makeAppSettings({ display_unit: 'mm' }),
      updateSettings,
    });

    render(<MeasurementPanel />);
    fireEvent.click(screen.getByText('in'));

    expect(updateSettings).toHaveBeenCalledWith({ display_unit: 'inches' });
  });
});
