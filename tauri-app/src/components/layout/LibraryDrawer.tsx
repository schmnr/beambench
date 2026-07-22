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

  if (!open) return null;

  return (
    <div
      className="no-select absolute bottom-2 left-[4.75rem] top-2 z-[35] flex w-[26rem] max-w-[calc(100%-5.5rem)] flex-col overflow-hidden rounded-xl border border-bb-border bg-bb-panel shadow-xl"
      data-testid="library-drawer"
    >
      <div className="flex items-center border-b border-bb-border px-2 pt-1.5">
        <button
          className={`flex-1 border-b-2 px-2 pb-1.5 text-xs ${
            tab === 'art'
              ? 'border-bb-accent font-semibold text-bb-accent'
              : 'border-transparent text-bb-text-muted hover:text-bb-text'
          }`}
          onClick={() => setTab('art')}
          data-testid="library-tab-art"
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
        >
          {t('panels.registry.material')}
        </button>
        <button
          className="ml-1 pb-1 text-bb-text-dim hover:text-bb-text"
          onClick={toggle}
          aria-label={t('common.close')}
        >
          <X size={13} />
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {tab === 'art' ? <ArtLibraryPanel /> : <MaterialLibrary />}
      </div>
    </div>
  );
}
