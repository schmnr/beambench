import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { FloatingPanel } from '../FloatingPanel';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));

describe('FloatingPanel', () => {
  const defaultProps = {
    panelId: 'test_panel',
    title: 'Test Panel',
    x: 100,
    y: 200,
    width: 400,
    height: 300,
    zIndex: 1,
    onClose: vi.fn(),
    onDock: vi.fn(),
    onMove: vi.fn(),
    onResize: vi.fn(),
    onFocus: vi.fn(),
  };

  it('renders panel with title and content', () => {
    render(
      <FloatingPanel {...defaultProps}>
        <div data-testid="panel-content">Hello</div>
      </FloatingPanel>,
    );
    expect(screen.getByText('Test Panel')).toBeDefined();
    expect(screen.getByTestId('panel-content')).toBeDefined();
  });

  it('close button calls onClose', () => {
    const onClose = vi.fn();
    render(
      <FloatingPanel {...defaultProps} onClose={onClose}>
        <div>Content</div>
      </FloatingPanel>,
    );
    fireEvent.click(screen.getByTitle('Close panel'));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('dock button calls onDock', () => {
    const onDock = vi.fn();
    render(
      <FloatingPanel {...defaultProps} onDock={onDock}>
        <div>Content</div>
      </FloatingPanel>,
    );
    fireEvent.click(screen.getByTitle('Dock panel'));
    expect(onDock).toHaveBeenCalledTimes(1);
  });

  it('applies correct z-index style', () => {
    render(
      <FloatingPanel {...defaultProps} zIndex={5}>
        <div>Content</div>
      </FloatingPanel>,
    );
    const panel = screen.getByTestId('floating-panel-test_panel');
    expect(panel.style.zIndex).toBe('35'); // 30 + 5
  });

  it('mousedown on panel calls onFocus', () => {
    const onFocus = vi.fn();
    render(
      <FloatingPanel {...defaultProps} onFocus={onFocus}>
        <div data-testid="panel-content">Content</div>
      </FloatingPanel>,
    );
    fireEvent.mouseDown(screen.getByTestId('floating-panel-test_panel'));
    expect(onFocus).toHaveBeenCalled();
  });

  it('cleans up document listeners on unmount during drag', () => {
    const removeSpy = vi.spyOn(document, 'removeEventListener');
    const { unmount } = render(
      <FloatingPanel {...defaultProps}>
        <div>Content</div>
      </FloatingPanel>,
    );
    // Find the title bar (cursor-move element) and initiate a drag
    const titleBar = screen.getByText('Test Panel').closest('[class*="cursor-move"]')!;
    fireEvent.mouseDown(titleBar, { clientX: 100, clientY: 200 });

    // Unmount mid-drag — cleanup effect should remove listeners
    unmount();

    const removeCalls = removeSpy.mock.calls.filter(
      ([type]) => type === 'mousemove' || type === 'mouseup',
    );
    expect(removeCalls.length).toBeGreaterThanOrEqual(2);
    removeSpy.mockRestore();
  });

  it('cleans up document listeners on unmount during resize', () => {
    const removeSpy = vi.spyOn(document, 'removeEventListener');
    const { unmount, container } = render(
      <FloatingPanel {...defaultProps}>
        <div>Content</div>
      </FloatingPanel>,
    );
    // Find the resize handle (cursor-nwse-resize element)
    const resizeHandle = container.querySelector('[class*="cursor-nwse-resize"]')
      ?? document.querySelector('[class*="cursor-nwse-resize"]');
    expect(resizeHandle).not.toBeNull();
    fireEvent.mouseDown(resizeHandle!, { clientX: 500, clientY: 500 });

    // Unmount mid-resize — cleanup effect should remove listeners
    unmount();

    const removeCalls = removeSpy.mock.calls.filter(
      ([type]) => type === 'mousemove' || type === 'mouseup',
    );
    expect(removeCalls.length).toBeGreaterThanOrEqual(2);
    removeSpy.mockRestore();
  });
});
