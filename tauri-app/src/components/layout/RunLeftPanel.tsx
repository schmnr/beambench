import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { PANEL_COMPONENTS, getPanelById } from '../../panels';

const RUN_LEFT_TABS = ['move'] as const;

/**
 * Run-mode left panel: a normal floating panel card (like the right one)
 * hosting jog/move; camera, macros, and console live under the laser panel.
 */
export function RunLeftPanel() {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<(typeof RUN_LEFT_TABS)[number]>('move');

  const PanelContent = PANEL_COMPONENTS[activeTab] ?? null;

  return (
    <div
      className="no-select flex h-full w-full flex-col bg-bb-bg px-2 pb-2 pt-1.5"
      onContextMenu={(e) => e.preventDefault()}
      data-testid="run-left-panel"
    >
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-bb-border bg-bb-panel shadow-lg">
        <div className="flex border-b border-bb-border px-1 pt-1">
          {RUN_LEFT_TABS.map((id) => {
            const def = getPanelById(id);
            return (
              <button
                key={id}
                className={`flex-1 truncate border-b-2 px-1.5 pb-1.5 pt-0.5 text-xs ${
                  activeTab === id
                    ? 'border-bb-accent font-semibold text-bb-accent'
                    : 'border-transparent text-bb-text-muted hover:text-bb-text'
                }`}
                onClick={() => setActiveTab(id)}
                data-testid={`run-left-tab-${id}`}
              >
                {def ? t(def.titleKey) : id}
              </button>
            );
          })}
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          {PanelContent && <PanelContent />}
        </div>
      </div>
    </div>
  );
}
