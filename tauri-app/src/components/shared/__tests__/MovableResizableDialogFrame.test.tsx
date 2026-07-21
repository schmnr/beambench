import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { MovableResizableDialogFrame } from '../MovableResizableDialogFrame';

type FrameSnapshot = {
  left: number;
  top: number;
  width: number;
  height: number;
};

const reactStateMock = vi.hoisted(() => ({
  deferFrameUpdaters: false,
  pendingFrameUpdaters: [] as Array<(current: FrameSnapshot) => FrameSnapshot>,
}));

vi.mock('react', async (importOriginal) => {
  const actual = await importOriginal<typeof import('react')>();
  return {
    ...actual,
    useState: <T,>(initialState: T | (() => T)) => {
      const [state, setState] = actual.useState(initialState);
      const isFrameState =
        typeof state === 'object' &&
        state !== null &&
        'left' in state &&
        'top' in state &&
        'width' in state &&
        'height' in state;
      if (!isFrameState) {
        return [state, setState];
      }

      const wrappedSetState: typeof setState = (nextState) => {
        if (reactStateMock.deferFrameUpdaters && typeof nextState === 'function') {
          reactStateMock.pendingFrameUpdaters.push(
            nextState as (current: FrameSnapshot) => FrameSnapshot,
          );
          return;
        }
        setState(nextState);
      };
      return [state, wrappedSetState];
    },
  };
});

afterEach(() => {
  cleanup();
  reactStateMock.deferFrameUpdaters = false;
  reactStateMock.pendingFrameUpdaters.length = 0;
});

function renderFrame() {
  render(
    <MovableResizableDialogFrame
      title="Test dialog"
      titleId="test-dialog-title"
      testId="test-dialog"
      initialWidth={320}
      initialHeight={240}
      minWidth={160}
      minHeight={120}
      footer={<button type="button">Done</button>}
    >
      <div>Dialog body</div>
    </MovableResizableDialogFrame>,
  );
}

function readFrame(): FrameSnapshot {
  const dialog = screen.getByTestId('test-dialog');
  return {
    left: parseFloat(dialog.style.left),
    top: parseFloat(dialog.style.top),
    width: parseFloat(dialog.style.width),
    height: parseFloat(dialog.style.height),
  };
}

function takePendingFrameUpdate(): (current: FrameSnapshot) => FrameSnapshot {
  expect(reactStateMock.pendingFrameUpdaters).toHaveLength(1);
  const [updater] = reactStateMock.pendingFrameUpdaters.splice(0, 1);
  return updater;
}

describe('MovableResizableDialogFrame', () => {
  it('uses a drag snapshot when the frame updater runs after mouseup clears refs', () => {
    renderFrame();
    const startFrame = readFrame();
    reactStateMock.deferFrameUpdaters = true;

    fireEvent.mouseDown(screen.getByTestId('test-dialog-drag-handle'), {
      clientX: 100,
      clientY: 100,
    });
    fireEvent.mouseMove(document, { clientX: 140, clientY: 130 });
    fireEvent.mouseUp(document);

    let nextFrame: FrameSnapshot | undefined;
    expect(() => {
      nextFrame = takePendingFrameUpdate()(startFrame);
    }).not.toThrow();
    expect(nextFrame).toBeDefined();
    expect(nextFrame!.left).toBe(startFrame.left + 40);
    expect(nextFrame!.top).toBe(startFrame.top + 30);
  });

  it('uses a resize snapshot when the frame updater runs after mouseup clears refs', () => {
    renderFrame();
    const startFrame = readFrame();
    reactStateMock.deferFrameUpdaters = true;

    fireEvent.mouseDown(screen.getByTestId('test-dialog-resize-handle'), {
      clientX: 300,
      clientY: 300,
    });
    fireEvent.mouseMove(document, { clientX: 360, clientY: 340 });
    fireEvent.mouseUp(document);

    let nextFrame: FrameSnapshot | undefined;
    expect(() => {
      nextFrame = takePendingFrameUpdate()(startFrame);
    }).not.toThrow();
    expect(nextFrame).toBeDefined();
    expect(nextFrame!.width).toBe(startFrame.width + 60);
    expect(nextFrame!.height).toBe(startFrame.height + 40);
  });
});
