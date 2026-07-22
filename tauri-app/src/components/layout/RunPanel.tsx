import { useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useMachineStore } from '../../stores/machineStore';
import { PANEL_COMPONENTS } from '../../panels';
import { DeviceSettingsDialog } from '../dialogs/DeviceSettingsDialog';

/**
 * Run-mode right panel: one floating card, machine-first. Laser Control,
 * full height; jog/console/macros live on the Run rail, layers in Design.
 */
export function RunPanel() {
  const { t } = useTranslation();
  const activeProfile = useMachineStore(
    (s) => (s.profiles ?? []).find((p) => p.id === s.activeProfileId) ?? null,
  );
  const [showProfiles, setShowProfiles] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  const PanelContent = PANEL_COMPONENTS['laser'] ?? null;

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
        <div className="min-h-0 flex-1 overflow-y-auto">
          {PanelContent && <PanelContent />}
        </div>
      </div>

      {showProfiles && <DeviceSettingsDialog onClose={() => setShowProfiles(false)} />}
    </div>
  );
}
