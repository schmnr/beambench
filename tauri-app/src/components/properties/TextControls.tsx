import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import {
  AlignCenter,
  AlignLeft,
  AlignRight,
  AlignVerticalJustifyCenter,
  AlignVerticalJustifyEnd,
  AlignVerticalJustifyStart,
  ChevronRight,
} from 'lucide-react';
import { appService } from '../../services/appService';
import { useAppStore } from '../../stores/appStore';
import type {
  TextAlignment,
  TextAlignmentV,
  TextCirclePlacement,
  TextLayoutMode,
  TextTransformStyle,
} from '../../types/project';
import {
  displayToMm,
  labelWithUnit,
  lengthStep,
  lengthUnitLabel,
  mmToDisplay,
  roundDisplayLength,
} from '../../utils/lengthUnits';
import { NumberInput } from '../shared/NumberInput';
import { Select } from '../shared/Select';
import { Toggle } from '../shared/Toggle';
import { INSPECTOR_SECTION_HEADER_CLASS } from '../shared/panelAppearance';
import { FontPicker } from './FontPicker';
import {
  activeTextShape,
  circleCurveToDegrees,
  circleDegreesToCurve,
  circlePlacementFromParts,
  circlePlacementParts,
  DEFAULT_BEND_RADIUS_MM,
  DEFAULT_TEXT_SHAPE_STRENGTH,
  signedTextShapeValue,
  textShapeSupportsDistort,
  textShapeUsesEnvelopeStrength,
  type TextShapeMode,
} from './textShapeMode';

const CONTROL_WIDTH_CLASS = 'w-40';
const ALIGN_LEFT: TextAlignment = 'left';
const ALIGN_CENTER: TextAlignment = 'center';
const ALIGN_RIGHT: TextAlignment = 'right';
const ALIGN_TOP: TextAlignmentV = 'top';
const ALIGN_MIDDLE: TextAlignmentV = 'middle';
const ALIGN_BOTTOM: TextAlignmentV = 'bottom';
const CIRCLE_TOP = 'top' as const;
const CIRCLE_BOTTOM = 'bottom' as const;
const CIRCLE_OUTSIDE = 'outside' as const;
const CIRCLE_INSIDE = 'inside' as const;

const fallbackFonts = ['Arial', 'Helvetica', 'sans-serif', 'serif', 'monospace'];

export interface TextControlValue {
  font_family: string;
  font_size_mm: number;
  alignment: TextAlignment;
  alignment_v: TextAlignmentV;
  bold: boolean;
  italic: boolean;
  upper_case: boolean;
  welded: boolean;
  h_spacing: number;
  v_spacing: number;
  layout_mode: TextLayoutMode;
  on_path: boolean;
  path_offset: number;
  distort: boolean;
  rtl: boolean;
  bend_radius: number;
  transform_style: TextTransformStyle;
  transform_curve: number;
  circle_placement: TextCirclePlacement;
  max_width: number | null;
  squeeze: boolean;
}

interface TextControlsProps {
  value: TextControlValue;
  onPatch: (patch: Partial<TextControlValue>) => void;
  onShapeChange: (shape: TextShapeMode) => void;
  pathControls?: ReactNode;
  creationMode?: boolean;
}

function useSystemFonts(currentFont: string): string[] {
  const [fonts, setFonts] = useState<string[]>([]);
  useEffect(() => {
    let mounted = true;
    void appService.getSystemFonts().then((loaded) => {
      if (mounted) setFonts(loaded);
    }).catch(() => {
      // Generic fallback families remain available if enumeration fails.
    });
    return () => { mounted = false; };
  }, []);
  return useMemo(() => Array.from(new Set([currentFont, ...(fonts.length ? fonts : fallbackFonts)])), [currentFont, fonts]);
}

function InspectorGroup({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2 border-t border-bb-border pt-2">
      <div className={INSPECTOR_SECTION_HEADER_CLASS}>{title}</div>
      {children}
    </section>
  );
}

function SegmentedControl({ children, label }: { children: ReactNode; label: string }) {
  return (
    <div role="group" aria-label={label} className="flex h-7 overflow-hidden rounded border border-bb-control-border bg-bb-input">
      {children}
    </div>
  );
}

function SegmentButton({
  active,
  label,
  children,
  onClick,
  grow = true,
}: {
  active: boolean;
  label: string;
  children: ReactNode;
  onClick: () => void;
  grow?: boolean;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={active}
      title={label}
      onClick={onClick}
      className={`${grow ? 'flex-1' : 'w-8'} flex items-center justify-center border-r border-bb-border px-1.5 text-[11px] last:border-r-0 ${
        active ? 'bg-bb-accent/15 text-bb-accent' : 'text-bb-text-muted hover:bg-bb-hover hover:text-bb-text'
      }`}
    >
      {children}
    </button>
  );
}

export function TextControls({ value, onPatch, onShapeChange, pathControls, creationMode = false }: TextControlsProps) {
  const { t } = useTranslation();
  const displayUnit = useAppStore((state) => state.settings?.display_unit) ?? 'mm';
  const unitLabel = lengthUnitLabel(displayUnit);
  const systemFonts = useSystemFonts(value.font_family);
  const shape = activeTextShape(value);
  const fixedWidth = (value.max_width ?? 0) > 0;
  const advancedActive = shape !== 'straight';
  const [advancedOpen, setAdvancedOpen] = useState(advancedActive);

  useEffect(() => {
    if (advancedActive) setAdvancedOpen(true);
  }, [advancedActive]);

  const shapeOptions = [
    ['straight', t('panels.text_properties.layout_straight')],
    ['bend', t('panels.text_properties.layout_bend')],
    ['path', t('panels.text_properties.layout_path')],
    ['arch', t('panels.text_properties.transform_arch')],
    ['rise', t('panels.text_properties.transform_rise')],
    ['wave', t('panels.text_properties.transform_wave')],
    ['flag', t('panels.text_properties.transform_flag')],
    ['angle', t('panels.text_properties.transform_angle')],
    ['circle', t('panels.text_properties.transform_circle')],
  ].map(([optionValue, label]) => ({ value: optionValue, label }));
  const directionValue = shape === 'bend' ? value.bend_radius : value.transform_curve;
  const reverseDirection = directionValue < 0;
  const circlePlacement = circlePlacementParts(value.circle_placement);
  const supportsDistort = textShapeSupportsDistort(shape);
  const usesEnvelopeStrength = textShapeUsesEnvelopeStrength(shape);

  const textTypeControls = (
    <InspectorGroup title={t('panels.text_properties.text_box')}>
      <div className="flex items-center justify-between gap-2 text-xs">
        <span className="text-bb-text-muted">{t('panels.text_properties.width_mode')}</span>
        <div className="w-40">
          <SegmentedControl label={t('panels.text_properties.width_mode')}>
            <SegmentButton active={!fixedWidth} label={t('panels.text_properties.auto_width')} onClick={() => onPatch({ max_width: null, squeeze: false })}>{t('panels.text_properties.auto_width')}</SegmentButton>
            <SegmentButton active={fixedWidth} label={t('panels.text_properties.fixed_width')} onClick={() => onPatch({ max_width: value.max_width && value.max_width > 0 ? value.max_width : Math.max(40, value.font_size_mm * 6) })}>{t('panels.text_properties.fixed_width')}</SegmentButton>
          </SegmentedControl>
        </div>
      </div>
      {fixedWidth ? (
        <>
          <NumberInput
            label={labelWithUnit(t('panels.text_properties.width'), unitLabel)}
            value={roundDisplayLength(mmToDisplay(value.max_width!, displayUnit), displayUnit)}
            onChange={(width) => onPatch({ max_width: Math.max(0.1, displayToMm(width, displayUnit)) })}
            min={mmToDisplay(0.1, displayUnit)}
            step={lengthStep(displayUnit)}
            inputWidthClassName={CONTROL_WIDTH_CLASS}
          />
          <Toggle label={t('panels.text_properties.squeeze_to_fit')} checked={value.squeeze} onChange={(squeeze) => onPatch({ squeeze })} />
        </>
      ) : null}
    </InspectorGroup>
  );

  return (
    <div className="flex flex-col gap-2">
      {creationMode ? textTypeControls : null}
      <InspectorGroup title={t('panels.text_properties.typography')}>
        <FontPicker
          label={t('panels.text_properties.font')}
          value={value.font_family}
          fonts={systemFonts}
          onChange={(font_family) => onPatch({ font_family })}
          searchLabel={t('panels.text_properties.search_fonts')}
          recentLabel={t('panels.text_properties.recent_fonts')}
          favoritesLabel={t('panels.text_properties.favorite_fonts')}
          allFontsLabel={t('panels.text_properties.all_fonts')}
          noFontsFoundLabel={t('panels.text_properties.no_fonts_found')}
          favoriteFontLabel={t('panels.text_properties.favorite_font')}
          unfavoriteFontLabel={t('panels.text_properties.unfavorite_font')}
        />
        <NumberInput
          label={labelWithUnit(t('panels.text_properties.size_mm'), unitLabel)}
          value={roundDisplayLength(mmToDisplay(value.font_size_mm, displayUnit), displayUnit)}
          onChange={(fontSize) => onPatch({ font_size_mm: displayToMm(fontSize, displayUnit) })}
          min={mmToDisplay(0.1, displayUnit)}
          max={mmToDisplay(500, displayUnit)}
          step={lengthStep(displayUnit, 0.5, 0.02)}
          inputWidthClassName={CONTROL_WIDTH_CLASS}
        />
        <div className="flex items-center justify-between gap-2 text-xs">
          <span className="text-bb-text-muted">{t('panels.text_properties.style')}</span>
          <div className="w-40">
            <SegmentedControl label={t('panels.text_properties.style')}>
              <SegmentButton active={!value.bold && !value.italic} label={t('panels.text_properties.style_regular')} onClick={() => onPatch({ bold: false, italic: false })}>R</SegmentButton>
              <SegmentButton active={value.bold && !value.italic} label={t('panels.text_properties.bold')} onClick={() => onPatch({ bold: true, italic: false })}><strong>B</strong></SegmentButton>
              <SegmentButton active={!value.bold && value.italic} label={t('panels.text_properties.italic')} onClick={() => onPatch({ bold: false, italic: true })}><em>I</em></SegmentButton>
              <SegmentButton active={value.bold && value.italic} label={t('panels.text_properties.style_bold_italic')} onClick={() => onPatch({ bold: true, italic: true })}><strong><em>BI</em></strong></SegmentButton>
            </SegmentedControl>
          </div>
        </div>
        <Toggle
          label={t('panels.text_properties.uppercase')}
          checked={value.upper_case}
          onChange={(upper_case) => onPatch({ upper_case })}
        />
      </InspectorGroup>

      <InspectorGroup title={t('panels.text_properties.alignment_spacing')}>
        {shape === 'straight' ? (
          <>
            <div className="flex items-center justify-between gap-2 text-xs">
              <span className="text-bb-text-muted">{t('panels.text_properties.align')}</span>
              <div className="w-40">
                <SegmentedControl label={t('panels.text_properties.align')}>
                  <SegmentButton active={value.alignment === ALIGN_LEFT} label={t('panels.text_properties.align_left')} onClick={() => onPatch({ alignment: ALIGN_LEFT })}><AlignLeft size={14} /></SegmentButton>
                  <SegmentButton active={value.alignment === ALIGN_CENTER} label={t('panels.text_properties.align_center')} onClick={() => onPatch({ alignment: ALIGN_CENTER })}><AlignCenter size={14} /></SegmentButton>
                  <SegmentButton active={value.alignment === ALIGN_RIGHT} label={t('panels.text_properties.align_right')} onClick={() => onPatch({ alignment: ALIGN_RIGHT })}><AlignRight size={14} /></SegmentButton>
                </SegmentedControl>
              </div>
            </div>
            <div className="flex items-center justify-between gap-2 text-xs">
              <span className="text-bb-text-muted">{t('panels.text_properties.v_align')}</span>
              <div className="w-40">
                <SegmentedControl label={t('panels.text_properties.v_align')}>
                  <SegmentButton active={value.alignment_v === ALIGN_TOP} label={t('panels.text_properties.align_top')} onClick={() => onPatch({ alignment_v: ALIGN_TOP })}><AlignVerticalJustifyStart size={14} /></SegmentButton>
                  <SegmentButton active={value.alignment_v === ALIGN_MIDDLE} label={t('panels.text_properties.align_middle')} onClick={() => onPatch({ alignment_v: ALIGN_MIDDLE })}><AlignVerticalJustifyCenter size={14} /></SegmentButton>
                  <SegmentButton active={value.alignment_v === ALIGN_BOTTOM} label={t('panels.text_properties.align_bottom')} onClick={() => onPatch({ alignment_v: ALIGN_BOTTOM })}><AlignVerticalJustifyEnd size={14} /></SegmentButton>
                </SegmentedControl>
              </div>
            </div>
          </>
        ) : null}
        <NumberInput
          label={labelWithUnit(t('panels.text_properties.tracking'), unitLabel)}
          value={roundDisplayLength(mmToDisplay(value.h_spacing, displayUnit), displayUnit)}
          onChange={(tracking) => onPatch({ h_spacing: displayToMm(tracking, displayUnit) })}
          step={lengthStep(displayUnit)}
          inputWidthClassName={CONTROL_WIDTH_CLASS}
        />
        <NumberInput
          label={labelWithUnit(t('panels.text_properties.line_spacing'), unitLabel)}
          value={roundDisplayLength(mmToDisplay(value.v_spacing, displayUnit), displayUnit)}
          onChange={(lineSpacing) => onPatch({ v_spacing: displayToMm(lineSpacing, displayUnit) })}
          step={lengthStep(displayUnit)}
          inputWidthClassName={CONTROL_WIDTH_CLASS}
          disabled={shape === 'bend' || shape === 'path' || shape === 'arch' || shape === 'circle'}
        />
        <Toggle label={t('panels.text_properties.rtl')} checked={value.rtl} onChange={(rtl) => onPatch({ rtl })} />
      </InspectorGroup>

      {!creationMode ? textTypeControls : null}

      <details
        className="group border-t border-bb-border pt-1"
        open={advancedOpen}
        onToggle={(event) => setAdvancedOpen(event.currentTarget.open)}
      >
        <summary className="flex cursor-pointer list-none items-center gap-1 py-1 text-[10px] font-semibold uppercase tracking-wider text-bb-text-dim hover:text-bb-text">
          <ChevronRight size={12} className="transition-transform duration-150 group-open:rotate-90" />
          {t('panels.text_properties.advanced_shape')}
        </summary>
        <div className="flex flex-col gap-2 pb-1 pt-1">
          <Select
            label={t('panels.properties.shape')}
            value={shape}
            options={shapeOptions}
            onChange={(nextShape) => onShapeChange(nextShape as TextShapeMode)}
            selectClassName="w-40"
          />
          {shape === 'path' ? pathControls : null}
          {shape === 'path' || shape === 'bend' ? (
            <NumberInput
              label={labelWithUnit(t('panels.text_properties.baseline_offset'), unitLabel)}
              value={roundDisplayLength(mmToDisplay(value.path_offset, displayUnit), displayUnit)}
              onChange={(pathOffset) => onPatch({ path_offset: displayToMm(pathOffset, displayUnit) })}
              step={lengthStep(displayUnit)}
              inputWidthClassName={CONTROL_WIDTH_CLASS}
            />
          ) : null}
          {shape === 'bend' ? (
            <NumberInput
              label={labelWithUnit(t('panels.text_properties.bend_radius'), unitLabel)}
              value={roundDisplayLength(mmToDisplay(Math.abs(value.bend_radius), displayUnit), displayUnit)}
              onChange={(bendRadius) => onPatch({
                bend_radius: signedTextShapeValue(
                  displayToMm(bendRadius, displayUnit),
                  reverseDirection,
                  DEFAULT_BEND_RADIUS_MM,
                ),
              })}
              min={mmToDisplay(0.1, displayUnit)}
              step={lengthStep(displayUnit, 5, 0.2)}
              inputWidthClassName={CONTROL_WIDTH_CLASS}
            />
          ) : null}
          {(shape === 'bend' || usesEnvelopeStrength) ? (
            <div className="flex items-center justify-between gap-2 text-xs">
              <span className="text-bb-text-muted">{t('dialog.offset.direction')}</span>
              <div className="w-40">
                <SegmentedControl label={t('dialog.offset.direction')}>
                  <SegmentButton
                    active={!reverseDirection}
                    label={t('panels.text_properties.direction_normal')}
                    onClick={() => onPatch(shape === 'bend'
                      ? { bend_radius: signedTextShapeValue(value.bend_radius, false, DEFAULT_BEND_RADIUS_MM) }
                      : { transform_curve: signedTextShapeValue(value.transform_curve, false, DEFAULT_TEXT_SHAPE_STRENGTH) })}
                  >
                    {t('panels.text_properties.direction_normal')}
                  </SegmentButton>
                  <SegmentButton
                    active={reverseDirection}
                    label={t('panels.text_properties.direction_reverse')}
                    onClick={() => onPatch(shape === 'bend'
                      ? { bend_radius: signedTextShapeValue(value.bend_radius, true, DEFAULT_BEND_RADIUS_MM) }
                      : { transform_curve: signedTextShapeValue(value.transform_curve, true, DEFAULT_TEXT_SHAPE_STRENGTH) })}
                  >
                    {t('panels.text_properties.direction_reverse')}
                  </SegmentButton>
                </SegmentedControl>
              </div>
            </div>
          ) : null}
          {usesEnvelopeStrength ? (
            <NumberInput
              label={t('panels.text_properties.strength')}
              value={Math.abs(value.transform_curve)}
              onChange={(strength) => onPatch({
                transform_curve: signedTextShapeValue(strength, reverseDirection, DEFAULT_TEXT_SHAPE_STRENGTH),
              })}
              min={1}
              max={100}
              step={1}
              inputWidthClassName={CONTROL_WIDTH_CLASS}
            />
          ) : null}
          {shape === 'circle' ? (
            <>
              <NumberInput
                label={t('panels.text_properties.arc_span')}
                value={circleCurveToDegrees(value.transform_curve)}
                onChange={(degrees) => onPatch({ transform_curve: circleDegreesToCurve(degrees) })}
                min={45}
                max={315}
                step={5}
                inputWidthClassName={CONTROL_WIDTH_CLASS}
              />
              <div className="flex items-center justify-between gap-2 text-xs">
                <span className="text-bb-text-muted">{t('panels.text_properties.circle_position')}</span>
                <div className="w-40">
                  <SegmentedControl label={t('panels.text_properties.circle_position')}>
                    <SegmentButton active={circlePlacement.vertical === CIRCLE_TOP} label={t('panels.text_properties.circle_top')} onClick={() => onPatch({ circle_placement: circlePlacementFromParts(CIRCLE_TOP, circlePlacement.side) })}>{t('panels.text_properties.circle_top')}</SegmentButton>
                    <SegmentButton active={circlePlacement.vertical === CIRCLE_BOTTOM} label={t('panels.text_properties.circle_bottom')} onClick={() => onPatch({ circle_placement: circlePlacementFromParts(CIRCLE_BOTTOM, circlePlacement.side) })}>{t('panels.text_properties.circle_bottom')}</SegmentButton>
                  </SegmentedControl>
                </div>
              </div>
              <div className="flex items-center justify-between gap-2 text-xs">
                <span className="text-bb-text-muted">{t('panels.text_properties.circle_side')}</span>
                <div className="w-40">
                  <SegmentedControl label={t('panels.text_properties.circle_side')}>
                    <SegmentButton active={circlePlacement.side === CIRCLE_OUTSIDE} label={t('panels.text_properties.circle_outside')} onClick={() => onPatch({ circle_placement: circlePlacementFromParts(circlePlacement.vertical, CIRCLE_OUTSIDE) })}>{t('panels.text_properties.circle_outside')}</SegmentButton>
                    <SegmentButton active={circlePlacement.side === CIRCLE_INSIDE} label={t('panels.text_properties.circle_inside')} onClick={() => onPatch({ circle_placement: circlePlacementFromParts(circlePlacement.vertical, CIRCLE_INSIDE) })}>{t('panels.text_properties.circle_inside')}</SegmentButton>
                  </SegmentedControl>
                </div>
              </div>
            </>
          ) : null}
          {supportsDistort ? (
            <Toggle
              label={t('panels.text_properties.distort')}
              checked={value.distort}
              onChange={(distort) => onPatch({ distort })}
            />
          ) : null}
        </div>
      </details>

      <InspectorGroup title={t('panels.text_properties.laser_output')}>
        <Toggle label={t('panels.text_properties.weld')} checked={value.welded} onChange={(welded) => onPatch({ welded })} />
        <div className="text-[10px] leading-4 text-bb-text-dim">{t('panels.text_properties.convert_hint')}</div>
      </InspectorGroup>
    </div>
  );
}
