import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import type { NestOptions } from '../../types/project';
import { nestSelected } from '../../commands/arrangeActions';
import { useNotificationStore } from '../../stores/notificationStore';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { useAppStore } from '../../stores/appStore';
import { NumberInput } from '../shared/NumberInput';
import { Toggle } from '../shared/Toggle';
import { mmToDisplay, displayToMm, roundDisplayLength, lengthStep, lengthUnitLabel, labelWithUnit } from '../../utils/lengthUnits';
import { useFocusTrap } from '../../hooks/useFocusTrap';

interface NestDialogProps {
  objectIds: string[];
  onClose: () => void;
}

export function NestDialog({ objectIds, onClose }: NestDialogProps) {
  const { t } = useTranslation();
  const projectId = useProjectStore((s) => s.project?.metadata.project_id ?? null);
  const savedSettings = useUiStore((s) => s.nestSettings);
  const updateNestSettings = useUiStore((s) => s.updateNestSettings);
  const nestingInProgress = useUiStore((s) => s.nestingInProgress);
  const [settings, setSettings] = useState<NestOptions>(savedSettings);
  const displayUnit = useAppStore((s) => s.settings?.display_unit) ?? 'mm';
  const initialProjectIdRef = useRef(projectId);
  const safeObjectIds = useMemo(() => [...objectIds], [objectIds]);
  // The Nest button below has autoFocus; the trap detects focus already inside
  // the dialog and leaves it there.
  const dialogRef = useRef<HTMLDivElement>(null);
  useFocusTrap(dialogRef, true);

  const runNest = useCallback(async () => {
    const currentProject = useProjectStore.getState().project;
    const currentProjectId = currentProject?.metadata.project_id ?? null;
    if (currentProjectId !== initialProjectIdRef.current) {
      useNotificationStore.getState().push(t('dialog.nest.error_project_changed'), 'warning');
      onClose();
      return;
    }
    if (!currentProject || safeObjectIds.length === 0 || nestingInProgress) {
      onClose();
      return;
    }

    const nextSettings: NestOptions = {
      ...settings,
      paddingMm: Math.max(0, settings.paddingMm),
    };
    updateNestSettings(nextSettings);
    onClose();
    await nestSelected(nextSettings, safeObjectIds);
  }, [nestingInProgress, onClose, safeObjectIds, settings, updateNestSettings, t]);

  useEffect(() => {
    const handleKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
      if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
        event.preventDefault();
        void runNest();
      }
    };
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [onClose, runNest]);

  useEffect(() => {
    if (projectId !== initialProjectIdRef.current) {
      onClose();
    }
  }, [projectId, onClose]);

  return createPortal(
    <div
      ref={dialogRef}
      role="dialog"
      aria-modal="true"
      aria-labelledby="nest-dialog-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="w-[360px] max-w-[90vw] rounded-lg border border-bb-border bg-bb-panel p-4 shadow-xl">
        <h2 id="nest-dialog-title" className="mb-3 text-sm font-semibold text-bb-text">
          {t('dialog.nest.title')}
        </h2>
        <div className="space-y-3">
          <NumberInput
            label={labelWithUnit(t('dialog.nest.min_spacing'), lengthUnitLabel(displayUnit))}
            value={roundDisplayLength(mmToDisplay(settings.paddingMm, displayUnit), displayUnit)}
            min={0}
            step={lengthStep(displayUnit, 0.25, 0.01)}
            onChange={(value) => setSettings((current) => ({ ...current, paddingMm: Math.max(0, displayToMm(value, displayUnit)) }))}
          />
          <Toggle
            label={t('dialog.nest.allow_rotation')}
            checked={settings.allowRotation}
            onChange={(checked) => setSettings((current) => ({ ...current, allowRotation: checked }))}
          />
          <Toggle
            label={t('dialog.nest.keep_contained')}
            checked={settings.lockInnerObjects}
            onChange={(checked) => setSettings((current) => ({ ...current, lockInnerObjects: checked }))}
          />
          <Toggle
            label={t('dialog.nest.allow_mirror')}
            checked={settings.allowMirror}
            onChange={(checked) => setSettings((current) => ({ ...current, allowMirror: checked }))}
          />
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="rounded bg-bb-bg px-3 py-1.5 text-xs font-medium text-bb-text hover:bg-bb-hover"
          >
            {t('common.cancel')}
          </button>
          <button
            type="button"
            autoFocus
            disabled={nestingInProgress}
            onClick={() => { void runNest(); }}
            className="rounded bg-bb-accent px-3 py-1.5 text-xs font-medium text-bb-on-accent hover:bg-bb-accent-hover disabled:opacity-60"
          >
            {t('dialog.nest.button')}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
