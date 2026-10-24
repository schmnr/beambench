import { CraftgineerPanel } from '../panels/CraftgineerPanel';
import { useConnectedLibraryStore } from '../../stores/connectedLibraryStore';
import { useTranslation } from 'react-i18next';
import { X } from 'lucide-react';
import { useUiStore } from '../../stores/uiStore';
import { ArtLibraryPanel } from '../panels/ArtLibraryPanel';
import { MaterialLibrary } from '../machine/MaterialLibrary';

/**
 * Library drawer opened from the tool rail's four-squares launcher.
 * Tabs: Art (the Art Library) and Materials (the Material Library).
 */
export function LibraryDrawer() {
  const { t } = useTranslation();
  const open = useUiStore((s) => s.libraryDrawerOpen);
  const tab = useUiStore((s) => s.libraryDrawerTab);
  const setTab = useUiStore((s) => s.setLibraryDrawerTab);
  const toggle = useUiStore((s) => s.toggleLibraryDrawer);

  const connected = useConnectedLibraryStore((s) => !!s.status?.account);
  if (!open) return null;

  return (
    <div
      className="no-select absolute bottom-2 left-[4.75rem] top-2 z-[35] flex w-[26rem] max-w-[calc(100%-5.5rem)] flex-col overflow-hidden rounded-xl border border-bb-border bg-bb-panel shadow-xl"
      data-testid="library-drawer"
    >
      <div
        className="flex items-center border-b border-bb-border px-2 pt-1.5"
        role="tablist"
        onKeyDown={(event) => {
          if ((event.target as HTMLElement).getAttribute('role') !== 'tab') return;
          const tabs = Array.from(
            event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="tab"]'),
          );
          const current = tabs.indexOf(event.target as HTMLButtonElement);
          const next =
            event.key === 'ArrowRight'
              ? (current + 1) % tabs.length
              : event.key === 'ArrowLeft'
                ? (current - 1 + tabs.length) % tabs.length
                : event.key === 'Home'
                  ? 0
                  : event.key === 'End'
                    ? tabs.length - 1
                    : -1;
          if (next >= 0) {
            event.preventDefault();
            tabs[next].focus();
            tabs[next].click();
          }
        }}
      >
        <button
          className={`flex-1 border-b-2 px-2 pb-1.5 text-xs ${
            tab === 'art'
              ? 'border-bb-accent font-semibold text-bb-accent'
              : 'border-transparent text-bb-text-muted hover:text-bb-text'
          }`}
          onClick={() => setTab('art')}
          data-testid="library-tab-art"
          role="tab"
          aria-selected={tab === 'art'}
          tabIndex={tab === 'art' ? 0 : -1}
        >
          {t('panels.registry.art_library')}
        </button>
        <button
          className={`flex-1 border-b-2 px-2 pb-1.5 text-xs ${
            tab === 'materials'
              ? 'border-bb-accent font-semibold text-bb-accent'
              : 'border-transparent text-bb-text-muted hover:text-bb-text'
          }`}
          onClick={() => setTab('materials')}
          data-testid="library-tab-materials"
          role="tab"
          aria-selected={tab === 'materials'}
          tabIndex={tab === 'materials' ? 0 : -1}
        >
          {t('panels.registry.material')}
        </button>
        {connected && (
          <button
            role="tab"
            tabIndex={tab === 'craftgineer' ? 0 : -1}
            aria-selected={tab === 'craftgineer'}
            className={`flex-1 border-b-2 px-2 pb-1.5 text-xs ${tab === 'craftgineer' ? 'border-bb-accent text-bb-accent' : 'border-transparent text-bb-text-muted'}`}
            onClick={() => setTab('craftgineer')}
          >
            {t('connected_library.provider')}
          </button>
        )}
        <button
          className="ml-1 pb-1 text-bb-text-dim hover:text-bb-text"
          onClick={toggle}
          aria-label={t('common.close')}
        >
          <X size={13} />
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {tab === 'art' ? (
          <ArtLibraryPanel />
        ) : tab === 'materials' ? (
          <MaterialLibrary />
        ) : (
          <CraftgineerPanel key={tab} connections={tab === 'connections' || !connected} />
        )}
      </div>
      <button
        className="border-t border-bb-border px-3 py-2 text-left text-xs text-bb-text-muted hover:text-bb-text"
        onClick={() => setTab('connections')}
      >
        {t('connected_library.manage')}
      </button>
    </div>
  );
}
