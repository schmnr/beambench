import { useEffect, useState, useSyncExternalStore } from 'react';
import { useTranslation } from 'react-i18next';
import { Type } from 'lucide-react';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { useNotificationStore } from '../../stores/notificationStore';
import type { ObjectData, TextLayoutMode } from '../../types/project';
import { TextControls, type TextControlValue } from './TextControls';
import { applyTextLayoutMode, clearTextGuidePath } from './textLayoutMode';
import {
  getPendingContentForObject,
  subscribePendingTextEdit,
} from '../../canvas/textEditSession';
import {
  INSPECTOR_TITLE_BAR_CLASS,
  INSPECTOR_TITLE_BAR_ICON_CLASS,
  INSPECTOR_TITLE_BAR_LABEL_CLASS,
} from '../shared/panelAppearance';

interface TextPropertiesPanelProps {
  objectId: string;
  data: Extract<ObjectData, { type: 'text' }>;
}

function TextContentEditor({
  value,
  label,
  onCommit,
}: {
  value: string;
  label: string;
  onCommit: (content: string) => void;
}) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);

  const commit = () => {
    if (draft !== value) onCommit(draft);
  };

  return (
    <label className="flex flex-col gap-1 text-xs">
      <span className="text-bb-text-muted">{label}</span>
      <textarea
        value={draft}
        rows={2}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
            event.preventDefault();
            commit();
            event.currentTarget.blur();
          }
        }}
        className="min-h-14 w-full resize-y rounded border border-bb-control-border bg-bb-input px-2 py-1.5 text-xs leading-4 text-bb-text focus:border-bb-accent focus:outline-none"
      />
    </label>
  );
}

export function TextPropertiesPanel({ objectId, data }: TextPropertiesPanelProps) {
  const { t } = useTranslation();
  const updateObjectData = useProjectStore((state) => state.updateObjectData);
  const pendingContent = useSyncExternalStore(
    subscribePendingTextEdit,
    () => getPendingContentForObject(objectId),
    () => null,
  );
  const missingGlyphs = data.missing_glyphs ?? [];
  const controlValue: TextControlValue = {
    font_family: data.font_family,
    font_size_mm: data.font_size_mm,
    alignment: data.alignment,
    alignment_v: data.alignment_v ?? 'top',
    bold: data.bold,
    italic: data.italic,
    upper_case: data.upper_case ?? false,
    welded: data.welded ?? false,
    h_spacing: data.h_spacing ?? 0,
    v_spacing: data.v_spacing ?? 0,
    layout_mode: data.layout_mode ?? 'straight',
    on_path: data.on_path ?? false,
    path_offset: data.path_offset ?? 0,
    distort: data.distort ?? false,
    rtl: data.rtl ?? false,
    bend_radius: data.bend_radius ?? 0,
    transform_style: data.transform_style ?? 'none',
    transform_curve: data.transform_curve ?? 0,
    circle_placement: data.circle_placement ?? 'top_outside',
    max_width: data.max_width ?? null,
    squeeze: data.squeeze ?? false,
  };

  const patchData = (patch: Partial<TextControlValue>) => {
    void updateObjectData(objectId, { ...data, ...patch });
  };

  const effectiveMode = data.on_path && (data.layout_mode ?? 'straight') === 'straight'
    ? 'path'
    : data.layout_mode ?? 'straight';

  const pathControls = effectiveMode === 'path' ? (
    <div className="flex items-center justify-between gap-2 rounded bg-bb-bg px-2 py-1.5 text-xs">
      <span className={data.guide_path_id ? 'text-bb-text-muted' : 'text-bb-warning-fg'}>
        {data.guide_path_id ? t('toolbars.properties.linked') : t('toolbars.properties.no_path')}
      </span>
      <div className="flex gap-1">
        <button
          type="button"
          className="h-6 rounded border border-bb-control-border bg-bb-input px-2 text-[10px] text-bb-text hover:bg-bb-hover"
          onClick={() => {
            useUiStore.getState().setPendingGuidePathText(objectId);
            useNotificationStore.getState().push(t('toolbars.properties.select_guide_path_hint'), 'info');
          }}
        >
          {data.guide_path_id ? t('toolbars.properties.pick') : t('toolbars.properties.select_path')}
        </button>
        {data.guide_path_id ? (
          <button
            type="button"
            className="h-6 rounded border border-bb-control-border bg-bb-input px-2 text-[10px] text-bb-text hover:bg-bb-hover"
            onClick={() => void clearTextGuidePath(objectId)}
          >
            {t('toolbars.properties.clear')}
          </button>
        ) : null}
      </div>
    </div>
  ) : null;

  return (
    <section
      className="overflow-hidden rounded-lg border border-bb-border bg-bb-bg/40"
      data-testid="text-properties-panel"
    >
      <div
        className={INSPECTOR_TITLE_BAR_CLASS}
        data-testid="text-properties-header"
      >
        <Type size={14} className={INSPECTOR_TITLE_BAR_ICON_CLASS} />
        <div className={INSPECTOR_TITLE_BAR_LABEL_CLASS}>
          {t('panels.text_properties.title')}
        </div>
        <button
          type="button"
          onClick={() => useUiStore.getState().beginTextEditSession(objectId, 'double-click')}
          className="ml-auto flex h-6 items-center gap-1 rounded border border-bb-control-border bg-bb-input px-2 text-[10px] text-bb-text hover:bg-bb-hover"
        >
          <Type size={12} />
          {t('panels.text_properties.edit_on_canvas')}
        </button>
      </div>

      <div className="flex flex-col gap-2.5 p-3">
        <TextContentEditor
          label={t('panels.text_properties.content')}
          value={pendingContent ?? data.content}
          onCommit={(content) => void updateObjectData(objectId, { ...data, content, variable_text: undefined })}
        />

        {(data.missing_font || missingGlyphs.length > 0) ? (
          <div className="rounded border border-bb-warning-border bg-bb-warning-bg px-2 py-1.5 text-[11px] leading-4 text-bb-warning-fg">
            {data.missing_font
              ? t('toolbars.properties.font_missing', { font: data.font_family })
              : t('toolbars.properties.missing_glyphs', { glyphs: missingGlyphs.join(' ') })}
          </div>
        ) : null}

        <TextControls
          value={controlValue}
          onPatch={patchData}
          onLayoutChange={(layoutMode) => {
            void applyTextLayoutMode(objectId, data, layoutMode as TextLayoutMode, { bendRadiusFallback: 50 });
          }}
          pathControls={pathControls}
        />
      </div>
    </section>
  );
}
