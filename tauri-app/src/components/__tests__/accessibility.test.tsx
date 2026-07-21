import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/react';
import { MainToolbar } from '../layout/MainToolbar';
import { CreationToolbar } from '../layout/CreationToolbar';
import { OffsetDialog } from '../dialogs/OffsetDialog';
import { GridArrayDialog } from '../dialogs/GridArrayDialog';
import { useProjectStore } from '../../stores/projectStore';
import { useUndoStore } from '../../stores/undoStore';
import { useUiStore } from '../../stores/uiStore';
import { useNotificationStore } from '../../stores/notificationStore';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const initialUndoState = useUndoStore.getState();
const initialProjectState = useProjectStore.getState();
const initialUiState = useUiStore.getState();
const initialNotificationState = useNotificationStore.getState();

afterEach(() => {
  cleanup();
  useUndoStore.setState(initialUndoState, true);
  useProjectStore.setState(initialProjectState, true);
  useUiStore.setState(initialUiState, true);
  useNotificationStore.setState(initialNotificationState, true);
});

describe('Accessibility: icon buttons have accessible names', () => {
  it('all MainToolbar buttons have aria-label or text content', () => {
    render(<MainToolbar />);
    const buttons = document.querySelectorAll('button');
    for (const btn of buttons) {
      const hasAccessibleName =
        (btn.getAttribute('aria-label') && btn.getAttribute('aria-label')!.length > 0) ||
        (btn.textContent && btn.textContent.trim().length > 0);
      expect(hasAccessibleName, `Button missing accessible name: ${btn.outerHTML.slice(0, 120)}`).toBe(true);
    }
  });

  it('all CreationToolbar buttons have aria-label or text content', () => {
    render(<CreationToolbar />);
    const buttons = document.querySelectorAll('button');
    expect(buttons.length).toBeGreaterThan(0);
    for (const btn of buttons) {
      const hasAccessibleName =
        (btn.getAttribute('aria-label') && btn.getAttribute('aria-label')!.length > 0) ||
        (btn.textContent && btn.textContent.trim().length > 0);
      expect(hasAccessibleName, `Button missing accessible name: ${btn.outerHTML.slice(0, 120)}`).toBe(true);
    }
  });
});

describe('Accessibility: dialogs have role and aria-labelledby', () => {
  it('OffsetDialog renders with role="dialog" and aria-labelledby', () => {
    render(<OffsetDialog objectIds={['obj-1']} onClose={vi.fn()} />);
    const dialog = document.querySelector('[role="dialog"]');
    expect(dialog).not.toBeNull();
    expect(dialog!.getAttribute('aria-modal')).toBe('true');
    expect(dialog!.getAttribute('aria-labelledby')).toBe('dialog-title');
    const title = document.getElementById('dialog-title');
    expect(title).not.toBeNull();
    expect(title!.textContent).toBe('Offset Shapes');
  });

  it('GridArrayDialog renders with role="dialog" and aria-labelledby', () => {
    // GridArrayDialog reads `project.metadata.project_id` for the
    // project-guard pattern — the fixture must include metadata.
    useProjectStore.setState({
      selectedObjectIds: ['obj-1'],
      project: {
        metadata: {
          format_version: '1.0',
          app_version: '0.1.0',
          project_id: 'test-project',
          project_name: 'Test',
          created_at: '2026-04-16',
          modified_at: '2026-04-16',
        },
        workspace: { bed_width_mm: 400, bed_height_mm: 400, origin: 'top_left' as const },
        objects: [{ id: 'obj-1', locked: false, bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } } }],
        layers: [],
        assets: [],
      } as never,
    });
    render(<GridArrayDialog objectIds={['obj-1']} onClose={vi.fn()} />);
    const dialog = document.querySelector('[role="dialog"]');
    expect(dialog).not.toBeNull();
    expect(dialog!.getAttribute('aria-modal')).toBe('true');
    expect(dialog!.getAttribute('aria-labelledby')).toBe('dialog-title');
    const title = document.getElementById('dialog-title');
    expect(title).not.toBeNull();
    expect(title!.textContent).toBe('Grid Array');
  });
});
