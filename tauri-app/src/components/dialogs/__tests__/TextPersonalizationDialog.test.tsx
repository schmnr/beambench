import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { open } from '@tauri-apps/plugin-dialog';
import { TextPersonalizationDialog } from '../TextPersonalizationDialog';
import { variableTextService } from '../../../services/variableTextService';
import { useProjectStore } from '../../../stores/projectStore';
import { useAppStore } from '../../../stores/appStore';
import { beginNewDocument } from '../../../stores/documentGeneration';
import { usePreviewStore } from '../../../stores/previewStore';
import { useUndoStore } from '../../../stores/undoStore';
import { makeProject, makeProjectObject, makeTextObjectData } from '../../../test-utils/projectFixtures';
import { defaultVariableTextConfig } from '../../../utils/variableText';

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));
vi.mock('../../../services/variableTextService', () => ({ variableTextService: {
  generateBatchPreview: vi.fn(), loadCsvFile: vi.fn(), generateVariableTextBatch: vi.fn(),
} }));
const projectState = useProjectStore.getState();
const previewState = usePreviewStore.getState();
const undoState = useUndoStore.getState();
const object = makeProjectObject({ id: 'label', data: makeTextObjectData({ content: 'SN', variable_text: undefined }) });
const close = vi.fn();
beforeEach(() => {
  vi.resetAllMocks();
  useProjectStore.setState(projectState, true);
  useProjectStore.setState({ project: makeProject({ objects: [object] }), selectedObjectIds: [object.id] });
  useAppStore.setState({ settings: null });
  vi.mocked(variableTextService.generateBatchPreview).mockImplementation(async (_id, config) => Array.from({ length: config.source.totalCopies }, (_, index) => `Preview ${index + 1}`));
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  useProjectStore.setState(projectState, true);
  usePreviewStore.setState(previewState, true);
  useUndoStore.setState(undoState, true);
});

describe('desktop text personalization', () => {
  it('previews serial options and applies a live configuration to the selected text', async () => {
    const update = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({ updateObjectData: update });
    render(<TextPersonalizationDialog objectId="label" onClose={close} />);
    fireEvent.change(screen.getByLabelText('Start'), { target: { value: '100' } });
    fireEvent.change(screen.getByLabelText('Advance By'), { target: { value: '2' } });
    fireEvent.change(screen.getByLabelText('Serial Padding'), { target: { value: '4' } });
    await screen.findByText('Preview 1');
    fireEvent.click(screen.getByRole('button', { name: 'Apply' }));
    await waitFor(() => expect(close).toHaveBeenCalledOnce());
    expect(update).toHaveBeenCalledWith('label', expect.objectContaining({ variable_text: expect.objectContaining({
      template: 'SN-{Serial}', mode: 'serial_number', source: expect.objectContaining({ current: 100, advanceBy: 2, fieldDefaults: { _serial_padding: '4' } }),
    }) }));
  });

  it('maps legacy serial token options into the visible controls', async () => {
    useProjectStore.setState({ project: makeProject({ objects: [{ ...object, data: makeTextObjectData({ content: 'SN-{Serial:100,2,4}', variable_text: defaultVariableTextConfig('SN-{Serial:100,2,4}', { mode: 'serial_number' }) }) }] }) });
    render(<TextPersonalizationDialog objectId="label" onClose={close} />);
    expect(screen.getByLabelText('Start')).toHaveProperty('value', '100');
    expect(screen.getByLabelText('Advance By')).toHaveProperty('value', '2');
    expect(screen.getByLabelText('Serial Padding')).toHaveProperty('value', '4');
    expect(screen.getByLabelText('Template')).toHaveProperty('value', 'SN-{Serial}');
    await screen.findByText('Preview 1');
  });

  it('returns a personalized object to ordinary editable text with Normal and Apply', async () => {
    const update = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({ updateObjectData: update });
    render(<TextPersonalizationDialog objectId="label" onClose={close} />);
    await screen.findByText('Preview 1');
    fireEvent.change(screen.getByLabelText('Mode'), { target: { value: 'normal' } });
    await waitFor(() => expect(variableTextService.generateBatchPreview).toHaveBeenLastCalledWith('label', expect.objectContaining({ mode: 'normal', template: 'Preview 1' })));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Apply' })).toHaveProperty('disabled', false));
    fireEvent.click(screen.getByRole('button', { name: 'Apply' }));
    await waitFor(() => expect(update).toHaveBeenCalledWith('label', expect.objectContaining({ content: 'Preview 1', variable_text: undefined })));
  });

  it('previews a chosen date format', async () => {
    vi.mocked(variableTextService.generateBatchPreview).mockResolvedValue(['05/11/2026']);
    render(<TextPersonalizationDialog objectId="label" onClose={close} />);
    fireEvent.change(screen.getByLabelText('Mode'), { target: { value: 'date_time' } });
    fireEvent.change(screen.getByLabelText('Date format'), { target: { value: 'DD/MM/YYYY' } });
    await screen.findByText('05/11/2026');
    expect(variableTextService.generateBatchPreview).toHaveBeenLastCalledWith('label', expect.objectContaining({ template: '{Date:DD/MM/YYYY}', mode: 'date_time' }));
  });

  it('loads CSV, exposes column insertion, previews rows, and creates copies with independent spacing', async () => {
    const generate = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({ generatePersonalizedCopies: generate });
    vi.mocked(open).mockResolvedValue('/tmp/names.csv');
    vi.mocked(variableTextService.loadCsvFile).mockResolvedValue({ headers: ['Name', 'Code'], rows: [['Zoë', 'A1'], ['Jo', 'A2']] });
    render(<TextPersonalizationDialog objectId="label" onClose={close} />);
    fireEvent.change(screen.getByLabelText('Mode'), { target: { value: 'merge_csv' } });
    fireEvent.click(screen.getByRole('button', { name: 'Load CSV...' }));
    await screen.findByRole('button', { name: 'Name' });
    expect(screen.getByLabelText('Template')).toHaveProperty('value', '{CSV:Name}');
    fireEvent.click(screen.getByRole('button', { name: 'Code' }));
    fireEvent.change(screen.getByLabelText('Horizontal spacing (mm)'), { target: { value: '40' } });
    fireEvent.change(screen.getByLabelText('Vertical spacing (mm)'), { target: { value: '0' } });
    await screen.findByText('Preview 2');
    fireEvent.click(screen.getByRole('button', { name: 'Create copies' }));
    await waitFor(() => expect(close).toHaveBeenCalledOnce());
    expect(generate).toHaveBeenCalledWith('label', expect.objectContaining({
      template: '{CSV:Name}{CSV:Code}', source: expect.objectContaining({ totalCopies: 2, csvData: [['Name', 'Code'], ['Zoë', 'A1'], ['Jo', 'A2']] }),
    }), 40, 0);
  });

  it('keeps edits local when cancelled', async () => {
    const update = vi.fn();
    useProjectStore.setState({ updateObjectData: update });
    render(<TextPersonalizationDialog objectId="label" onClose={close} />);
    fireEvent.change(screen.getByLabelText('Template'), { target: { value: 'Part-{Serial}' } });
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(close).toHaveBeenCalledOnce();
    expect(update).not.toHaveBeenCalled();
    expect(useProjectStore.getState().project?.objects[0]).toEqual(object);
  });

  it('blocks invalid templates and clears old preview before applying changed input', async () => {
    render(<TextPersonalizationDialog objectId="label" onClose={close} />);
    await screen.findByText('Preview 1');
    fireEvent.change(screen.getByLabelText('Template'), { target: { value: '{CSV:Missing}' } });
    expect(screen.getByRole('alert').textContent).toContain('Check the template fields');
    expect(screen.getByRole('button', { name: 'Create copies' })).toHaveProperty('disabled', true);
    expect(screen.queryByText('Preview 1')).toBeNull();
  });

  it('shows CSV errors without discarding the source text', async () => {
    vi.mocked(open).mockResolvedValue('/tmp/bad.csv');
    vi.mocked(variableTextService.loadCsvFile).mockRejectedValue(new Error('Unclosed quoted CSV field'));
    render(<TextPersonalizationDialog objectId="label" onClose={close} />);
    fireEvent.change(screen.getByLabelText('Mode'), { target: { value: 'merge_csv' } });
    fireEvent.click(screen.getByRole('button', { name: 'Load CSV...' }));
    await waitFor(() => expect(variableTextService.loadCsvFile).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByRole('button', { name: 'Load CSV...' })).toHaveProperty('disabled', false));
    expect(screen.getByRole('button', { name: 'Create copies' })).toHaveProperty('disabled', true);
    expect(useProjectStore.getState().project?.objects[0]).toEqual(object);
  });

  it('closes when another project opens and ignores its late preview', async () => {
    let resolve!: (value: string[]) => void;
    vi.mocked(variableTextService.generateBatchPreview).mockReturnValue(new Promise((done) => { resolve = done; }));
    const view = render(<TextPersonalizationDialog objectId="label" onClose={close} />);
    await waitFor(() => expect(variableTextService.generateBatchPreview).toHaveBeenCalled());
    act(() => useProjectStore.setState({ project: makeProject({ metadata: { ...makeProject().metadata, project_id: 'other' }, objects: [] }) }));
    expect(close).toHaveBeenCalled();
    view.unmount();
    await act(async () => resolve(['Late preview']));
    expect(screen.queryByText('Late preview')).toBeNull();
  });
});

describe('generated copies project integration', () => {
  it('updates artwork and selection, then refreshes undo and preview once', async () => {
    const copy = makeProjectObject({ id: 'copy', data: makeTextObjectData({ content: 'SN-1' }) });
    const updated = { ...object, data: { ...object.data, variable_text: defaultVariableTextConfig('SN-{Serial}') } };
    vi.mocked(variableTextService.generateVariableTextBatch).mockResolvedValue({ copies: [copy], updatedOriginal: updated });
    const refresh = vi.fn().mockResolvedValue(undefined);
    const invalidate = vi.fn();
    useUndoStore.setState({ refresh }); usePreviewStore.setState({ invalidate });
    await expect(useProjectStore.getState().generatePersonalizedCopies('label', defaultVariableTextConfig('SN-{Serial}'), 40, 0)).resolves.toBe(true);
    expect(useProjectStore.getState().project?.objects).toEqual([updated, copy]);
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['copy']);
    expect(refresh).toHaveBeenCalledOnce(); expect(invalidate).toHaveBeenCalledOnce();
  });

  it('does not attach a late batch to a different document', async () => {
    let resolve!: (value: { copies: typeof object[]; updatedOriginal: typeof object }) => void;
    vi.mocked(variableTextService.generateVariableTextBatch).mockReturnValue(new Promise((done) => { resolve = done; }));
    const pending = useProjectStore.getState().generatePersonalizedCopies('label', defaultVariableTextConfig('SN-{Serial}'), 40, 0);
    beginNewDocument();
    const next = makeProject({ metadata: { ...makeProject().metadata, project_id: 'next' }, objects: [] });
    useProjectStore.setState({ project: next, selectedObjectIds: [] });
    resolve({ copies: [object], updatedOriginal: object });
    await expect(pending).resolves.toBe(false);
    expect(useProjectStore.getState().project).toEqual(next);
    expect(useProjectStore.getState().selectedObjectIds).toEqual([]);
  });
});
