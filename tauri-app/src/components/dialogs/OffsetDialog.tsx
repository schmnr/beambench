import { useState, useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { useAppStore } from '../../stores/appStore';
import { useUiStore } from '../../stores/uiStore';
import { vectorService } from '../../services/vectorService';
import { NumberInput } from '../shared/NumberInput';
import { Select } from '../shared/Select';
import { Toggle } from '../shared/Toggle';
import { mmToDisplay, displayToMm, roundDisplayLength, lengthStep, lengthUnitLabel, labelWithUnit } from '../../utils/lengthUnits';
import type { OffsetCornerStyle, OffsetDirection } from '../../types/vector';
import { useFocusTrap } from '../../hooks/useFocusTrap';

interface OffsetDialogProps {
  objectIds: string[];
  onClose: () => void;
}

export function OffsetDialog({ objectIds, onClose }: OffsetDialogProps) {
  const { t } = useTranslation();
  const projectId = useProjectStore((s) => s.project?.metadata.project_id ?? null);
  const displayUnit = useAppStore((s) => s.settings?.display_unit) ?? 'mm';
  const setOffsetPreview = useUiStore((s) => s.setOffsetPreview);
  const [distance, setDistance] = useState(1);
  const [direction, setDirection] = useState<OffsetDirection>('outward');
  const [cornerStyle, setCornerStyle] = useState<OffsetCornerStyle>('miter');
  const [deleteOriginal, setDeleteOriginal] = useState(false);
  // Backend-derived: true only when the whole selection is open paths. Drives
  // the relabel (Side A/B/Both sides) and the live ghost preview.
  const [sourceAllOpen, setSourceAllOpen] = useState(false);
  const initialProjectIdRef = useRef(projectId);
  const dialogRef = useRef<HTMLDivElement>(null);
  // Stale-response guard (mirrors TrimTool/NodeTool request-id pattern).
  const previewReqRef = useRef(0);
  // Skip the debounce for the next preview run: true on mount (labels + ghost
  // should resolve as soon as the dialog opens) and again when the Both-sides
  // default is applied (its rerun replaces a ghost we deliberately withheld).
  const immediateRunRef = useRef(true);
  // Apply the Both-sides default at most once, and never over a user's choice.
  const userChangedDirectionRef = useRef(false);
  const appliedOpenDefaultRef = useRef(false);
  useFocusTrap(dialogRef, true);

  const objectIdsKey = objectIds.join(',');

  const directionOptions: Array<{ value: OffsetDirection; label: string }> = sourceAllOpen
    ? [
        { value: 'outward', label: t('dialog.offset.side_a') },
        { value: 'inward', label: t('dialog.offset.side_b') },
        { value: 'both', label: t('dialog.offset.both_sides') },
      ]
    : [
        { value: 'outward', label: t('dialog.offset.direction_outward') },
        { value: 'inward', label: t('dialog.offset.direction_inward') },
        { value: 'both', label: t('dialog.offset.direction_both') },
      ];

  const cornerOptions: Array<{ value: OffsetCornerStyle; label: string }> = [
    { value: 'miter', label: t('dialog.offset.corner_miter') },
    { value: 'round', label: t('dialog.offset.corner_round') },
    { value: 'bevel', label: t('dialog.offset.corner_bevel') },
  ];

  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [onClose]);

  useEffect(() => {
    if (projectId !== initialProjectIdRef.current) {
      onClose();
    }
  }, [projectId, onClose]);

  // Clear the canvas ghost when the dialog unmounts (covers every close path:
  // Apply, Cancel, Escape, backdrop click, project change).
  useEffect(() => () => setOffsetPreview(null), [setOffsetPreview]);

  // Live preview: first run immediate (so labels + ghost resolve together),
  // subsequent parameter changes debounced. A monotonic request id discards
  // out-of-order responses.
  useEffect(() => {
    const ids = objectIdsKey.length ? objectIdsKey.split(',') : [];
    if (ids.length === 0) {
      setOffsetPreview(null);
      setSourceAllOpen(false);
      return;
    }

    let cancelled = false;
    const run = async () => {
      const seq = ++previewReqRef.current;
      try {
        const preview = await vectorService.previewOffsetShapes(ids, distance, direction, cornerStyle);
        if (cancelled || seq !== previewReqRef.current) return; // stale response
        setSourceAllOpen(preview.source_all_open);
        // Default to Both sides once for an all-open selection, unless the user
        // has already picked a side.
        if (
          preview.source_all_open &&
          direction !== 'both' &&
          !appliedOpenDefaultRef.current &&
          !userChangedDirectionRef.current
        ) {
          appliedOpenDefaultRef.current = true;
          immediateRunRef.current = true;
          setOffsetPreview(null);
          setDirection('both');
          return;
        }
        setOffsetPreview(preview.paths.length > 0 ? preview.paths : null);
      } catch {
        if (cancelled || seq !== previewReqRef.current) return; // stale failure
        // Unknown topology on error: drop the ghost and fall back to the
        // generic Inward/Outward labels.
        setOffsetPreview(null);
        setSourceAllOpen(false);
      }
    };

    if (immediateRunRef.current) {
      immediateRunRef.current = false;
      void run();
      return () => { cancelled = true; };
    }
    const timer = setTimeout(() => void run(), 120);
    return () => { cancelled = true; clearTimeout(timer); };
  }, [objectIdsKey, distance, direction, cornerStyle, setOffsetPreview]);

  const handleSubmit = async () => {
    const currentProject = useProjectStore.getState().project;
    const currentProjectId = currentProject?.metadata.project_id ?? null;
    if (currentProjectId !== initialProjectIdRef.current) {
      useNotificationStore.getState().push(t('dialog.offset.error_project_changed'), 'warning');
      onClose();
      return;
    }

    if (currentProject && objectIds.some((id) => !currentProject.objects.some((object) => object.id === id))) {
      useNotificationStore.getState().push(t('dialog.offset.error_objects_unavailable'), 'warning');
      onClose();
      return;
    }

    try {
      await useProjectStore.getState().offsetShapes(objectIds, distance, direction, cornerStyle, deleteOriginal);
      onClose();
    } catch {
      // Store already shows error notification; keep dialog open so user can adjust params
    }
  };

  return createPortal(
    <div ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby="dialog-title" className="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="bg-bb-panel border border-bb-border rounded-lg shadow-xl p-4 min-w-[320px]">
        <h2 id="dialog-title" className="text-sm font-semibold text-bb-text mb-3">{t('dialog.offset.title')}</h2>
        <div className="space-y-2">
          <NumberInput
            label={labelWithUnit(t('dialog.offset.distance'), lengthUnitLabel(displayUnit))}
            value={roundDisplayLength(mmToDisplay(distance, displayUnit), displayUnit)}
            onChange={(v) => setDistance(displayToMm(v, displayUnit))}
            min={mmToDisplay(0.1, displayUnit)}
            step={lengthStep(displayUnit, 0.5, 0.02)}
          />
          <Select
            label={sourceAllOpen ? t('dialog.offset.side') : t('dialog.offset.direction')}
            value={direction}
            options={directionOptions}
            onChange={(value) => {
              userChangedDirectionRef.current = true;
              setDirection(value as OffsetDirection);
            }}
          />
          <Select
            label={t('dialog.offset.corner_style')}
            value={cornerStyle}
            options={cornerOptions}
            onChange={(value) => setCornerStyle(value as OffsetCornerStyle)}
          />
          <Toggle label={t('dialog.offset.delete_original')} checked={deleteOriginal} onChange={setDeleteOriginal} />
        </div>
        <div className="flex justify-end gap-2 mt-4">
          <button onClick={onClose} className="px-3 py-1 text-xs font-medium rounded bg-bb-bg hover:bg-bb-hover text-bb-text">{t('common.cancel')}</button>
          <button data-testid="offset-submit" onClick={() => void handleSubmit()} className="px-3 py-1 text-xs font-medium rounded bg-bb-accent hover:bg-bb-accent-hover text-bb-on-accent">{t('common.apply')}</button>
        </div>
      </div>
    </div>,
    document.body
  );
}
