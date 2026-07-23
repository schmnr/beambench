import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useUiStore } from '../../stores/uiStore';
import { useAppStore } from '../../stores/appStore';
import { appService } from '../../services/appService';
import { NumberInput } from '../shared/NumberInput';
import { Select } from '../shared/Select';
import { Toggle } from '../shared/Toggle';
import type { TextAlignment, TextAlignmentV, TextLayoutMode } from '../../types/project';
import { mmToDisplay, displayToMm, roundDisplayLength, lengthStep, lengthUnitLabel, labelWithUnit } from '../../utils/lengthUnits';

const TEXT_LAYOUT_STRAIGHT = 'straight' as const;
const TEXT_LAYOUT_BEND = 'bend' as const;
const TEXT_LAYOUT_PATH = 'path' as const;

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

const layoutModeOptions = [
  { value: TEXT_LAYOUT_STRAIGHT, labelKey: 'panels.text_properties.layout_straight' },
  { value: TEXT_LAYOUT_BEND, labelKey: 'panels.text_properties.layout_bend' },
  { value: TEXT_LAYOUT_PATH, labelKey: 'panels.text_properties.layout_path' },
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
  const unitLabel = lengthUnitLabel(displayUnit);
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
    <div className="flex flex-col gap-2.5 px-3 py-2">
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
        label={labelWithUnit(t('panels.text_properties.size_mm'), unitLabel)}
        value={roundDisplayLength(mmToDisplay(textDefaults.font_size_mm, displayUnit), displayUnit)}
        onChange={(v) => updateTextDefaults({ font_size_mm: displayToMm(v, displayUnit) })}
        step={lengthStep(displayUnit)}
        min={mmToDisplay(0.1, displayUnit)}
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
        disabled={textDefaults.layout_mode !== TEXT_LAYOUT_STRAIGHT}
      />
      <Select
        label={t('panels.text_properties.v_align')}
        value={textDefaults.alignment_v}
        options={verticalAlignmentOptions.map((option) => ({ value: option.value, label: t(option.labelKey) }))}
        onChange={(alignment_v) => updateTextDefaults({ alignment_v: alignment_v as TextAlignmentV })}
        disabled={textDefaults.layout_mode !== TEXT_LAYOUT_STRAIGHT}
      />
      <Select
        label={t('panels.text_properties.layout')}
        value={textDefaults.layout_mode}
        options={layoutModeOptions.map((option) => ({ value: option.value, label: t(option.labelKey) }))}
        onChange={(value) => {
          const layout_mode = value as TextLayoutMode;
          updateTextDefaults({
            layout_mode,
            on_path: layout_mode === TEXT_LAYOUT_PATH,
            ...(layout_mode === TEXT_LAYOUT_BEND && textDefaults.bend_radius === 0
              ? { bend_radius: 50 }
              : {}),
          });
        }}
      />
      <NumberInput
        label={labelWithUnit(t('panels.text_properties.h_space'), unitLabel)}
        value={roundDisplayLength(mmToDisplay(textDefaults.h_spacing, displayUnit), displayUnit)}
        onChange={(v) => updateTextDefaults({ h_spacing: displayToMm(v, displayUnit) })}
        step={lengthStep(displayUnit)}
      />
      <NumberInput
        label={labelWithUnit(t('panels.text_properties.v_space'), unitLabel)}
        value={roundDisplayLength(mmToDisplay(textDefaults.v_spacing, displayUnit), displayUnit)}
        onChange={(v) => updateTextDefaults({ v_spacing: displayToMm(v, displayUnit) })}
        step={lengthStep(displayUnit)}
        disabled={textDefaults.layout_mode !== TEXT_LAYOUT_STRAIGHT}
      />
      {textDefaults.layout_mode === TEXT_LAYOUT_PATH && (
        <NumberInput
          label={labelWithUnit(t('panels.text_properties.path_offset'), unitLabel)}
          value={roundDisplayLength(mmToDisplay(textDefaults.path_offset, displayUnit), displayUnit)}
          onChange={(value) => updateTextDefaults({ path_offset: displayToMm(value, displayUnit) })}
          step={lengthStep(displayUnit)}
        />
      )}
      {textDefaults.layout_mode === TEXT_LAYOUT_BEND && (
        <NumberInput
          label={labelWithUnit(t('panels.text_properties.bend_radius'), unitLabel)}
          value={roundDisplayLength(mmToDisplay(textDefaults.bend_radius, displayUnit), displayUnit)}
          onChange={(value) => updateTextDefaults({ bend_radius: displayToMm(value, displayUnit) })}
          min={mmToDisplay(0.1, displayUnit)}
          step={lengthStep(displayUnit, 5, 0.2)}
        />
      )}
      <Toggle
        label={t('panels.text_properties.weld')}
        checked={textDefaults.welded}
        onChange={(welded) => updateTextDefaults({ welded })}
      />
      <Toggle
        label={t('panels.text_properties.distort')}
        checked={textDefaults.distort}
        onChange={(distort) => updateTextDefaults({ distort })}
        disabled={textDefaults.layout_mode === TEXT_LAYOUT_STRAIGHT}
      />
    </div>
  );
}
