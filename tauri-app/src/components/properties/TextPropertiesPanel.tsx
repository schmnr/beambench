import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { useAppStore } from '../../stores/appStore';
import { appService } from '../../services/appService';
import { TextInput } from '../shared/TextInput';
import { NumberInput } from '../shared/NumberInput';
import { Select } from '../shared/Select';
import { Toggle } from '../shared/Toggle';
import type { ObjectData, TextAlignment, TextAlignmentV, TextLayoutMode } from '../../types/project';
import { applyTextLayoutMode, clearTextGuidePath } from './textLayoutMode';
import { useUiStore } from '../../stores/uiStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { mmToDisplay, displayToMm, roundDisplayLength, lengthStep, lengthUnitLabel, labelWithUnit } from '../../utils/lengthUnits';

interface TextPropertiesPanelProps {
  objectId: string;
  data: Extract<ObjectData, { type: 'text' }>;
}

const TEXT_LAYOUT_STRAIGHT = 'straight' as const;
const TEXT_LAYOUT_BEND = 'bend' as const;
const TEXT_LAYOUT_PATH = 'path' as const;
const TEXT_ALIGNMENT_TOP = 'top' as const;

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
  { value: TEXT_ALIGNMENT_TOP, labelKey: 'panels.text_properties.align_top' },
  { value: 'middle', labelKey: 'panels.text_properties.align_middle' },
  { value: 'bottom', labelKey: 'panels.text_properties.align_bottom' },
];

const layoutModeOptions = [
  { value: TEXT_LAYOUT_STRAIGHT, labelKey: 'panels.text_properties.layout_straight' },
  { value: TEXT_LAYOUT_BEND, labelKey: 'panels.text_properties.layout_bend' },
  { value: TEXT_LAYOUT_PATH, labelKey: 'panels.text_properties.layout_path' },
];

export function TextPropertiesPanel({ objectId, data }: TextPropertiesPanelProps) {
  const { t } = useTranslation();
  const updateObjectData = useProjectStore((s) => s.updateObjectData);
  const displayUnit = useAppStore((s) => s.settings?.display_unit) ?? 'mm';
  const [systemFonts, setSystemFonts] = useState<string[]>([]);
  const fontOptions = systemFonts.length > 0
    ? systemFonts.map((font) => ({ value: font, label: font }))
    : fontFamilyOptions.map((option) => ({ value: option.value, label: t(option.labelKey) }));
  const missingGlyphs = data.missing_glyphs ?? [];

  useEffect(() => {
    appService.getSystemFonts().then((fonts) => {
      if (fonts.length > 0) {
        setSystemFonts(fonts);
      }
    }).catch(() => {
      // Keep generic defaults when native font enumeration is unavailable.
    });
  }, []);

  // Effective layout mode: legacy on_path+straight maps to 'path'
  const effectiveMode = (data.on_path && (data.layout_mode ?? TEXT_LAYOUT_STRAIGHT) === TEXT_LAYOUT_STRAIGHT)
    ? TEXT_LAYOUT_PATH
    : (data.layout_mode ?? TEXT_LAYOUT_STRAIGHT);

  return (
    <div className="flex flex-col gap-1.5 pt-1 border-t border-bb-border">
      <div className="text-xs font-medium text-bb-accent uppercase tracking-wider">{t('panels.text_properties.title')}</div>
      <TextInput
        label={t('panels.text_properties.content')}
        value={data.content}
        onChange={(content) => updateObjectData(objectId, { ...data, content, variable_text: undefined })}
      />
      <Select
        label={t('panels.text_properties.font')}
        value={data.font_family}
        options={fontOptions}
        onChange={(font_family) => updateObjectData(objectId, { ...data, font_family })}
      />
      {(data.missing_font || missingGlyphs.length > 0) && (
        <div className="text-[11px] leading-4 text-bb-warning-fg">
          {data.missing_font
            ? t('toolbars.properties.font_missing', { font: data.font_family })
            : t('toolbars.properties.missing_glyphs', { glyphs: missingGlyphs.join(' ') })}
        </div>
      )}
      <NumberInput
        label={labelWithUnit(t('panels.text_properties.size_mm'), lengthUnitLabel(displayUnit))}
        value={roundDisplayLength(mmToDisplay(data.font_size_mm, displayUnit), displayUnit)}
        onChange={(v) => updateObjectData(objectId, { ...data, font_size_mm: displayToMm(v, displayUnit) })}
        min={mmToDisplay(0.5, displayUnit)}
        max={mmToDisplay(500, displayUnit)}
        step={lengthStep(displayUnit, 0.5, 0.02)}
      />
      <Select
        label={t('panels.text_properties.align')}
        value={data.alignment}
        options={alignmentOptions.map((option) => ({ value: option.value, label: t(option.labelKey) }))}
        onChange={(alignment) =>
          updateObjectData(objectId, { ...data, alignment: alignment as TextAlignment })
        }
      />
      <Select
        label={t('panels.text_properties.v_align')}
        value={data.alignment_v ?? TEXT_ALIGNMENT_TOP}
        options={verticalAlignmentOptions.map((option) => ({ value: option.value, label: t(option.labelKey) }))}
        onChange={(alignment_v) =>
          updateObjectData(objectId, { ...data, alignment_v: alignment_v as TextAlignmentV })
        }
      />
      <Select
        label={t('panels.text_properties.layout')}
        value={effectiveMode}
        options={layoutModeOptions.map((option) => ({ value: option.value, label: t(option.labelKey) }))}
        onChange={(layout_mode) =>
          void applyTextLayoutMode(objectId, data, layout_mode as TextLayoutMode, {
            bendRadiusFallback: 50,
          })
        }
      />
      {effectiveMode === TEXT_LAYOUT_PATH && (
        <div className="flex items-center gap-1.5 text-xs">
          {data.guide_path_id ? (
            <>
              <span className="text-bb-text-dim text-[10px]">{t('toolbars.properties.linked')}</span>
              <button
                className="px-1.5 py-0.5 text-[10px] bg-bb-bg border border-bb-border rounded text-bb-text hover:bg-bb-hover h-6"
                onClick={() => {
                  useUiStore.getState().setPendingGuidePathText(objectId);
                  useNotificationStore.getState().push(t('toolbars.properties.select_guide_path_hint'), 'info');
                }}
              >
                {t('toolbars.properties.pick')}
              </button>
              <button
                className="px-1.5 py-0.5 text-[10px] bg-bb-bg border border-bb-border rounded text-bb-text hover:bg-bb-hover h-6"
                onClick={() => void clearTextGuidePath(objectId)}
              >
                {t('toolbars.properties.clear')}
              </button>
            </>
          ) : (
            <>
              <span className="text-bb-warning text-[10px]">{t('toolbars.properties.no_path')}</span>
              <button
                className="px-1.5 py-0.5 text-[10px] bg-bb-bg border border-bb-border rounded text-bb-text hover:bg-bb-hover h-6"
                onClick={() => {
                  useUiStore.getState().setPendingGuidePathText(objectId);
                  useNotificationStore.getState().push(t('toolbars.properties.select_guide_path_hint'), 'info');
                }}
              >
                {t('toolbars.properties.select_path')}
              </button>
            </>
          )}
        </div>
      )}
      <div className="flex items-center gap-3">
        <Toggle
          label={t('panels.text_properties.bold')}
          checked={data.bold}
          onChange={(bold) => updateObjectData(objectId, { ...data, bold })}
        />
        <Toggle
          label={t('panels.text_properties.italic')}
          checked={data.italic}
          onChange={(italic) => updateObjectData(objectId, { ...data, italic })}
        />
      </div>
      <div className="flex items-center gap-3">
        <Toggle
          label={t('panels.text_properties.uppercase')}
          checked={data.upper_case ?? false}
          onChange={(upper_case) => updateObjectData(objectId, { ...data, upper_case })}
        />
        <Toggle
          label={t('panels.text_properties.weld')}
          checked={data.welded ?? false}
          onChange={(welded) => updateObjectData(objectId, { ...data, welded })}
        />
      </div>
      <NumberInput
        label={t('panels.text_properties.max_width')}
        value={data.max_width ?? 0}
        onChange={(max_width) => updateObjectData(objectId, { ...data, max_width: max_width > 0 ? max_width : null })}
        min={0}
        step={0.1}
      />
      <div className="flex items-center gap-3">
        <Toggle
          label={t('panels.text_properties.squeeze')}
          checked={data.squeeze ?? false}
          onChange={(squeeze) => updateObjectData(objectId, { ...data, squeeze })}
        />
        <Toggle
          label={t('panels.text_properties.distort')}
          checked={data.distort ?? false}
          onChange={(distort) => updateObjectData(objectId, { ...data, distort })}
        />
        <Toggle
          label={t('panels.text_properties.rtl')}
          checked={data.rtl ?? false}
          onChange={(rtl) => updateObjectData(objectId, { ...data, rtl })}
        />
      </div>
      <NumberInput
        label={t('panels.text_properties.h_space')}
        value={data.h_spacing ?? 0}
        onChange={(h_spacing) => updateObjectData(objectId, { ...data, h_spacing })}
        step={0.1}
      />
      <NumberInput
        label={t('panels.text_properties.v_space')}
        value={data.v_spacing ?? 0}
        onChange={(v_spacing) => updateObjectData(objectId, { ...data, v_spacing })}
        step={0.1}
      />
      {effectiveMode === TEXT_LAYOUT_PATH && (
        <NumberInput
          label={t('panels.text_properties.path_offset')}
          value={data.path_offset ?? 0}
          onChange={(path_offset) => updateObjectData(objectId, { ...data, path_offset })}
          step={0.1}
        />
      )}
      {effectiveMode === TEXT_LAYOUT_BEND && (
        <NumberInput
          label={t('panels.text_properties.bend_radius')}
          value={data.bend_radius ?? 0}
          onChange={(bend_radius) => updateObjectData(objectId, { ...data, bend_radius })}
          step={0.1}
        />
      )}
    </div>
  );
}
