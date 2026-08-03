import { useTranslation } from 'react-i18next';
import { useUiStore, type MeshDeformMode } from '../../stores/uiStore';
import { WarpIcon } from '../icons/WarpIcon';
import { ContextualToolSection } from './ContextualToolSection';

const MODES: Array<{
  mode: MeshDeformMode;
  labelKey: 'warp_tool.four_points' | 'warp_tool.sixteen_points';
  titleKey: 'menus.tools.warp_selection_4_points' | 'menus.tools.deform_selection_16_points';
}> = [
  { mode: 'warp', labelKey: 'warp_tool.four_points', titleKey: 'menus.tools.warp_selection_4_points' },
  { mode: 'mesh', labelKey: 'warp_tool.sixteen_points', titleKey: 'menus.tools.deform_selection_16_points' },
];

export function WarpPropertiesSection() {
  const { t } = useTranslation();
  const mode = useUiStore((state) => state.meshDeformMode);
  const setMode = useUiStore((state) => state.setMeshDeformMode);

  return (
    <ContextualToolSection title={t('warp_tool.title')} icon={<WarpIcon size={16} />} testId="warp-properties-section">
      <div className="grid grid-cols-2 gap-1.5" role="toolbar" aria-label={t('warp_tool.title')}>
        {MODES.map((option) => {
          const active = mode === option.mode;
          return (
            <button
              key={option.mode}
              type="button"
              aria-label={t(option.titleKey)}
              aria-pressed={active}
              onClick={() => setMode(option.mode)}
              className={`h-9 rounded-lg border px-2 text-[11px] font-medium outline-none transition-colors focus-visible:ring-1 focus-visible:ring-bb-accent ${
                active
                  ? 'border-bb-accent bg-bb-accent/15 text-bb-accent'
                  : 'border-bb-border bg-bb-surface text-bb-text-muted hover:border-bb-accent/40 hover:bg-bb-hover hover:text-bb-text'
              }`}
            >
              {t(option.labelKey)}
            </button>
          );
        })}
      </div>
      <div className="rounded-lg border border-bb-border bg-bb-panel px-2.5 py-2 text-[11px] leading-4 text-bb-text-muted">
        {t(mode === 'warp' ? 'status.tool_hint.warp_selection' : 'status.tool_hint.deform_selection')}
      </div>
    </ContextualToolSection>
  );
}
