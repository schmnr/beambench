import { useEffect, useRef, useState } from 'react';
import { CraftgineerPanel } from '../panels/CraftgineerPanel';
import {
  useConnectedLibraryStore,
  usePrintcutcarveLibraryStore,
} from '../../stores/connectedLibraryStore';
import type { LibraryProvider } from '../../services/connectedLibraryService';
import { useTranslation } from 'react-i18next';
import { X } from 'lucide-react';
import { useUiStore } from '../../stores/uiStore';
import { ArtLibraryPanel } from '../panels/ArtLibraryPanel';
import { MaterialLibrary } from '../machine/MaterialLibrary';
const providers = ['craftgineer', 'printcutcarve'] as const;

export function LibraryDrawer() {
  const { t } = useTranslation();
  const open = useUiStore((s) => s.libraryDrawerOpen);
  const tab = useUiStore((s) => s.libraryDrawerTab);
  const setTab = useUiStore((s) => s.setLibraryDrawerTab);
  const toggle = useUiStore((s) => s.toggleLibraryDrawer);
  const craftgineer = useConnectedLibraryStore((s) => !!s.status?.account);
  const printcutcarve = usePrintcutcarveLibraryStore((s) => !!s.status?.account);
  const [connectionProvider, setConnectionProvider] = useState<LibraryProvider>('craftgineer');
  const activeTab = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    activeTab.current?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' });
  }, [open, tab, craftgineer, printcutcarve]);
  if (!open) return null;
  const tabs = [
    { id: 'art' as const, label: t('panels.registry.art_library') },
    { id: 'materials' as const, label: t('panels.registry.material') },
    ...(craftgineer
      ? [{ id: 'craftgineer' as const, label: t('connected_library.provider') }]
      : []),
    ...(printcutcarve
      ? [{ id: 'printcutcarve' as const, label: t('printcutcarve_library.provider') }]
      : []),
  ];
  const provider = tab === 'printcutcarve' || tab === 'craftgineer' ? tab : connectionProvider;
  const connections =
    tab === 'connections' ||
    (tab === 'craftgineer' && !craftgineer) ||
    (tab === 'printcutcarve' && !printcutcarve);
  const visibleSelected = tabs.some((value) => value.id === tab);
  return (
    <div
      className="no-select absolute bottom-2 left-[4.75rem] top-2 z-[35] flex w-[26rem] max-w-[calc(100%-5.5rem)] flex-col overflow-hidden rounded-xl border border-bb-border bg-bb-panel shadow-xl"
      data-testid="library-drawer"
    >
      <div className="flex items-center border-b border-bb-border px-2 pt-1.5">
        <div
          className="flex min-w-0 flex-1 overflow-x-auto"
          role="tablist"
          onKeyDown={(event) => {
            if ((event.target as HTMLElement).getAttribute('role') !== 'tab') return;
            const buttons = Array.from(
              event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="tab"]'),
            );
            const current = buttons.indexOf(event.target as HTMLButtonElement);
            const next =
              event.key === 'ArrowRight'
                ? (current + 1) % buttons.length
                : event.key === 'ArrowLeft'
                  ? (current - 1 + buttons.length) % buttons.length
                  : event.key === 'Home'
                    ? 0
                    : event.key === 'End'
                      ? buttons.length - 1
                      : -1;
            if (next >= 0) {
              event.preventDefault();
              buttons[next].focus();
              buttons[next].click();
            }
          }}
        >
          {tabs.map((value) => (
            <button
              key={value.id}
              ref={tab === value.id ? activeTab : undefined}
              id={`library-tab-${value.id}`}
              data-testid={`library-tab-${value.id}`}
              className={`shrink-0 whitespace-nowrap border-b-2 px-3 pb-1.5 text-xs ${tab === value.id ? 'border-bb-accent font-semibold text-bb-accent' : 'border-transparent text-bb-text-muted hover:text-bb-text'}`}
              onClick={() => setTab(value.id)}
              role="tab"
              aria-selected={tab === value.id}
              aria-controls="library-content"
              tabIndex={tab === value.id || (!visibleSelected && value.id === 'art') ? 0 : -1}
            >
              {value.label}
            </button>
          ))}
        </div>
        <button
          className="ml-1 shrink-0 pb-1 text-bb-text-dim hover:text-bb-text"
          onClick={toggle}
          aria-label={t('common.close')}
        >
          <X size={13} />
        </button>
      </div>
      <div
        id="library-content"
        className="min-h-0 flex-1 overflow-y-auto"
        role="tabpanel"
        aria-labelledby={visibleSelected ? `library-tab-${tab}` : undefined}
        aria-label={visibleSelected ? undefined : t('connected_library.manage')}
      >
        {tab === 'art' ? (
          <ArtLibraryPanel />
        ) : tab === 'materials' ? (
          <MaterialLibrary />
        ) : (
          <>
            {connections && (
              <div
                className="flex flex-wrap gap-2 border-b border-bb-border p-3"
                role="group"
                aria-label={t('connected_library.manage')}
              >
                {providers.map((value) => (
                  <button
                    key={value}
                    aria-pressed={provider === value}
                    className={`rounded border px-3 py-1.5 text-xs ${provider === value ? 'border-bb-accent text-bb-accent' : 'border-bb-border text-bb-text-muted'}`}
                    onClick={() => {
                      setConnectionProvider(value);
                      setTab('connections');
                    }}
                  >
                    {t(
                      `${value === 'craftgineer' ? 'connected_library' : 'printcutcarve_library'}.provider`,
                    )}
                  </button>
                ))}
              </div>
            )}
            <CraftgineerPanel
              key={`${provider}-${connections}`}
              provider={provider}
              connections={connections}
            />
          </>
        )}
      </div>
      <button
        className="border-t border-bb-border px-3 py-2 text-left text-xs text-bb-text-muted hover:text-bb-text"
        onClick={() => {
          setConnectionProvider(provider);
          setTab('connections');
        }}
      >
        {t('connected_library.manage')}
      </button>
    </div>
  );
}
