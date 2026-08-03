import { useState } from 'react';
import { Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '../../stores/appStore';
import { useProjectStore } from '../../stores/projectStore';
import { defaultVectorSettings } from '../../types/cutEntryDefaults';
import { displayToMm, lengthStep, lengthUnitLabel, mmToDisplay, roundDisplayLength } from '../../utils/lengthUnits';
import { TabsIcon } from '../icons/TabsIcon';
import { NumberInput } from '../shared/NumberInput';
import { ContextualToolSection } from './ContextualToolSection';

export function TabPropertiesSection() {
  const { t } = useTranslation();
  const settings = useAppStore((state) => state.settings);
  const project = useProjectStore((state) => state.project);
  const selectedObjectIds = useProjectStore((state) => state.selectedObjectIds);
  const selectedLayerId = useProjectStore((state) => state.selectedLayerId);
  const updateCutEntry = useProjectStore((state) => state.updateCutEntry);
  const clearTabs = useProjectStore((state) => state.clearTabs);
  const [clearing, setClearing] = useState(false);

  const selectedObject = selectedObjectIds.length === 1
    ? project?.objects.find((object) => object.id === selectedObjectIds[0]) ?? null
    : null;
  const layerId = selectedObject?.layer_id ?? selectedLayerId;
  const layer = project?.layers.find((candidate) => candidate.id === layerId) ?? null;
  const vectorEntry = layer?.entries.find((entry) => (
    entry.operation === 'line'
    || entry.operation === 'cut'
    || entry.operation === 'score'
    || entry.operation === 'offset_fill'
  )) ?? null;
  const vectorSettings = vectorEntry?.vector_settings ?? defaultVectorSettings();
  const tabCount = selectedObject?.tabs?.length ?? 0;
  const displayUnit = settings?.display_unit === 'inches' ? 'inches' : 'mm';
  const widthMm = vectorSettings.tab_width_mm ?? 3;
  const clearLabel = t('dialog.hotkey_editor.clear');

  const updateWidth = (displayValue: number) => {
    if (!layer || !vectorEntry) return;
    const nextWidthMm = Math.max(0.1, displayToMm(displayValue, displayUnit));
    void updateCutEntry(layer.id, vectorEntry.id, {
      vector_settings: {
        ...vectorSettings,
        tab_width_mm: nextWidthMm,
      },
    });
  };

  const clearPlacedTabs = async () => {
    if (!selectedObject || tabCount === 0 || clearing) return;
    setClearing(true);
    try {
      await clearTabs(selectedObject.id);
    } finally {
      setClearing(false);
    }
  };

  return (
    <ContextualToolSection
      title={t('toolbars.creation.tabs')}
      icon={<TabsIcon size={16} />}
      testId="tab-properties-section"
    >
      <NumberInput
        label={`${t('panels.properties.width')} (${lengthUnitLabel(displayUnit)})`}
        value={roundDisplayLength(mmToDisplay(widthMm, displayUnit), displayUnit)}
        onChange={updateWidth}
        min={mmToDisplay(0.1, displayUnit)}
        max={mmToDisplay(100, displayUnit)}
        step={lengthStep(displayUnit, 0.5, 0.01)}
        disabled={!vectorEntry}
      />

      <div className="flex items-center justify-between gap-2">
        <span className="text-[11px] text-bb-text-muted">
          {selectedObject
            ? <>{t('toolbars.creation.tabs')}: {tabCount}</>
            : t('panels.properties.nothing_selected')}
        </span>
        <button
          type="button"
          aria-label={clearLabel}
          title={clearLabel}
          disabled={!selectedObject || tabCount === 0 || clearing}
          onClick={() => void clearPlacedTabs()}
          className="flex h-8 items-center gap-1.5 rounded-lg border border-bb-border bg-bb-surface px-2.5 text-[11px] font-medium text-bb-text outline-none transition-colors hover:border-bb-accent/40 hover:bg-bb-hover focus-visible:ring-1 focus-visible:ring-bb-accent disabled:cursor-default disabled:opacity-40"
        >
          <Trash2 size={14} />
          {clearLabel}
        </button>
      </div>

      <div className="rounded-lg border border-bb-border bg-bb-panel px-2.5 py-2 text-[11px] leading-4 text-bb-text-muted">
        {t('status.tool_hint.tabs')}
      </div>
    </ContextualToolSection>
  );
}
