import { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { NumberInput } from '../shared/NumberInput';
import { Toggle } from '../shared/Toggle';
import { Copy } from 'lucide-react';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';
import { DIALOG_TONE, DialogButton, DialogFooter, DialogNotice, DialogSection } from '../shared/DialogPrimitives';

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

  const [count, setCount] = useState(6);
  const [rotateCopies, setRotateCopies] = useState(true);
  const [scaleCopies, setScaleCopies] = useState(false);
  const [finalScalePercent, setFinalScalePercent] = useState(100);

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
    <MovableResizableDialogFrame
      title={t('dialog.copy_along_path.title')}
      titleId="copy-along-path-dialog-title"
      testId="copy-along-path-dialog"
      initialWidth={430}
      initialHeight={390}
      minWidth={380}
      minHeight={340}
      onRequestClose={onClose}
      closeOnBackdropClick
      footer={(
        <DialogFooter>
          <DialogButton tone={DIALOG_TONE.quiet} onClick={onClose}>{t('common.close')}</DialogButton>
          <DialogButton tone={DIALOG_TONE.primary} onClick={() => void apply()} disabled={!isValid} data-testid="copy-along-path-submit">
            {t('common.apply')}
          </DialogButton>
        </DialogFooter>
      )}
    >
      <div className="min-h-0 flex-1 overflow-y-auto bg-bb-bg/20 p-4">
        <DialogSection icon={<Copy size={14} />} title={t('dialog.copy_along_path.title')}>
          <div className="space-y-3">
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
        </DialogSection>
        {!isValid && (
          <div className="mt-3"><DialogNotice tone={DIALOG_TONE.warning}>{t('dialog.copy_along_path.validation_error')}</DialogNotice></div>
        )}
      </div>
    </MovableResizableDialogFrame>,
    document.body,
  );
}
