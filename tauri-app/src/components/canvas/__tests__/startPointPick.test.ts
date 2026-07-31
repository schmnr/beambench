import { afterEach, describe, expect, it } from 'vitest';
import { useUiStore } from '../../../stores/uiStore';
import { exitStartPointPickMode } from '../startPointPick';

const initialUiState = useUiStore.getState();

describe('exitStartPointPickMode', () => {
  afterEach(() => {
    useUiStore.setState(initialUiState, true);
  });

  it('clears the pending object and returns the canvas to Select', () => {
    useUiStore.setState({
      activeTool: 'node',
      pendingStartPointObjectId: 'vector-1',
      workspaceMode: 'design',
    });

    exitStartPointPickMode();

    expect(useUiStore.getState().pendingStartPointObjectId).toBeNull();
    expect(useUiStore.getState().activeTool).toBe('select');
  });
});
