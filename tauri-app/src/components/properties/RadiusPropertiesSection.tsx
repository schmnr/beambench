import { useTranslation } from 'react-i18next';
import { useAppStore } from '../../stores/appStore';
import { useUiStore } from '../../stores/uiStore';
import { displayToMm, lengthStep, lengthUnitLabel, mmToDisplay, roundDisplayLength } from '../../utils/lengthUnits';
import { NumberInput } from '../shared/NumberInput';
import { ContextualToolSection } from './ContextualToolSection';

function RadiusGlyph() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="M4 3v11c0 4 2 6 6 6h11" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
      <path d="M4 12c0 5 3 8 8 8" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeDasharray="2 2" />
    </svg>
  );
}

export function RadiusPropertiesSection() {
  const { t } = useTranslation();
  const settings = useAppStore((state) => state.settings);
  const radiusToolValue = useUiStore((state) => state.radiusToolValue);
  const setRadiusToolValue = useUiStore((state) => state.setRadiusToolValue);
  const displayUnit = settings?.display_unit === 'inches' ? 'inches' : 'mm';
  const radiusMm = radiusToolValue ?? settings?.last_radius_mm ?? 5;

  return (
    <ContextualToolSection
      title={t('toolbars.modifiers.radius_tool')}
      icon={<RadiusGlyph />}
      testId="radius-properties-section"
    >
      <NumberInput
        label={t('toolbars.modifiers.radius_with_unit', { unit: lengthUnitLabel(displayUnit) })}
        value={roundDisplayLength(mmToDisplay(radiusMm, displayUnit), displayUnit)}
        onChange={(value) => setRadiusToolValue(displayToMm(value, displayUnit))}
        min={mmToDisplay(0.01, displayUnit)}
        step={lengthStep(displayUnit, 0.5, 0.01)}
      />
      <div className="rounded-lg border border-bb-border bg-bb-panel px-2.5 py-2 text-[11px] leading-4 text-bb-text-muted">
        {t('status.tool_hint.radius')}
      </div>
    </ContextualToolSection>
  );
}
