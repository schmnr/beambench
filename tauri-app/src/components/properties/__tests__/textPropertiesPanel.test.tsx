import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent } from '@testing-library/react';
import { TextPropertiesPanel } from '../TextPropertiesPanel';
import { TextDefaultsSection } from '../TextDefaultsSection';
import { useProjectStore } from '../../../stores/projectStore';
import { useUiStore } from '../../../stores/uiStore';
import type { ObjectData } from '../../../types/project';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string) => {
    if (cmd === 'get_system_fonts') return Promise.resolve(['Arial', 'Helvetica']);
    return Promise.resolve(null);
  }),
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const makeTextData = (over: Partial<Extract<ObjectData, { type: 'text' }>> = {}): Extract<ObjectData, { type: 'text' }> => ({
  type: 'text',
  content: 'Hello',
  font_family: 'Arial',
  font_size_mm: 10,
  alignment: 'left',
  alignment_v: 'top',
  bold: false,
  italic: false,
  upper_case: false,
  welded: false,
  h_spacing: 0,
  v_spacing: 0,
  layout_mode: 'straight',
  on_path: false,
  path_offset: 0,
  distort: false,
  rtl: false,
  bend_radius: 0,
  transform_style: 'none',
  transform_curve: 0,
  circle_placement: 'top_outside',
  max_width: null,
  squeeze: false,
  ignore_empty_vars: false,
  missing_font: false,
  guide_path_id: null,
  missing_glyphs: [],
  ...over,
});

const initialProjectState = useProjectStore.getState();
const initialUiState = useUiStore.getState();

afterEach(() => {
  cleanup();
  useProjectStore.setState(initialProjectState, true);
  useUiStore.setState(initialUiState, true);
});

describe('TextPropertiesPanel', () => {
  it('bold toggle commits updated text data', () => {
    const updateObjectData = vi.fn();
    useProjectStore.setState({ updateObjectData });
    render(<TextPropertiesPanel objectId="t1" data={makeTextData()} />);
    fireEvent.click(screen.getByLabelText('Bold'));
    expect(updateObjectData).toHaveBeenCalledWith('t1', expect.objectContaining({ bold: true }));
  });

  it('uppercase toggle commits updated text data', () => {
    const updateObjectData = vi.fn();
    useProjectStore.setState({ updateObjectData });
    render(<TextPropertiesPanel objectId="t1" data={makeTextData()} />);
    fireEvent.click(screen.getByLabelText('Uppercase'));
    expect(updateObjectData).toHaveBeenCalledWith('t1', expect.objectContaining({ upper_case: true }));
  });

  it('path mode without a guide path offers Select Path', () => {
    render(
      <TextPropertiesPanel
        objectId="t1"
        data={makeTextData({ layout_mode: 'path', guide_path_id: null })}
      />,
    );
    expect(screen.getByText('Select Path')).toBeDefined();
  });

  it('path mode with a linked guide path offers Pick and Clear', () => {
    render(
      <TextPropertiesPanel
        objectId="t1"
        data={makeTextData({ layout_mode: 'path', guide_path_id: 'g1' })}
      />,
    );
    expect(screen.getByText('Pick')).toBeDefined();
    expect(screen.getByText('Clear')).toBeDefined();
  });
});

describe('TextDefaultsSection', () => {
  it('edits the text defaults used for the next text object', () => {
    render(<TextDefaultsSection />);
    fireEvent.click(screen.getByLabelText('Bold'));
    expect(useUiStore.getState().textDefaults.bold).toBe(true);
  });

  it('shows font and size controls', () => {
    render(<TextDefaultsSection />);
    expect(screen.getByText('Font')).toBeDefined();
    expect(screen.getAllByRole('spinbutton').length).toBeGreaterThanOrEqual(3);
  });
});
