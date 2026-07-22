import { useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useMachineStore } from '../../stores/machineStore';
import { getPanelById, PANEL_COMPONENTS } from '../../panels';
import { DeviceSettingsDialog } from '../dialogs/DeviceSettingsDialog';

const RUN_TABS = ['laser', 'cuts_layers', 'move', 'console', 'macros'] as const;

/**
 * Run-mode right panel: one floating card, machine-first. Laser Control
 * leads; layers (cut order), jog, console, and macros ride along as tabs.
 */
export function RunPanel() {
  const { t } = useTranslation();
  const activeProfile = useMachineStore(
    (s) => (s.profiles ?? []).find((p) => p.id === s.activeProfileId) ?? null,
  );
  const [activeTab, setActiveTab] = useState<(typeof RUN_TABS)[number]>('laser');
  const [showProfiles, setShowProfiles] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  const PanelContent = PANEL_COMPONENTS[activeTab] ?? null;

  return (
    <div
      ref={containerRef}
      className="no-select h-full bg-bb-bg flex flex-col px-2 pb-2 pt-1.5"
      onContextMenu={(e) => e.preventDefault()}
      data-testid="run-panel"
    >
      <button
        className="z-10 -mb-px self-start rounded-t-lg bg-bb-accent px-3 py-1 text-xxs font-bold text-bb-on-accent hover:bg-bb-accent-hover"
        onClick={() => setShowProfiles(true)}
        title={t('panels.machine.laser.manage_machine_profiles')}
        data-testid="run-machine-profile-chip"
      >
        ⌗ {activeProfile?.name ?? t('panels.machine.laser.no_machine')}
      </button>

      <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-b-xl rounded-tr-xl border border-bb-border bg-bb-panel shadow-lg">
        <div className="flex border-b border-bb-border px-1 pt-1">
          {RUN_TABS.map((id) => {
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
                data-testid={`run-tab-${id}`}
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

      {showProfiles && <DeviceSettingsDialog onClose={() => setShowProfiles(false)} />}
    </div>
  );
}
