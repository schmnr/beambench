import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import type { ReactNode } from 'react';

let nativeMenuActive = false;

vi.mock('../../../utils/platform', () => ({
  isNativeMenuActive: () => nativeMenuActive,
}));
vi.mock('../MenuBar', () => ({ MenuBar: () => <div>File</div> }));
vi.mock('../MainToolbar', () => ({ MainToolbar: () => <div>MainToolbar</div> }));
vi.mock('../PropertiesToolbar', () => ({ PropertiesToolbar: () => <div>PropertiesToolbar</div> }));
vi.mock('../CreationToolbar', () => ({ CreationToolbar: () => <div>CreationToolbar</div> }));
vi.mock('../NodeSubToolbar', () => ({ NodeSubToolbar: () => <div>NodeSubToolbar</div> }));
vi.mock('../ModifiersToolbar', () => ({ ModifiersToolbar: () => <div>ModifiersToolbar</div> }));
vi.mock('../StatusBar', () => ({ StatusBar: () => <div>StatusBar</div> }));
vi.mock('../RightPanel', () => ({ RightPanel: () => <div>RightPanel</div> }));
vi.mock('../LeftPanel', () => ({ LeftPanel: () => <div>LeftPanel</div> }));
vi.mock('../BottomPanel', () => ({ BottomPanel: () => <div>BottomPanel</div> }));
vi.mock('../PanelResizer', () => ({ PanelResizer: () => <div>PanelResizer</div> }));
vi.mock('../FloatingPanelLayer', () => ({ FloatingPanelLayer: () => <div>FloatingPanelLayer</div> }));
vi.mock('../../canvas/Canvas', () => ({ Canvas: () => <div>Canvas</div> }));
vi.mock('../../import/ImportDropZone', () => ({ ImportDropZone: ({ children }: { children: ReactNode }) => <div>{children}</div> }));
vi.mock('../../../panels/DndContext', () => ({ PanelDndProvider: ({ children }: { children: ReactNode }) => <div>{children}</div> }));

import { AppShell } from '../AppShell';

afterEach(() => {
  cleanup();
  nativeMenuActive = false;
});

describe('AppShell native menu behavior', () => {
  it('renders the React menu bar off macOS', () => {
    nativeMenuActive = false;
    render(<AppShell />);
    expect(screen.getByText('File')).toBeDefined();
  });

  it('hides the React menu bar when the native menu is active', () => {
    nativeMenuActive = true;
    render(<AppShell />);
    expect(screen.queryByText('File')).toBeNull();
  });
});
