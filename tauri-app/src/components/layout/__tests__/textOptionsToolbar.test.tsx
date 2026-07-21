import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent, waitFor } from '@testing-library/react';
import { PropertiesToolbar } from '../PropertiesToolbar';
import { useProjectStore } from '../../../stores/projectStore';
import { useAppStore } from '../../../stores/appStore';
import { makeLayer, makeProject as makeProjectFixture, makeProjectObject, makeTextObjectData } from '../../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string) => {
    if (cmd === 'get_system_fonts') return Promise.resolve(['Arial', 'Helvetica', 'Times New Roman']);
    return Promise.resolve(null);
  }),
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const makeTextProject = () => ({
  ...makeProjectFixture({
    metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
    layers: [makeLayer({ id: 'l1', name: 'L1', operation: 'line', color_tag: '#ff0000' })],
    assets: [],
  }),
  objects: [makeProjectObject({
    id: 'txt1', name: 'Text1',
    bounds: { min: { x: 0, y: 0 }, max: { x: 50, y: 10 } },
    layer_id: 'l1',
    data: makeTextObjectData({ content: 'Hello', font_family: 'sans-serif', font_size_mm: 10, bold: true }),
  })],
});

const makeShapeProject = () => ({
  ...makeProjectFixture({
    metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
    layers: [makeLayer({ id: 'l1', name: 'L1', operation: 'line', color_tag: '#ff0000' })],
    assets: [],
  }),
  objects: [makeProjectObject({
    id: 'shp1', name: 'Rect1',
    bounds: { min: { x: 0, y: 0 }, max: { x: 50, y: 50 } },
    layer_id: 'l1',
    data: { type: 'shape' as const, kind: 'rectangle' as const, width: 50, height: 50, corner_radius: 0 },
  })],
});

const initialState = useProjectStore.getState();
const initialAppState = useAppStore.getState();

afterEach(() => {
  cleanup();
  useProjectStore.setState(initialState, true);
  useAppStore.setState(initialAppState, true);
});

describe('PropertiesToolbar — text options', () => {
  it('shows text controls always but dimmed when non-text object selected', () => {
    useProjectStore.setState({ project: makeShapeProject(), selectedObjectIds: ['shp1'] });
    render(<PropertiesToolbar />);
    // Position fields should be visible
    expect(screen.getByText('X Pos')).toBeDefined();
    // Text controls should be visible but dimmed
    expect(screen.getByText('Font')).toBeDefined();
    // "Height" appears in both position and text sections
    expect(screen.getAllByText('Height').length).toBeGreaterThanOrEqual(1);
  });

  it('shows fonts from getSystemFonts', async () => {
    useProjectStore.setState({ project: makeTextProject(), selectedObjectIds: ['txt1'] });
    render(<PropertiesToolbar />);
    await waitFor(() => {
      expect(screen.getByText('Arial')).toBeDefined();
    });
    expect(screen.getByText('Helvetica')).toBeDefined();
  });

  it('field change commits updateObjectData on blur', () => {
    const updateObjectData = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeTextProject(), selectedObjectIds: ['txt1'], updateObjectData });
    render(<PropertiesToolbar />);
    // Find the Height input — it's a spinbutton after the position ones
    const inputs = screen.getAllByRole('spinbutton');
    // Height input is after X, W, Y, H, Rot, SX, SY = 7 position inputs
    const htInput = inputs[7];
    fireEvent.change(htInput, { target: { value: '12' } });
    fireEvent.blur(htInput);
    expect(updateObjectData).toHaveBeenCalled();
  });

  it('text field typing buffers keystrokes and commits once on blur', () => {
    const updateObjectData = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeTextProject(), selectedObjectIds: ['txt1'], updateObjectData });
    render(<PropertiesToolbar />);
    const inputs = screen.getAllByRole('spinbutton');
    const htInput = inputs[7];
    fireEvent.change(htInput, { target: { value: '1' } });
    fireEvent.change(htInput, { target: { value: '12' } });
    expect(updateObjectData).not.toHaveBeenCalled();
    fireEvent.blur(htInput);
    expect(updateObjectData).toHaveBeenCalledTimes(1);
    expect(updateObjectData.mock.calls[0][1].font_size_mm).toBe(12);
  });

  it('Bold/Italic toggle buttons reflect current state', () => {
    useProjectStore.setState({ project: makeTextProject(), selectedObjectIds: ['txt1'] });
    render(<PropertiesToolbar />);
    // Bold: active (data.bold = true) — the radio indicator (parent button's first child span) should have bg-bb-accent
    const boldLabel = screen.getByText('Bold');
    const boldBtn = boldLabel.closest('button')!;
    const boldIndicator = boldBtn.querySelector('span')!;
    expect(boldIndicator.className).toContain('bg-bb-accent');
    // Italic: inactive (data.italic = false) — indicator should NOT have bg-bb-accent
    const italicLabel = screen.getByText('Italic');
    const italicBtn = italicLabel.closest('button')!;
    const italicIndicator = italicBtn.querySelector('span')!;
    expect(italicIndicator.className).not.toContain('bg-bb-accent');
  });

  it('shows Welded and Distort controls', () => {
    useProjectStore.setState({ project: makeTextProject(), selectedObjectIds: ['txt1'] });
    render(<PropertiesToolbar />);
    expect(screen.getByText('Welded')).toBeDefined();
    expect(screen.getByText('Distort')).toBeDefined();
  });

  it('shows Offset control for path offset', () => {
    useProjectStore.setState({ project: makeTextProject(), selectedObjectIds: ['txt1'] });
    render(<PropertiesToolbar />);
    expect(screen.getByText('Offset')).toBeDefined();
  });

  it('shows Align X and Align Y dropdowns', () => {
    useProjectStore.setState({ project: makeTextProject(), selectedObjectIds: ['txt1'] });
    render(<PropertiesToolbar />);
    expect(screen.getByText('Align X')).toBeDefined();
    expect(screen.getByText('Align Y')).toBeDefined();
  });

  it('upper-case toggle updates boolean text data', () => {
    const updateObjectData = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeTextProject(), selectedObjectIds: ['txt1'], updateObjectData });
    render(<PropertiesToolbar />);
    fireEvent.click(screen.getByText('Upper Case'));
    expect(updateObjectData).toHaveBeenCalled();
    const lastCall = updateObjectData.mock.calls[updateObjectData.mock.calls.length - 1];
    expect(lastCall?.[1].upper_case).toBe(true);
  });

  it('welded toggle updates text data', () => {
    const updateObjectData = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ project: makeTextProject(), selectedObjectIds: ['txt1'], updateObjectData });
    render(<PropertiesToolbar />);
    fireEvent.click(screen.getByText('Welded'));
    expect(updateObjectData).toHaveBeenCalled();
    const lastCall = updateObjectData.mock.calls[updateObjectData.mock.calls.length - 1];
    expect(lastCall?.[1].welded).toBe(true);
  });
});
