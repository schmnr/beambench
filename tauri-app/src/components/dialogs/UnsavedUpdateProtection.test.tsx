import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { UnsavedChangesDialog } from './UnsavedChangesDialog';
import { useProjectStore } from '../../stores/projectStore';
import { useUnsavedGuardStore } from '../../stores/unsavedGuardStore';
import { makeProject } from '../../test-utils/projectFixtures';

const originalSave = useProjectStore.getState().saveProject;

afterEach(() => {
  cleanup();
  useUnsavedGuardStore.getState().clear();
  useProjectStore.setState({ project: null, saveProject: originalSave });
});

it('keeps the pending action when saving leaves the document dirty', async () => {
  const execute = vi.fn();
  const cancel = vi.fn();
  const save = vi.fn(async () => {});
  useProjectStore.setState({ project: makeProject({ dirty: true }), saveProject: save });
  useUnsavedGuardStore.getState().request({ execute, cancel });
  const view = render(<UnsavedChangesDialog />);

  fireEvent.click(view.getByTestId('unsaved-changes-save'));
  await waitFor(() => expect(
    (view.getByTestId('unsaved-changes-cancel') as HTMLButtonElement).disabled,
  ).toBe(false));
  expect(save).toHaveBeenCalledOnce();
  expect(execute).not.toHaveBeenCalled();

  fireEvent.click(view.getByTestId('unsaved-changes-cancel'));
  expect(cancel).toHaveBeenCalledOnce();
  expect(execute).not.toHaveBeenCalled();
});
