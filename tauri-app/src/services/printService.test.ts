import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { printService } from './printService';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const PRINT_ROOT_ID = 'beam-bench-print-root';

beforeEach(() => {
  vi.useFakeTimers();
  vi.mocked(invoke).mockReset();
  document.body.innerHTML = '<div id="app-root">App UI</div>';
  document.head.innerHTML = '';
  document.title = 'Beam Bench';
  Object.defineProperty(window, 'print', { value: vi.fn(), configurable: true });
  vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback: FrameRequestCallback) => {
    callback(0);
    return 0;
  });
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  document.body.innerHTML = '';
  document.head.innerHTML = '';
});

describe('printService', () => {
  it('renders print SVG into the top-level document before invoking native webview print', async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === 'render_print_document') {
        return {
          title: 'Print Test',
          svg: '<svg data-testid="print-svg" width="100mm" height="50mm"></svg>',
        };
      }
      if (command === 'print_current_webview') return undefined;
      throw new Error(`Unexpected command ${command}`);
    });

    await printService.printProject('black');

    expect(invoke).toHaveBeenNthCalledWith(1, 'render_print_document', { mode: 'black' });
    expect(invoke).toHaveBeenNthCalledWith(2, 'print_current_webview');
    expect(document.getElementById(PRINT_ROOT_ID)?.querySelector('svg')).not.toBeNull();
    expect(window.print).not.toHaveBeenCalled();

    window.dispatchEvent(new Event('afterprint'));
    expect(document.getElementById(PRINT_ROOT_ID)).toBeNull();
    expect(document.title).toBe('Beam Bench');
  });

  it('falls back to window.print when native webview print is unavailable', async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === 'render_print_document') {
        return {
          title: 'Print Test',
          svg: '<svg data-testid="print-svg" width="100mm" height="50mm"></svg>',
        };
      }
      if (command === 'print_current_webview') throw new Error('unsupported');
      throw new Error(`Unexpected command ${command}`);
    });

    await printService.printProject('color');

    expect(invoke).toHaveBeenNthCalledWith(1, 'render_print_document', { mode: 'color' });
    expect(window.print).toHaveBeenCalledOnce();
    window.dispatchEvent(new Event('afterprint'));
  });
});
