import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useProjectStore } from '../../../stores/projectStore';
import { makeLayer, makeProject, makeProjectObject } from '../../../test-utils/projectFixtures';
import { BarcodeDialog } from '../BarcodeDialog';

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
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it('stays open when barcode creation fails', async () => {
    const spy = vi.spyOn(useProjectStore.getState(), 'addObject').mockResolvedValue(null);
    const onClose = vi.fn();
    render(<BarcodeDialog layerId="layer-1" onClose={onClose} />);

    const dataInput = screen.getByText('Data').closest('label')?.querySelector('input');
    fireEvent.change(dataInput!, { target: { value: 'hello123' } });
    fireEvent.click(screen.getByTestId('barcode-submit'));

    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(onClose).not.toHaveBeenCalled();
  });

  it('refuses to create after the active project changes', async () => {
    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Project A', created_at: '', modified_at: '' },
        workspace: { bed_width_mm: 400, bed_height_mm: 400, origin: 'top_left' },
        layers: [makeLayer({ id: 'layer-1', name: 'Layer 1' })],
        objects: [],
        notes: '',
      }),
    });
    const spy = vi.spyOn(useProjectStore.getState(), 'addObject').mockResolvedValue(makeProjectObject({ id: 'barcode-1', data: { type: 'barcode', barcode_type: 'qr_code', data: '', width: 10, height: 10 } }));
    const onClose = vi.fn();
    render(<BarcodeDialog layerId="layer-1" onClose={onClose} />);

    const dataInput = screen.getByText('Data').closest('label')?.querySelector('input');
    fireEvent.change(dataInput!, { target: { value: 'hello123' } });
    act(() => useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p2', project_name: 'Project B', created_at: '', modified_at: '' },
        workspace: { bed_width_mm: 400, bed_height_mm: 400, origin: 'top_left' },
        layers: [makeLayer({ id: 'layer-2', name: 'Layer 2' })],
        objects: [],
        notes: '',
      }),
    }));
    fireEvent.click(screen.getByTestId('barcode-submit'));

    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(spy).not.toHaveBeenCalled();
  });
});
