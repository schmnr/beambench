import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { PrintDialog } from '../PrintDialog';
import { printService } from '../../../services/printService';

vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe('PrintDialog', () => {
  it('prints with operation-aware black output by default', async () => {
    const printProject = vi.spyOn(printService, 'printProject').mockResolvedValue(undefined);
    const onClose = vi.fn();

    render(<PrintDialog onClose={onClose} />);
    fireEvent.click(screen.getByRole('button', { name: 'Print...' }));

    await waitFor(() => {
      expect(printProject).toHaveBeenCalledWith('black', 'operation');
      expect(onClose).toHaveBeenCalledOnce();
    });
  });

  it('offers layer-color outline printing as an explicit alternative', async () => {
    const printProject = vi.spyOn(printService, 'printProject').mockResolvedValue(undefined);

    render(<PrintDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByRole('radio', { name: 'Layer Colors' }));
    fireEvent.click(screen.getByRole('radio', { name: 'Outlines Only' }));
    fireEvent.click(screen.getByRole('button', { name: 'Print...' }));

    await waitFor(() => {
      expect(printProject).toHaveBeenCalledWith('color', 'outline');
    });
  });
});
