import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useUiStore } from '../../stores/uiStore';
import { useAppStore } from '../../stores/appStore';
import { appService } from '../../services/appService';
import { NumberInput } from '../shared/NumberInput';
import { Select } from '../shared/Select';
import { Toggle } from '../shared/Toggle';
import type { TextAlignment, TextAlignmentV } from '../../types/project';
import { mmToDisplay, displayToMm, roundDisplayLength, lengthStep, labelWithUnit } from '../../utils/lengthUnits';

const fontFamilyOptions = [
  { value: 'sans-serif', labelKey: 'panels.text_properties.font_sans_serif' },
  { value: 'serif', labelKey: 'panels.text_properties.font_serif' },
  { value: 'monospace', labelKey: 'panels.text_properties.font_monospace' },
];

const alignmentOptions = [
  { value: 'left', labelKey: 'panels.text_properties.align_left' },
  { value: 'center', labelKey: 'panels.text_properties.align_center' },
  { value: 'right', labelKey: 'panels.text_properties.align_right' },
];

const verticalAlignmentOptions = [
  { value: 'top', labelKey: 'panels.text_properties.align_top' },
  { value: 'middle', labelKey: 'panels.text_properties.align_middle' },
  { value: 'bottom', labelKey: 'panels.text_properties.align_bottom' },
];

/**
 * Editable defaults for the next text object, shown in the Properties panel
 * while the text tool is active with nothing selected. Replaces the old
 * properties-toolbar flow for pre-configuring text before clicking the canvas.
 */
export function TextDefaultsSection() {
  const { t } = useTranslation();
  const textDefaults = useUiStore((s) => s.textDefaults);
  const updateTextDefaults = useUiStore((s) => s.updateTextDefaults);
  const displayUnit = useAppStore((s) => s.settings?.display_unit) ?? 'mm';
  const [systemFonts, setSystemFonts] = useState<string[]>([]);
  const fontOptions = systemFonts.length > 0
    ? systemFonts.map((font) => ({ value: font, label: font }))
    : fontFamilyOptions.map((option) => ({ value: option.value, label: t(option.labelKey) }));

  useEffect(() => {
    appService.getSystemFonts().then((fonts) => {
      if (fonts.length > 0) setSystemFonts(fonts);
    }).catch(() => {
      // Keep generic defaults when native font enumeration is unavailable.
    });
  }, []);

  return (
    <div className="flex flex-col gap-2 px-2">
      <div className="text-[10px] font-semibold tracking-wider text-bb-text-muted uppercase pt-2">
        {t('panels.text_properties.title')}
      </div>
      <Select
        label={t('panels.text_properties.font')}
        value={textDefaults.font_family}
        options={fontOptions}
        onChange={(font_family) => updateTextDefaults({ font_family })}
      />
      <NumberInput
        label={labelWithUnit(t('panels.text_properties.size_mm'), displayUnit)}
        value={roundDisplayLength(mmToDisplay(textDefaults.font_size_mm, displayUnit), displayUnit)}
        onChange={(v) => updateTextDefaults({ font_size_mm: displayToMm(v, displayUnit) })}
        step={lengthStep(displayUnit)}
        min={displayToMm(0.1, 'mm')}
      />
      <div className="flex items-center gap-3">
        <Toggle
          label={t('panels.text_properties.bold')}
          checked={textDefaults.bold}
          onChange={(bold) => updateTextDefaults({ bold })}
        />
        <Toggle
          label={t('panels.text_properties.italic')}
          checked={textDefaults.italic}
          onChange={(italic) => updateTextDefaults({ italic })}
        />
        <Toggle
          label={t('panels.text_properties.uppercase')}
          checked={textDefaults.upper_case}
          onChange={(upper_case) => updateTextDefaults({ upper_case })}
        />
      </div>
      <Select
        label={t('panels.text_properties.align')}
        value={textDefaults.alignment}
        options={alignmentOptions.map((option) => ({ value: option.value, label: t(option.labelKey) }))}
        onChange={(alignment) => updateTextDefaults({ alignment: alignment as TextAlignment })}
      />
      <Select
        label={t('panels.text_properties.v_align')}
        value={textDefaults.alignment_v}
        options={verticalAlignmentOptions.map((option) => ({ value: option.value, label: t(option.labelKey) }))}
        onChange={(alignment_v) => updateTextDefaults({ alignment_v: alignment_v as TextAlignmentV })}
      />
      <NumberInput
        label={labelWithUnit(t('panels.text_properties.h_space'), displayUnit)}
        value={roundDisplayLength(mmToDisplay(textDefaults.h_spacing, displayUnit), displayUnit)}
        onChange={(v) => updateTextDefaults({ h_spacing: displayToMm(v, displayUnit) })}
        step={lengthStep(displayUnit)}
      />
      <NumberInput
        label={labelWithUnit(t('panels.text_properties.v_space'), displayUnit)}
        value={roundDisplayLength(mmToDisplay(textDefaults.v_spacing, displayUnit), displayUnit)}
        onChange={(v) => updateTextDefaults({ v_spacing: displayToMm(v, displayUnit) })}
        step={lengthStep(displayUnit)}
      />
      <Toggle
        label={t('panels.text_properties.weld')}
        checked={textDefaults.welded}
        onChange={(welded) => updateTextDefaults({ welded })}
      />
    </div>
  );
}
