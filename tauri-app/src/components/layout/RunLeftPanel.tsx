import { useTranslation } from 'react-i18next';
import { PANEL_COMPONENTS, getPanelById } from '../../panels';

const MOVE_PANEL_ID = 'move' as const;

/**
 * Run-mode left panel: a normal floating panel card (like the right one)
 * hosting jog/move; camera, macros, and console live under the laser panel.
 */
export function RunLeftPanel() {
  const { t } = useTranslation();
  const PanelContent = PANEL_COMPONENTS[MOVE_PANEL_ID] ?? null;
  const panelDefinition = getPanelById(MOVE_PANEL_ID);

  return (
    <div
      className="no-select flex h-full w-full flex-col bg-bb-bg px-2 pb-2 pt-1.5"
      onContextMenu={(e) => e.preventDefault()}
      data-testid="run-left-panel"
    >
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-bb-border bg-bb-panel shadow-lg">
        <div className="border-b border-bb-border px-3 py-1.5 text-xs font-semibold text-bb-text">
          {panelDefinition ? t(panelDefinition.titleKey) : t('panels.registry.move')}
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          {PanelContent && <PanelContent />}
        </div>
      </div>
    </div>
  );
}
