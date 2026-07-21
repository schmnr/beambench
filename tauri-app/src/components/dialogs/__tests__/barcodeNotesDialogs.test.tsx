import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent, waitFor } from '@testing-library/react';
import { BarcodeDialog } from '../BarcodeDialog';
import { NotesDialog } from '../NotesDialog';
import { useProjectStore } from '../../../stores/projectStore';
import { makeLayer, makeProject, makeProjectObject } from '../../../test-utils/projectFixtures';

const mockInvoke = vi.fn().mockResolvedValue(null);
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => mockInvoke(...args) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const initialProjectState = useProjectStore.getState();

afterEach(() => {
  cleanup();
  useProjectStore.setState(initialProjectState, true);
  mockInvoke.mockClear();
});

describe('BarcodeDialog', () => {
  it('renders type dropdown and data input', () => {
    render(<BarcodeDialog layerId="layer-1" onClose={vi.fn()} />);
    expect(screen.getByText('Type')).toBeDefined();
    expect(screen.getByText('Data')).toBeDefined();
    expect(screen.getByText('Width (mm)')).toBeDefined();
    expect(screen.getByText('Height (mm)')).toBeDefined();
  });

  it('submit calls addObject with barcode data', async () => {
    const spy = vi.spyOn(useProjectStore.getState(), 'addObject').mockResolvedValue(makeProjectObject({ id: 'barcode-1', data: { type: 'barcode', barcode_type: 'qr_code', data: '', width: 10, height: 10 } }));
    const onClose = vi.fn();
    render(<BarcodeDialog layerId="layer-1" onClose={onClose} />);

    // Enter data in the text input - find the input within the Data label
    const dataInput = screen.getByText('Data').closest('label')?.querySelector('input');
    expect(dataInput).toBeTruthy();
    fireEvent.change(dataInput!, { target: { value: 'hello123' } });

    fireEvent.click(screen.getByTestId('barcode-submit'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith(
        'Barcode (QR Code)',
        'layer-1',
        expect.objectContaining({ type: 'barcode', barcode_type: 'qr_code', data: 'hello123' }),
        expect.any(Object),
      );
    });
    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
    spy.mockRestore();
  });

  it('stays open when barcode creation fails', async () => {
    const spy = vi.spyOn(useProjectStore.getState(), 'addObject').mockResolvedValue(null);
    const onClose = vi.fn();
    render(<BarcodeDialog layerId="layer-1" onClose={onClose} />);

    const dataInput = screen.getByText('Data').closest('label')?.querySelector('input');
    expect(dataInput).toBeTruthy();
    fireEvent.change(dataInput!, { target: { value: 'hello123' } });

    fireEvent.click(screen.getByTestId('barcode-submit'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalled();
    });
    expect(onClose).not.toHaveBeenCalled();
    spy.mockRestore();
  });

  it('refuses to create after the active project changes', async () => {
    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', ...{ project_id: 'p1', project_name: 'Project A', created_at: '', modified_at: '' } },
        workspace: { ...{ bed_width_mm: 400, bed_height_mm: 400 }, origin: 'top_left' as const },
        layers: [{ id: 'layer-1', name: 'Layer 1' }].map((l) => makeLayer(l)),
        objects: [],
        notes: '',
      }),
    });

    const spy = vi.spyOn(useProjectStore.getState(), 'addObject').mockResolvedValue(makeProjectObject({ id: 'barcode-1', data: { type: 'barcode', barcode_type: 'qr_code', data: '', width: 10, height: 10 } }));
    const onClose = vi.fn();
    render(<BarcodeDialog layerId="layer-1" onClose={onClose} />);

    const dataInput = screen.getByText('Data').closest('label')?.querySelector('input');
    expect(dataInput).toBeTruthy();
    fireEvent.change(dataInput!, { target: { value: 'hello123' } });

    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', ...{ project_id: 'p2', project_name: 'Project B', created_at: '', modified_at: '' } },
        workspace: { ...{ bed_width_mm: 400, bed_height_mm: 400 }, origin: 'top_left' as const },
        layers: [{ id: 'layer-2', name: 'Layer 2' }].map((l) => makeLayer(l)),
        objects: [],
        notes: '',
      }),
    });

    fireEvent.click(screen.getByTestId('barcode-submit'));

    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
    expect(spy).not.toHaveBeenCalled();
    spy.mockRestore();
  });
});

describe('NotesDialog', () => {
  it('renders textarea with existing notes', () => {
    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
        workspace: { ...{ bed_width_mm: 400, bed_height_mm: 400 }, origin: 'top_left' as const },
        layers: [].map((l) => makeLayer(l)),
        objects: [],
        notes: 'Existing notes text',
      }),
    });

    render(<NotesDialog onClose={vi.fn()} />);

    const textarea = screen.getByTestId('notes-textarea') as HTMLTextAreaElement;
    expect(textarea.value).toBe('Existing notes text');
  });

  it('closes on Escape', () => {
    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
        workspace: { ...{ bed_width_mm: 400, bed_height_mm: 400 }, origin: 'top_left' as const },
        layers: [],
        objects: [],
        notes: 'Existing notes text',
      }),
    });

    const onClose = vi.fn();
    render(<NotesDialog onClose={onClose} />);

    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });

    expect(onClose).toHaveBeenCalled();
  });

  it('closes on backdrop click', () => {
    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
        workspace: { ...{ bed_width_mm: 400, bed_height_mm: 400 }, origin: 'top_left' as const },
        layers: [],
        objects: [],
        notes: 'Existing notes text',
      }),
    });

    const onClose = vi.fn();
    render(<NotesDialog onClose={onClose} />);

    fireEvent.click(screen.getByRole('dialog'));

    expect(onClose).toHaveBeenCalled();
  });

  it('closes on Cancel button', () => {
    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
        workspace: { ...{ bed_width_mm: 400, bed_height_mm: 400 }, origin: 'top_left' as const },
        layers: [],
        objects: [],
        notes: 'Existing notes text',
      }),
    });

    const onClose = vi.fn();
    render(<NotesDialog onClose={onClose} />);

    fireEvent.click(screen.getByText('Cancel'));

    expect(onClose).toHaveBeenCalled();
  });

  it('save calls updateProjectNotes', async () => {
    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
        workspace: { ...{ bed_width_mm: 400, bed_height_mm: 400 }, origin: 'top_left' as const },
        layers: [].map((l) => makeLayer(l)),
        objects: [],
        notes: '',
      }),
    });

    const spy = vi.spyOn(useProjectStore.getState(), 'updateProjectNotes').mockResolvedValue(true);
    const onClose = vi.fn();
    render(<NotesDialog onClose={onClose} />);

    const textarea = screen.getByTestId('notes-textarea');
    fireEvent.change(textarea, { target: { value: 'New project notes' } });

    fireEvent.click(screen.getByTestId('notes-save'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith('New project notes');
    });
    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
    spy.mockRestore();
  });

  it('stays open when notes save fails', async () => {
    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', ...{ project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' } },
        workspace: { ...{ bed_width_mm: 400, bed_height_mm: 400 }, origin: 'top_left' as const },
        layers: [].map((l) => makeLayer(l)),
        objects: [],
        notes: '',
      }),
    });

    const spy = vi.spyOn(useProjectStore.getState(), 'updateProjectNotes').mockResolvedValue(false);
    const onClose = vi.fn();
    render(<NotesDialog onClose={onClose} />);

    fireEvent.change(screen.getByTestId('notes-textarea'), { target: { value: 'Retry me' } });
    fireEvent.click(screen.getByTestId('notes-save'));

    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith('Retry me');
    });
    expect(onClose).not.toHaveBeenCalled();
    spy.mockRestore();
  });

  it('closes itself when the active project changes', async () => {
    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', ...{ project_id: 'p1', project_name: 'Project A', created_at: '', modified_at: '' } },
        workspace: { ...{ bed_width_mm: 400, bed_height_mm: 400 }, origin: 'top_left' as const },
        layers: [].map((l) => makeLayer(l)),
        objects: [],
        notes: 'Existing notes text',
      }),
    });

    const onClose = vi.fn();
    render(<NotesDialog onClose={onClose} />);

    fireEvent.change(screen.getByTestId('notes-textarea'), { target: { value: 'Draft for A' } });

    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', ...{ project_id: 'p2', project_name: 'Project B', created_at: '', modified_at: '' } },
        workspace: { ...{ bed_width_mm: 400, bed_height_mm: 400 }, origin: 'top_left' as const },
        layers: [].map((l) => makeLayer(l)),
        objects: [],
        notes: 'Notes B',
      }),
    });

    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
  });

  it('refuses to save after the active project changes', async () => {
    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', ...{ project_id: 'p1', project_name: 'Project A', created_at: '', modified_at: '' } },
        workspace: { ...{ bed_width_mm: 400, bed_height_mm: 400 }, origin: 'top_left' as const },
        layers: [].map((l) => makeLayer(l)),
        objects: [],
        notes: 'Existing notes text',
      }),
    });

    const spy = vi.spyOn(useProjectStore.getState(), 'updateProjectNotes').mockResolvedValue(true);
    const onClose = vi.fn();
    render(<NotesDialog onClose={onClose} />);

    fireEvent.change(screen.getByTestId('notes-textarea'), { target: { value: 'Draft for A' } });

    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', ...{ project_id: 'p2', project_name: 'Project B', created_at: '', modified_at: '' } },
        workspace: { ...{ bed_width_mm: 400, bed_height_mm: 400 }, origin: 'top_left' as const },
        layers: [].map((l) => makeLayer(l)),
        objects: [],
        notes: 'Notes B',
      }),
    });

    fireEvent.click(screen.getByTestId('notes-save'));

    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
    expect(spy).not.toHaveBeenCalled();
    spy.mockRestore();
  });
});
