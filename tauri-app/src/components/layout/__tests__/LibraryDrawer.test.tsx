import '@testing-library/jest-dom/vitest';
import { beforeEach, afterEach, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { LibraryDrawer } from '../LibraryDrawer';
import {
  useConnectedLibraryStore as cg,
  usePrintcutcarveLibraryStore as pcc,
} from '../../../stores/connectedLibraryStore';
import { useUiStore } from '../../../stores/uiStore';
vi.mock('../../panels/CraftgineerPanel', () => ({
  CraftgineerPanel: ({ provider, connections }: { provider: string; connections: boolean }) => (
    <p>
      {provider}:{connections ? 'connections' : 'files'}
    </p>
  ),
}));
vi.mock('../../panels/ArtLibraryPanel', () => ({ ArtLibraryPanel: () => <p>Local art</p> }));
vi.mock('../../machine/MaterialLibrary', () => ({ MaterialLibrary: () => <p>Materials</p> }));
const status = {
  account: { account: { id: 'maker', displayName: 'Maker' }, access: 'active' as const },
  remembered: false,
  walletUnavailable: false,
};
beforeEach(() => {
  useUiStore.setState({ libraryDrawerOpen: true, libraryDrawerTab: 'art' });
  cg.setState({ status });
  pcc.setState({ status });
});
afterEach(cleanup);
it('shows both provider tabs and supports arrow, Home and End navigation', () => {
  render(<LibraryDrawer />);
  const tabs = screen.getAllByRole('tab');
  expect(tabs).toHaveLength(4);
  fireEvent.keyDown(tabs[0], { key: 'End' });
  expect(tabs[3]).toHaveFocus();
  expect(screen.getByText('printcutcarve:files')).toBeTruthy();
  fireEvent.keyDown(tabs[3], { key: 'ArrowLeft' });
  expect(tabs[2]).toHaveFocus();
  expect(screen.getByText('craftgineer:files')).toBeTruthy();
  fireEvent.keyDown(tabs[2], { key: 'Home' });
  expect(tabs[0]).toHaveFocus();
});
it('keeps Craftgineer visible when PrintCutCarve disconnects and offers its connection controls', () => {
  useUiStore.setState({ libraryDrawerTab: 'printcutcarve' });
  render(<LibraryDrawer />);
  act(() => pcc.setState({ status: null }));
  expect(screen.getByRole('tab', { name: 'Craftgineer' })).toBeTruthy();
  expect(screen.getByRole('tab', { name: 'PrintCutCarve' })).toHaveAttribute('aria-selected', 'true');
  expect(screen.getByText('printcutcarve:connections')).toBeTruthy();
  fireEvent.click(screen.getByRole('tab', { name: 'Craftgineer' }));
  expect(screen.getByText('craftgineer:files')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Manage connections' }));
  fireEvent.click(screen.getByRole('button', { name: 'Craftgineer' }));
  expect(screen.getByText('craftgineer:connections')).toBeTruthy();
});
it('keeps both tabs discoverable before login and opens the library after connecting', () => {
  cg.setState({ status: null });
  pcc.setState({ status: null });
  render(<LibraryDrawer />);
  expect(screen.getAllByRole('tab')).toHaveLength(4);
  fireEvent.click(screen.getByRole('tab', { name: 'Craftgineer' }));
  expect(screen.getByText('craftgineer:connections')).toBeTruthy();
  fireEvent.click(screen.getByRole('tab', { name: 'PrintCutCarve' }));
  expect(screen.getByText('printcutcarve:connections')).toBeTruthy();
  act(() => pcc.setState({ status }));
  expect(screen.getByText('printcutcarve:files')).toBeTruthy();
  expect(screen.getByRole('tab', { name: 'PrintCutCarve' })).toHaveAttribute('aria-selected', 'true');
});
