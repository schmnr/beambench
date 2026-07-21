import { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import type { ResizeSlotsOptions } from '../../types/project';
import { useProjectStore } from '../../stores/projectStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { useAppStore } from '../../stores/appStore';
import { NumberInput } from '../shared/NumberInput';
import { mmToDisplay, displayToMm, roundDisplayLength, lengthStep, lengthUnitLabel, labelWithUnit } from '../../utils/lengthUnits';
import { useFocusTrap } from '../../hooks/useFocusTrap';

interface ResizeSlotsDialogProps {
  objectIds: string[];
  onClose: () => void;
}

const DEFAULT_OPTIONS: ResizeSlotsOptions = {
  currentThicknessMm: 3,
  newThicknessMm: 3,
  toleranceMm: 0,
  adjustSlotDepth: true,
  adjustSlotWidth: true,
  adjustTabHeight: true,
};

export function ResizeSlotsDialog({ objectIds, onClose }: ResizeSlotsDialogProps) {
  const { t } = useTranslation();
  const projectId = useProjectStore((s) => s.project?.metadata.project_id ?? null);
  const resizeSlots = useProjectStore((s) => s.resizeSlots);
  const displayUnit = useAppStore((s) => s.settings?.display_unit) ?? 'mm';
  const [options, setOptions] = useState<ResizeSlotsOptions>(DEFAULT_OPTIONS);
  const initialProjectIdRef = useRef(projectId);
  const dialogRef = useRef<HTMLDivElement>(null);
  useFocusTrap(dialogRef, true);

  useEffect(() => {
    const handleKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [onClose]);

  useEffect(() => {
    if (projectId !== initialProjectIdRef.current) {
      onClose();
    }
  }, [projectId, onClose]);

  const safeObjectIds = useMemo(() => [...objectIds], [objectIds]);
  const isValid = options.currentThicknessMm > 0
    && options.newThicknessMm > 0
    && options.toleranceMm >= 0;

  const apply = async (closeAfterApply: boolean) => {
    const currentProject = useProjectStore.getState().project;
    const currentProjectId = currentProject?.metadata.project_id ?? null;
    if (currentProjectId !== initialProjectIdRef.current) {
      useNotificationStore
        .getState()
        .push(t('dialog.resize_slots.error_project_changed'), 'warning');
      onClose();
      return;
    }
    if (!isValid || safeObjectIds.length === 0) return;
    const applied = await resizeSlots(safeObjectIds, options);
    if (applied && closeAfterApply) onClose();
  };

  return createPortal(
    <div
      ref={dialogRef}
      role="dialog"
      aria-modal="true"
      aria-labelledby="resize-slots-dialog-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="min-w-[360px] rounded-lg border border-bb-border bg-bb-panel p-4 shadow-xl">
        <h2 id="resize-slots-dialog-title" className="mb-3 text-sm font-semibold text-bb-text">
          {t('dialog.resize_slots.title')}
        </h2>
        <div className="space-y-2">
          <NumberInput
            label={labelWithUnit(t('dialog.resize_slots.old_thickness'), lengthUnitLabel(displayUnit))}
            value={roundDisplayLength(mmToDisplay(options.currentThicknessMm, displayUnit), displayUnit)}
            min={mmToDisplay(0.01, displayUnit)}
            step={lengthStep(displayUnit, 0.1, 0.005)}
            onChange={(value) => setOptions((current) => ({ ...current, currentThicknessMm: displayToMm(value, displayUnit) }))}
          />
          <NumberInput
            label={labelWithUnit(t('dialog.resize_slots.new_thickness'), lengthUnitLabel(displayUnit))}
            value={roundDisplayLength(mmToDisplay(options.newThicknessMm, displayUnit), displayUnit)}
            min={mmToDisplay(0.01, displayUnit)}
            step={lengthStep(displayUnit, 0.1, 0.005)}
            onChange={(value) => setOptions((current) => ({ ...current, newThicknessMm: displayToMm(value, displayUnit) }))}
          />
          <NumberInput
            label={labelWithUnit(t('dialog.resize_slots.tolerance'), lengthUnitLabel(displayUnit))}
            value={roundDisplayLength(mmToDisplay(options.toleranceMm, displayUnit), displayUnit)}
            min={0}
            step={lengthStep(displayUnit, 0.05, 0.002)}
            onChange={(value) => setOptions((current) => ({ ...current, toleranceMm: Math.max(0, displayToMm(value, displayUnit)) }))}
          />
          <CheckboxRow
            label={t('dialog.resize_slots.adjust_slot_depth')}
            checked={options.adjustSlotDepth ?? true}
            onChange={(checked) => setOptions((current) => ({ ...current, adjustSlotDepth: checked }))}
          />
          <CheckboxRow
            label={t('dialog.resize_slots.adjust_slot_width')}
            checked={options.adjustSlotWidth ?? true}
            onChange={(checked) => setOptions((current) => ({ ...current, adjustSlotWidth: checked }))}
          />
          <CheckboxRow
            label={t('dialog.resize_slots.adjust_tab_height')}
            checked={options.adjustTabHeight ?? true}
            onChange={(checked) => setOptions((current) => ({ ...current, adjustTabHeight: checked }))}
          />
        </div>
        {!isValid && (
          <div className="mt-3 rounded border border-bb-warning-border bg-bb-warning-bg px-2 py-1 text-xs text-bb-warning-fg">
            {t('dialog.resize_slots.validation_error')}
          </div>
        )}
        <div className="mt-4 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="rounded bg-bb-bg px-3 py-1 text-xs font-medium text-bb-text hover:bg-bb-hover"
          >
            {t('common.cancel')}
          </button>
          <button
            onClick={() => void apply(false)}
            disabled={!isValid}
            className="rounded bg-bb-bg px-3 py-1 text-xs font-medium text-bb-text hover:bg-bb-hover disabled:opacity-50"
          >
            {t('common.apply')}
          </button>
          <button
            onClick={() => void apply(true)}
            disabled={!isValid}
            className="rounded bg-bb-accent px-3 py-1 text-xs font-medium text-bb-on-accent disabled:opacity-50"
          >
            {t('common.ok')}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}

function CheckboxRow({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex items-center justify-between gap-3 rounded border border-bb-border bg-bb-bg px-2 py-1 text-xs text-bb-text">
      <span>{label}</span>
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.currentTarget.checked)}
        className="h-4 w-4 accent-bb-accent"
      />
    </label>
  );
}
