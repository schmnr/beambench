import { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { open } from '@tauri-apps/plugin-dialog';
import { useProjectStore } from '../../stores/projectStore';
import { useAppStore } from '../../stores/appStore';
import { variableTextService } from '../../services/variableTextService';
import { defaultVariableTextConfig, templateHasVariableText } from '../../utils/variableText';
import { displayToMm, lengthUnitLabel, mmToDisplay, roundDisplayLength } from '../../utils/lengthUnits';
import type { VariableTextConfig, VariableTextMode } from '../../types/variableText';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';
import { DIALOG_TONE, DialogButton, DialogFooter, DialogNotice } from '../shared/DialogPrimitives';
import { NumberInput } from '../shared/NumberInput';
import { Select } from '../shared/Select';

const MODES = ['normal', 'serial_number', 'date_time', 'merge_csv'];
const DATE_FORMATS = ['YYYY-MM-DD', 'DD/MM/YYYY', 'MM/DD/YYYY', 'YYYY-MM-DD HH:mm:ss'];
const NO_HEADERS: string[] = [];

function normalizeSerialConfig(config: VariableTextConfig): VariableTextConfig {
  if (config.mode !== 'serial_number') return config;
  const serial = /\{Serial:\s*(-?\d+)\s*,\s*(-?\d+)\s*,\s*(\d+)\s*\}/.exec(config.template);
  if (!serial) return config;
  const start = Number(serial[1]);
  return { ...config, template: config.template.split(serial[0]).join('{Serial}'),
    source: { ...config.source, current: start, start, end: start, advanceBy: Number(serial[2]),
      fieldDefaults: { ...config.source.fieldDefaults, _serial_padding: serial[3] } } };
}

export function TextPersonalizationDialog({ objectId, onClose }: { objectId: string; onClose: () => void }) {
  const { t } = useTranslation();
  const project = useProjectStore((state) => state.project);
  const unit = useAppStore((state) => state.settings?.display_unit) ?? 'mm';
  const initialProjectId = useRef(project?.metadata.project_id);
  const object = project?.objects.find((entry) => entry.id === objectId);
  const [config, setConfig] = useState<VariableTextConfig>(() => {
    if (object?.data.type !== 'text') return defaultVariableTextConfig('SN-{Serial}', { mode: 'serial_number' });
    const saved = structuredClone(object.data.variable_text ?? defaultVariableTextConfig(
      templateHasVariableText(object.data.content) ? object.data.content : `${object.data.content}-{Serial}`,
      { mode: 'serial_number' },
    ));
    const mode = MODES.includes(saved.mode ?? '') ? saved.mode : (saved.template.includes('{CSV:') ? 'merge_csv' : saved.template.includes('{Date') ? 'date_time' : 'serial_number');
    // Map older inline counter options into the visible controls.
    return normalizeSerialConfig({ ...saved, mode });
  });
  const [spacingX, setSpacingX] = useState(() => Math.max(10, (object?.bounds.max.x ?? 0) - (object?.bounds.min.x ?? 0) + 5));
  const [spacingY, setSpacingY] = useState(0);
  const [dateTimestamp, setDateTimestamp] = useState(() => new Date().toISOString());
  const previewConfig = useMemo(() => ({ ...config, source: { ...config.source, fieldDefaults: { ...config.source.fieldDefaults, _date_timestamp: dateTimestamp } } }), [config, dateTimestamp]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [preview, setPreview] = useState<{ config: VariableTextConfig; values: string[] } | null>(null);
  const alive = useRef(true);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  useEffect(() => {
    if (project?.metadata.project_id !== initialProjectId.current || !object) onClose();
  }, [project?.metadata.project_id, object, onClose]);

  const headers = config.source.csvData[0] ?? NO_HEADERS;
  const rowCount = Math.max(0, config.source.csvData.length - 1);
  const draftError = useMemo(() => {
    const { source } = config;
    if (!config.template.trim() || !Number.isInteger(source.totalCopies) || source.totalCopies < 1 || source.totalCopies > 1000
      || !Number.isSafeInteger(source.current) || !Number.isSafeInteger(source.advanceBy)
      || Math.abs(source.current ?? 0) > 1e9 || Math.abs(source.advanceBy ?? 0) > 1e6 || source.advanceBy === 0
      || !Number.isFinite(spacingX) || !Number.isFinite(spacingY)) return t('personalization.invalid');
    if (config.mode === 'normal') return '';
    const fields = [...config.template.matchAll(/\{([^}]+)\}/g)].map((match) => match[1]);
    if (fields.some((field) => !/^(Serial(?::[-\d, ]+)?|Date(?::[^}]+)?|CSV:.+)$/.test(field))) return t('personalization.fields_error');
    if (fields.some((field) => field.startsWith('CSV:') && !headers.includes(field.slice(4)))) return t('personalization.fields_error');
    if (config.mode === 'merge_csv' && (rowCount === 0 || !fields.some((field) => field.startsWith('CSV:'))
      || (source.current ?? 0) < 0 || (source.current ?? 0) + (source.totalCopies - 1) * (source.advanceBy ?? 1) < 0 || (source.current ?? 0) + (source.totalCopies - 1) * (source.advanceBy ?? 1) >= rowCount)) return t('personalization.rows_error');
    return '';
  }, [config, headers, rowCount, spacingX, spacingY, t]);

  useEffect(() => {
    let active = true;
    if (draftError) return;
    const timer = setTimeout(() => {
      setError('');
      void variableTextService.generateBatchPreview(objectId, previewConfig).then((values) => {
        if (active) setPreview({ config: previewConfig, values });
      }).catch((failure: unknown) => { if (active) { setPreview(null); setError(String(failure)); } });
    }, 150);
    return () => { active = false; clearTimeout(timer); };
  }, [previewConfig, draftError, objectId]);

  const currentPreview = preview?.config === previewConfig ? preview.values : null;
  const problem = error || draftError || (currentPreview?.some((value) => !value.trim()) ? t('personalization.empty') : '');
  const ready = !busy && !object?.locked && !problem && !!currentPreview;
  const patchSource = (patch: Partial<VariableTextConfig['source']>) => setConfig((current) => ({ ...current, source: { ...current.source, ...patch } }));
  const changeMode = (mode: string) => {
    setError('');
    setConfig(defaultVariableTextConfig(mode === 'normal' ? (currentPreview?.[0] ?? (object?.data.type === 'text' ? object.data.content : '')) : mode === 'date_time' ? '{Date:YYYY-MM-DD}' : mode === 'merge_csv' ? '' : 'SN-{Serial}', { mode: mode as VariableTextMode }));
  };
  const loadCsv = async () => {
    setBusy(true); setError('');
    try {
      const path = await open({ multiple: false, filters: [{ name: t('panels.variable_text.mode_merge_csv'), extensions: ['csv'] }] });
      if (typeof path !== 'string' || !alive.current) return;
      const csv = await variableTextService.loadCsvFile(path);
      if (!alive.current) return;
      setConfig((current) => ({ ...current,
        template: current.template || `{CSV:${csv.headers[0]}}`,
        source: { ...current.source, csvPath: path, csvData: [csv.headers, ...csv.rows], current: 0, start: 0, end: csv.rows.length - 1, advanceBy: 1, totalCopies: Math.min(1000, csv.rows.length) },
      }));
    } catch (failure) { if (alive.current) setError(String(failure)); }
    finally { if (alive.current) setBusy(false); }
  };
  const apply = async (copies: boolean) => {
    const current = useProjectStore.getState();
    const target = current.project?.objects.find((entry) => entry.id === objectId);
    if (!ready || current.project?.metadata.project_id !== initialProjectId.current || target?.data.type !== 'text' || target.locked) return;
    setBusy(true);
    try {
      const success = copies
        ? await current.generatePersonalizedCopies(objectId, previewConfig, spacingX, spacingY)
        : await current.updateObjectData(objectId, { ...target.data, content: config.template, variable_text: config.mode === 'normal' ? undefined : config });
      if (alive.current && success) onClose();
    } finally { if (alive.current) setBusy(false); }
  };

  return createPortal(
    <MovableResizableDialogFrame title={t('personalization.title')} titleId="text-personalization-title" testId="text-personalization-dialog"
      initialWidth={720} initialHeight={650} minWidth={420} minHeight={380} onRequestClose={busy ? undefined : onClose}
      footer={<DialogFooter leading={<span className="text-xs text-bb-text-muted">{t('personalization.original')}</span>}>
        <DialogButton onClick={onClose} disabled={busy}>{t('common.cancel')}</DialogButton>
        <DialogButton disabled={!ready} onClick={() => void apply(false)}>{t('common.apply')}</DialogButton>
        <DialogButton tone={DIALOG_TONE.primary} disabled={!ready} onClick={() => void apply(true)}>{busy ? t('panels.variable_text.generating') : t('personalization.create')}</DialogButton>
      </DialogFooter>}
    >
      <div className="min-h-0 flex-1 overflow-auto p-4">
        <div className="flex flex-wrap gap-5">
          <fieldset disabled={busy} className="min-w-0 flex-1 basis-72 space-y-3">
            <Select label={t('panels.variable_text.mode')} value={config.mode ?? 'serial_number'} selectClassName="w-40" onChange={changeMode}
              options={MODES.map((value) => ({ value, label: t(`panels.variable_text.mode_${value}`) }))} />
            {config.mode === 'serial_number' && <>
              <NumberInput label={t('panels.variable_text.start')} value={config.source.current ?? 1} min={-1e9} max={1e9} onChange={(value) => patchSource({ current: value, start: value, end: value })} />
              <NumberInput label={t('panels.variable_text.advance_by')} value={config.source.advanceBy ?? 1} min={-1e6} max={1e6} onChange={(value) => patchSource({ advanceBy: value })} />
              <NumberInput label={t('panels.variable_text.serial_padding')} value={Number(config.source.fieldDefaults._serial_padding ?? 1)} min={1} max={12} onChange={(value) => patchSource({ fieldDefaults: { ...config.source.fieldDefaults, _serial_padding: String(Math.floor(value)) } })} />
            </>}
            {config.mode === 'date_time' && <>
              <Select label={t('personalization.date_format')} value={/\{Date:([^}]+)\}/.exec(config.template)?.[1] ?? 'YYYY-MM-DD'} selectClassName="w-52"
                options={[...new Set([...DATE_FORMATS, /\{Date:([^}]+)\}/.exec(config.template)?.[1] ?? 'YYYY-MM-DD'])].map((value) => ({ value, label: value }))}
                onChange={(value) => setConfig((current) => ({ ...current, template: current.template.replace(/\{Date(?::[^}]+)?\}/g, `{Date:${value}}`) }))} />
              <p className="text-xs leading-5 text-bb-text-muted">{t('personalization.date_help')}</p>
              <DialogButton onClick={() => setDateTimestamp(new Date().toISOString())}>{t('personalization.refresh_date')}</DialogButton>
            </>}
            {config.mode === 'merge_csv' && <>
              <DialogButton onClick={() => void loadCsv()}>{t('personalization.load')}</DialogButton>
              <p className="break-words text-xs text-bb-text-muted">{rowCount ? t('panels.variable_text.csv_loaded', { name: config.source.csvPath?.split(/[/\\]/).pop() ?? '', count: rowCount }) : t('panels.variable_text.no_csv')}</p>
              <div className="flex flex-wrap gap-1">{headers.map((header) => <DialogButton key={header} onClick={() => setConfig((current) => ({ ...current, template: `${current.template}{CSV:${header}}` }))}>{header}</DialogButton>)}</div>
              {rowCount > 0 && <NumberInput label={t('personalization.first_row')} value={(config.source.current ?? 0) + 1} min={1} max={rowCount} onChange={(value) => patchSource({ current: Math.floor(value) - 1 })} />}
            </>}
            <label className="flex flex-col gap-1 text-xs text-bb-text-muted">{t('panels.variable_text.template')}
              <textarea rows={3} value={config.template} onChange={(event) => setConfig((current) => normalizeSerialConfig({ ...current, template: event.target.value }))}
                className="w-full resize-y rounded-lg border border-bb-control-border bg-bb-input p-2 text-bb-text focus:border-bb-accent focus:outline-none" />
            </label>
            <NumberInput label={t('panels.variable_text.copies')} value={config.source.totalCopies} min={1} max={1000} onChange={(value) => patchSource({ totalCopies: Math.floor(value) })} />
            <NumberInput label={`${t('personalization.spacing_x')} (${lengthUnitLabel(unit)})`} value={roundDisplayLength(mmToDisplay(spacingX, unit), unit)} onChange={(value) => setSpacingX(displayToMm(value, unit))} />
            <NumberInput label={`${t('personalization.spacing_y')} (${lengthUnitLabel(unit)})`} value={roundDisplayLength(mmToDisplay(spacingY, unit), unit)} onChange={(value) => setSpacingY(displayToMm(value, unit))} />
          </fieldset>
          <section className="min-w-0 flex-1 basis-64" aria-label={t('panels.variable_text.preview')} aria-live="polite">
            <h3 className="mb-2 text-sm font-medium text-bb-text">{t('panels.variable_text.preview')}</h3>
            <p className="mb-3 text-xs leading-5 text-bb-text-muted">{t('personalization.preview_help')}</p>
            {problem ? <div role="alert"><DialogNotice tone={DIALOG_TONE.error}>{problem}</DialogNotice></div> : currentPreview ?
              <ol className="max-h-80 list-inside list-decimal overflow-auto divide-y divide-bb-border text-xs text-bb-text">{currentPreview.map((value, index) => <li key={index} className="whitespace-pre-wrap break-words py-2">{value}</li>)}</ol>
              : <p className="text-xs text-bb-text-muted">{t('personalization.loading')}</p>}
          </section>
        </div>
      </div>
    </MovableResizableDialogFrame>, document.body,
  );
}
