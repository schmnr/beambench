import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { ResizeSlotsDialog } from '../ResizeSlotsDialog';
import { useProjectStore } from '../../../stores/projectStore';
import { makeProject } from '../../../test-utils/projectFixtures';

const initialProjectState = useProjectStore.getState();

afterEach(() => {
  cleanup();
  useProjectStore.setState(initialProjectState, true);
  vi.restoreAllMocks();
});

describe('ResizeSlotsDialog', () => {
  it('blocks invalid numeric input before applying', async () => {
    const resizeSlots = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({
      project: makeProject(),
      resizeSlots,
    });
    render(<ResizeSlotsDialog objectIds={['obj-1']} onClose={vi.fn()} />);

    fireEvent.change(screen.getByLabelText('Old Material Thickness (mm)'), {
      target: { value: '0' },
    });

    expect(screen.getByText(/Old Material Thickness and New Thickness must be greater than 0/i)).toBeDefined();
    const applyButton = screen.getByText('Apply').closest('button')!;
    expect(applyButton.disabled).toBe(true);
    fireEvent.click(applyButton);
    expect(resizeSlots).not.toHaveBeenCalled();
  });

  it('traps Tab focus inside the dialog', () => {
    useProjectStore.setState({
      project: makeProject(),
      resizeSlots: vi.fn().mockResolvedValue(true),
    });
    render(
      <>
        <button data-testid="outside">outside</button>
        <ResizeSlotsDialog objectIds={['obj-1']} onClose={vi.fn()} />
      </>,
    );

    const dialog = screen.getByRole('dialog');
    const focusable = Array.from(dialog.querySelectorAll<HTMLElement>(
      'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
    ));
    expect(focusable.length).toBeGreaterThan(1);

    // Mount focus lands on the first focusable control inside the dialog.
    expect(document.activeElement).toBe(focusable[0]);

    // Tab from the last control wraps to the first instead of escaping.
    const last = focusable[focusable.length - 1];
    last.focus();
    fireEvent.keyDown(last, { key: 'Tab' });
    expect(document.activeElement).toBe(focusable[0]);

    // Shift+Tab from the first control wraps to the last.
    fireEvent.keyDown(focusable[0], { key: 'Tab', shiftKey: true });
    expect(document.activeElement).toBe(last);
  });

  it('closes when the active project changes', async () => {
    useProjectStore.setState({ project: makeProject() });
    const onClose = vi.fn();
    render(<ResizeSlotsDialog objectIds={['obj-1']} onClose={onClose} />);

    await act(async () => {
      useProjectStore.setState({
        project: makeProject({
          metadata: {
            format_version: '1',
            app_version: '0.1.0',
            project_id: 'other',
            project_name: 'Other Project',
            created_at: '',
            modified_at: '',
          },
        }),
      });
    });

    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });
});
