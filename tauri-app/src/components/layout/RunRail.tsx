import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Move, Terminal, Play, X } from 'lucide-react';
import { IconButton } from '../shared/IconButton';
import { PANEL_COMPONENTS, getPanelById } from '../../panels';

const FLYOUTS = [
  { id: 'move', icon: Move },
  { id: 'console', icon: Terminal },
  { id: 'macros', icon: Play },
] as const;

type FlyoutId = (typeof FLYOUTS)[number]['id'];

/**
 * Run-mode machine rail: jog, console, and macros open as flyout cards
 * beside the rail (device settings lives on the machine chip). Mirrors the Design
 * rail's floating-card styling.
 */
export function RunRail() {
  const { t } = useTranslation();
  const [openFlyout, setOpenFlyout] = useState<FlyoutId | null>(null);

  const FlyoutContent = openFlyout ? (PANEL_COMPONENTS[openFlyout] ?? null) : null;
  const flyoutDef = openFlyout ? getPanelById(openFlyout) : undefined;

  return (
    <>
      <div className="my-2 ml-2 flex flex-col items-center gap-0.5 self-start rounded-xl border border-bb-border bg-bb-panel px-1 py-1.5 shadow-lg">
        {FLYOUTS.map(({ id, icon: Icon }) => {
          const def = getPanelById(id);
          return (
            <IconButton
              key={id}
              icon={<Icon size={24} />}
              label={def ? t(def.titleKey) : id}
              onClick={() => setOpenFlyout((cur) => (cur === id ? null : id))}
              active={openFlyout === id}
              size="sm"
            />
          );
        })}
      </div>

      {openFlyout && FlyoutContent && (
        <div
          className="absolute bottom-2 left-[4.25rem] top-2 z-[35] flex w-80 flex-col overflow-hidden rounded-xl border border-bb-border bg-bb-panel shadow-xl"
          data-testid="run-rail-flyout"
        >
          <div className="flex items-center justify-between border-b border-bb-border px-3 py-1.5">
            <span className="text-xs font-semibold text-bb-text">
              {flyoutDef ? t(flyoutDef.titleKey) : openFlyout}
            </span>
            <button
              className="text-bb-text-dim hover:text-bb-text"
              onClick={() => setOpenFlyout(null)}
              aria-label={t('common.close')}
            >
              <X size={13} />
            </button>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto">
            <FlyoutContent />
          </div>
        </div>
      )}
    </>
  );
}
