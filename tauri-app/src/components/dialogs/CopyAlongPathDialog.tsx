import { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { NumberInput } from '../shared/NumberInput';
import { Toggle } from '../shared/Toggle';
import { useFocusTrap } from '../../hooks/useFocusTrap';

interface CopyAlongPathDialogProps {
  objectIds: string[];
  pathObjectId: string;
  onClose: () => void;
}

export function CopyAlongPathDialog({ objectIds, pathObjectId, onClose }: CopyAlongPathDialogProps) {
  const { t } = useTranslation();
  const projectId = useProjectStore((s) => s.project?.metadata.project_id ?? null);
  const copyAlongPath = useProjectStore((s) => s.copyAlongPath);
  const initialProjectIdRef = useRef(projectId);
  const safeObjectIds = useMemo(() => [...objectIds], [objectIds]);
  const dialogRef = useRef<HTMLDivElement>(null);
  useFocusTrap(dialogRef, true);

  const [count, setCount] = useState(6);
  const [rotateCopies, setRotateCopies] = useState(true);
  const [scaleCopies, setScaleCopies] = useState(false);
  const [finalScalePercent, setFinalScalePercent] = useState(100);

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

  const isValid = count >= 1 && finalScalePercent > 0 && finalScalePercent <= 10000;

  const apply = async () => {
    const currentProject = useProjectStore.getState().project;
    const currentProjectId = currentProject?.metadata.project_id ?? null;
    if (currentProjectId !== initialProjectIdRef.current) {
      useNotificationStore
        .getState()
        .push(t('dialog.copy_along_path.error_project_changed'), 'warning');
      onClose();
      return;
    }
    if (!isValid || safeObjectIds.length === 0) {
      return;
    }
    const applied = await copyAlongPath(safeObjectIds, pathObjectId, {
      count,
      rotateCopies,
      scaleCopies,
      finalScalePercent,
    });
    if (applied) {
      onClose();
    }
  };

  return createPortal(
    <div
      ref={dialogRef}
      role="dialog"
      aria-modal="true"
      aria-labelledby="copy-along-path-dialog-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="min-w-[360px] rounded-lg border border-bb-border bg-bb-panel p-4 shadow-xl">
        <h2 id="copy-along-path-dialog-title" className="mb-3 text-sm font-semibold text-bb-text">
          {t('dialog.copy_along_path.title')}
        </h2>
        <div className="space-y-2">
          <NumberInput
            label={t('dialog.copy_along_path.number_of_copies')}
            value={count}
            min={1}
            max={100}
            onChange={(value) => setCount(Math.max(1, Math.floor(value)))}
          />
          <Toggle
            label={t('dialog.copy_along_path.rotate_copies')}
            checked={rotateCopies}
            onChange={setRotateCopies}
          />
          <Toggle
            label={t('dialog.copy_along_path.scale_copies')}
            checked={scaleCopies}
            onChange={setScaleCopies}
          />
          <NumberInput
            label={t('dialog.copy_along_path.final_scale')}
            value={finalScalePercent}
            min={0.01}
            max={10000}
            step={1}
            disabled={!scaleCopies}
            onChange={setFinalScalePercent}
          />
        </div>
        {!isValid && (
          <div className="mt-3 rounded border border-bb-warning-border bg-bb-warning-bg px-2 py-1 text-xs text-bb-warning-fg">
            {t('dialog.copy_along_path.validation_error')}
          </div>
        )}
        <div className="mt-4 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="rounded bg-bb-bg px-3 py-1 text-xs font-medium text-bb-text hover:bg-bb-hover"
          >
            {t('common.close')}
          </button>
          <button
            onClick={() => void apply()}
            disabled={!isValid}
            data-testid="copy-along-path-submit"
            className="rounded bg-bb-accent px-3 py-1 text-xs font-medium text-bb-on-accent disabled:opacity-50"
          >
            {t('common.apply')}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
