import { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import type { DockDirection, DockOptions } from '../../types/project';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { useAppStore } from '../../stores/appStore';
import { NumberInput } from '../shared/NumberInput';
import { Toggle } from '../shared/Toggle';
import { mmToDisplay, displayToMm, roundDisplayLength, lengthStep, lengthUnitLabel, labelWithUnit } from '../../utils/lengthUnits';
import { useFocusTrap } from '../../hooks/useFocusTrap';

interface DockDialogProps {
  objectIds: string[];
  onClose: () => void;
}

export function DockDialog({ objectIds, onClose }: DockDialogProps) {
  const { t } = useTranslation();
  const projectId = useProjectStore((s) => s.project?.metadata.project_id ?? null);
  const dockObjects = useProjectStore((s) => s.dockObjects);
  const savedSettings = useUiStore((s) => s.dockSettings);
  const updateDockSettings = useUiStore((s) => s.updateDockSettings);
  const [settings, setSettings] = useState<DockOptions>(savedSettings);
  const displayUnit = useAppStore((s) => s.settings?.display_unit) ?? 'mm';
  const initialProjectIdRef = useRef(projectId);
  const dialogRef = useRef<HTMLDivElement>(null);
  useFocusTrap(dialogRef, true);

  const directions: Array<{ value: DockDirection; label: string }> = [
    { value: 'left', label: t('dialog.dock.dock_left') },
    { value: 'right', label: t('dialog.dock.dock_right') },
    { value: 'up', label: t('dialog.dock.dock_up') },
    { value: 'down', label: t('dialog.dock.dock_down') },
  ];

  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [onClose]);

  useEffect(() => {
    if (projectId !== initialProjectIdRef.current) {
      onClose();
    }
  }, [projectId, onClose]);

  useEffect(() => {
    updateDockSettings(settings);
  }, [settings, updateDockSettings]);

  const safeObjectIds = useMemo(() => [...objectIds], [objectIds]);

  const applyUpdatedObjects = async (direction: DockDirection) => {
    const currentProject = useProjectStore.getState().project;
    const currentProjectId = currentProject?.metadata.project_id ?? null;
    if (currentProjectId !== initialProjectIdRef.current) {
      useNotificationStore.getState().push(t('dialog.dock.error_project_changed'), 'warning');
      onClose();
      return;
    }
    if (!currentProject || safeObjectIds.length === 0) {
      onClose();
      return;
    }
    const applied = await dockObjects(safeObjectIds, direction, settings);
    if (applied) {
      onClose();
    }
  };

  return createPortal(
    <div
      ref={dialogRef}
      role="dialog"
      aria-modal="true"
      aria-labelledby="dock-dialog-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="min-w-[340px] rounded-lg border border-bb-border bg-bb-panel p-4 shadow-xl">
        <h2 id="dock-dialog-title" className="mb-3 text-sm font-semibold text-bb-text">
          {t('dialog.dock.title')}
        </h2>
        <div className="grid grid-cols-2 gap-2">
          {directions.map((direction) => (
            <button
              key={direction.value}
              onClick={() => void applyUpdatedObjects(direction.value)}
              className="rounded border border-bb-border bg-bb-bg px-3 py-2 text-xs font-medium text-bb-text hover:bg-bb-hover"
            >
              {direction.label}
            </button>
          ))}
        </div>
        <div className="mt-3 space-y-2">
          <Toggle
            label={t('dialog.dock.move_as_group')}
            checked={settings.moveAsGroup}
            onChange={(checked) => setSettings((current) => ({ ...current, moveAsGroup: checked }))}
          />
          <Toggle
            label={t('dialog.dock.lock_inner')}
            checked={settings.lockInnerObjects}
            onChange={(checked) => setSettings((current) => ({ ...current, lockInnerObjects: checked }))}
          />
          <NumberInput
            label={labelWithUnit(t('dialog.dock.padding'), lengthUnitLabel(displayUnit))}
            value={roundDisplayLength(mmToDisplay(settings.paddingMm, displayUnit), displayUnit)}
            min={0}
            step={lengthStep(displayUnit, 0.5, 0.02)}
            onChange={(value) => setSettings((current) => ({ ...current, paddingMm: Math.max(0, displayToMm(value, displayUnit)) }))}
          />
        </div>
        <div className="mt-4 flex justify-end">
          <button
            onClick={onClose}
            className="rounded bg-bb-bg px-3 py-1 text-xs font-medium text-bb-text hover:bg-bb-hover"
          >
            {t('common.close')}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
