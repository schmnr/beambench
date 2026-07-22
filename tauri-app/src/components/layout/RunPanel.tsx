import { useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useMachineStore } from '../../stores/machineStore';
import { PANEL_COMPONENTS, getPanelById } from '../../panels';
import { DeviceSettingsDialog } from '../dialogs/DeviceSettingsDialog';
import { PanelResizer } from './PanelResizer';

const LOWER_TABS = ['camera', 'macros', 'console'] as const;

/**
 * Run-mode right panel: machine chip, then two stacked cards — Laser
 * Control on top, Camera/Macros/Console tabs below.
 */
export function RunPanel() {
  const { t } = useTranslation();
  const activeProfile = useMachineStore(
    (s) => (s.profiles ?? []).find((p) => p.id === s.activeProfileId) ?? null,
  );
  const [lowerTab, setLowerTab] = useState<(typeof LOWER_TABS)[number]>('camera');
  const [upperRatio, setUpperRatio] = useState(0.58);
  const [showProfiles, setShowProfiles] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  const LaserContent = PANEL_COMPONENTS['laser'] ?? null;
  const LowerContent = PANEL_COMPONENTS[lowerTab] ?? null;
  const laserDef = getPanelById('laser');

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

      {/* Upper card: Laser Control */}
      <div
        className="flex min-h-0 flex-col overflow-hidden rounded-b-xl rounded-tr-xl border border-bb-border bg-bb-panel shadow-lg"
        style={{ flex: upperRatio }}
      >
        <div className="border-b border-bb-border px-3 py-1.5 text-xs font-semibold text-bb-text">
          {laserDef ? t(laserDef.titleKey) : 'Laser'}
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          {LaserContent && <LaserContent />}
        </div>
      </div>

      {/* Split handle */}
      <PanelResizer
        direction="bottom"
        onResize={(delta) => {
          const height = containerRef.current?.clientHeight ?? 0;
          if (height <= 0) return;
          // Drag up = positive delta = lower card grows (bottom-resizer semantics).
          setUpperRatio((r) => Math.min(0.85, Math.max(0.15, r - delta / height)));
        }}
      />

      {/* Lower card: Camera / Macros / Console */}
      <div
        className="flex min-h-0 flex-col overflow-hidden rounded-xl border border-bb-border bg-bb-panel shadow-lg"
        style={{ flex: 1 - upperRatio }}
      >
        <div className="flex border-b border-bb-border px-1 pt-1">
          {LOWER_TABS.map((id) => {
            const def = getPanelById(id);
            return (
              <button
                key={id}
                className={`flex-1 truncate border-b-2 px-1.5 pb-1.5 pt-0.5 text-xs ${
                  lowerTab === id
                    ? 'border-bb-accent font-semibold text-bb-accent'
                    : 'border-transparent text-bb-text-muted hover:text-bb-text'
                }`}
                onClick={() => setLowerTab(id)}
                data-testid={`run-lower-tab-${id}`}
              >
                {def ? t(def.titleKey) : id}
              </button>
            );
          })}
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          {LowerContent && <LowerContent />}
        </div>
      </div>

      {showProfiles && <DeviceSettingsDialog onClose={() => setShowProfiles(false)} />}
    </div>
  );
}
