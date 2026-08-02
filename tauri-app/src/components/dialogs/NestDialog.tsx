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
import { Boxes } from 'lucide-react';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';
import { DIALOG_TONE, DialogButton, DialogFooter, DialogSection } from '../shared/DialogPrimitives';

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
    <MovableResizableDialogFrame
      title={t('dialog.nest.title')}
      titleId="nest-dialog-title"
      testId="nest-dialog"
      initialWidth={420}
      initialHeight={360}
      minWidth={380}
      minHeight={320}
      onRequestClose={onClose}
      closeOnBackdropClick
      footer={(
        <DialogFooter>
          <DialogButton tone={DIALOG_TONE.quiet} onClick={onClose}>{t('common.cancel')}</DialogButton>
          <DialogButton tone={DIALOG_TONE.primary} autoFocus disabled={nestingInProgress} onClick={() => { void runNest(); }}>
            {t('dialog.nest.button')}
          </DialogButton>
        </DialogFooter>
      )}
    >
      <div className="min-h-0 flex-1 overflow-y-auto bg-bb-bg/20 p-4">
        <DialogSection icon={<Boxes size={14} />} title={t('dialog.nest.title')}>
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
        </DialogSection>
      </div>
    </MovableResizableDialogFrame>,
    document.body,
  );
}
