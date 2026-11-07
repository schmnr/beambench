import '@testing-library/jest-dom/vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useProjectStore } from '../../../stores/projectStore';
import { usePdfPagePickerStore } from '../../../stores/pdfPagePickerStore';
import { makeProject, makeProjectObject } from '../../../test-utils/projectFixtures';
import { PdfPagePickerDialog } from '../PdfPagePickerDialog';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));

const initialProjectState = useProjectStore.getState();
const project = makeProject({ objects: [], dirty: false });
const pageRequest = { file_index: 0, pdf_page_count: 2, document_epoch: 7 };
const imported = makeProjectObject({ id: 'selected-page' });

beforeEach(() => {
  vi.resetAllMocks();
  useProjectStore.setState({ project, selectedLayerId: 'layer-1', selectedObjectIds: [] });
});

afterEach(() => {
  cleanup();
  usePdfPagePickerStore.getState().request?.finish(null);
  useProjectStore.setState(initialProjectState, true);
});

describe('desktop PDF page selection', () => {
  it('imports the chosen pages after each PDF in the batch gets a choice', async () => {
    const filePaths = ['C:\\artwork\\front.pdf', 'C:\\artwork\\back.pdf'];
    vi.mocked(open).mockResolvedValue(filePaths);
    vi.mocked(invoke)
      .mockResolvedValueOnce(pageRequest)
      .mockResolvedValueOnce({ ...pageRequest, file_index: 1, pdf_page_count: 4 })
      .mockResolvedValueOnce([imported])
      .mockResolvedValueOnce(makeProject({ objects: [imported] }))
      .mockResolvedValueOnce({ can_undo: true, can_redo: false });
    render(<PdfPagePickerDialog />);

    const pending = useProjectStore.getState().importFiles();
    await screen.findByRole('dialog', { name: 'Choose a PDF page' });
    expect(screen.getByText('front.pdf')).toBeVisible();
    const pageInput = screen.getByRole('spinbutton', { name: 'Page number' });
    const confirm = screen.getByRole('button', { name: 'Import Page' });
    expect(pageInput).toHaveFocus();
    expect(confirm).toBeDisabled();
    fireEvent.change(pageInput, { target: { value: '3' } });
    expect(confirm).toBeDisabled();
    fireEvent.change(pageInput, { target: { value: '2' } });
    fireEvent.click(confirm);

    expect(await screen.findByText('back.pdf')).toBeVisible();
    expect(screen.getByRole('spinbutton')).toHaveValue(null);
    fireEvent.change(screen.getByRole('spinbutton'), { target: { value: '4' } });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Import Page' }));
      await pending;
    });

    expect(invoke).toHaveBeenNthCalledWith(1, 'import_files', { filePaths, layerId: 'layer-1' });
    expect(invoke).toHaveBeenNthCalledWith(2, 'import_files', {
      filePaths, layerId: 'layer-1', pdfSelection: { pages: [2, null], documentEpoch: 7 },
    });
    expect(invoke).toHaveBeenNthCalledWith(3, 'import_files', {
      filePaths, layerId: 'layer-1', pdfSelection: { pages: [2, 4], documentEpoch: 7 },
    });
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['selected-page']);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('cancels a dropped PDF without retrying or changing the project', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(pageRequest);
    render(<PdfPagePickerDialog />);
    const files = [{ filename: 'dropped.pdf', dataBase64: 'cGRm' }];
    const pending = useProjectStore.getState().importFileData(files);
    const pageInput = await screen.findByRole('spinbutton');
    await act(async () => {
      fireEvent.keyDown(pageInput, { key: 'Escape' });
      await pending;
    });

    expect(invoke).toHaveBeenCalledExactlyOnceWith('import_file_data', { files, layerId: 'layer-1' });
    expect(useProjectStore.getState().project).toBe(project);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('imports a single-page PDF without a prompt or another import request', async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce([imported])
      .mockResolvedValueOnce(makeProject({ objects: [imported] }))
      .mockResolvedValueOnce({ can_undo: true, can_redo: false });
    render(<PdfPagePickerDialog />);

    await act(() => useProjectStore.getState().importFilePaths(['/tmp/single.pdf']));

    expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === 'import_files'))
      .toEqual([['import_files', { filePaths: ['/tmp/single.pdf'], layerId: 'layer-1' }]]);
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['selected-page']);
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});
