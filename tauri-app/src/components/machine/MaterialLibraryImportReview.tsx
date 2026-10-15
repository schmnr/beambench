import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import type { ClbImportReport } from '../../types/material';
import { useAppStore } from '../../stores/appStore';
import { formatSpeedForDisplay, speedUnitLabel } from '../../utils/speedUnits';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';
import { DIALOG_TONE, DialogButton, DialogFooter, DialogNotice } from '../shared/DialogPrimitives';

interface Props {
  report: ClbImportReport;
  importing: boolean;
  error: string | null;
  onCancel: () => void;
  onImport: () => void;
}

export function MaterialLibraryImportReview({ report, importing, error, onCancel, onImport }: Props) {
  const { t } = useTranslation();
  const settings = useAppStore((s) => s.settings);
  const unit = settings?.display_unit ?? 'mm';
  const timeUnit = settings?.speed_time_unit ?? 'minutes';

  return createPortal(
    <MovableResizableDialogFrame
      title={t('panels.machine.material_library.clb_review')}
      titleId="clb-review-title"
      testId="clb-import-review"
      initialWidth={620}
      initialHeight={520}
      minWidth={360}
      minHeight={300}
      onRequestClose={importing ? undefined : onCancel}
      closeOnBackdropClick
      footer={(
        <DialogFooter>
          <DialogButton tone={DIALOG_TONE.quiet} disabled={importing} onClick={onCancel}>
            {t('common.cancel')}
          </DialogButton>
          <DialogButton tone={DIALOG_TONE.primary} disabled={importing || report.presets.length === 0} onClick={onImport}>
            {t('panels.machine.material_library.import')}
          </DialogButton>
        </DialogFooter>
      )}
    >
      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto p-4 text-sm text-bb-text" aria-busy={importing}>
        <p>{t('panels.machine.material_library.clb_summary', { count: report.presets.length, skipped: report.skipped.length })}</p>
        <DialogNotice tone={DIALOG_TONE.warning}>
          {t('panels.machine.material_library.clb_notice')}
        </DialogNotice>
        {error && <DialogNotice tone={DIALOG_TONE.error} role="alert">{error}</DialogNotice>}
        {report.presets.map((preset) => (
          <details key={preset.id} className="border-b border-bb-border pb-2">
            <summary tabIndex={0} className="cursor-pointer break-words font-medium">
              {preset.name} ({t(`panels.machine.material_library.operation.${preset.operation}`)})
            </summary>
            <dl className="my-2 grid grid-cols-2 gap-x-4 gap-y-1 text-xs">
              <dt>{t('panels.machine.material_library.speed_with_unit', { unit: speedUnitLabel(unit, timeUnit) })}</dt>
              <dd>{formatSpeedForDisplay(preset.speed_mm_min, unit, timeUnit)}</dd>
              <dt>{t('panels.machine.material_library.power_percent')}</dt><dd>{preset.power_percent}</dd>
              <dt>{t('panels.machine.material_library.passes')}</dt><dd>{preset.passes}</dd>
            </dl>
            <pre className="whitespace-pre-wrap break-words text-xs text-bb-text-muted">{preset.notes}</pre>
          </details>
        ))}
        {report.skipped.map((item, index) => (
          <p key={index} className="break-words text-xs text-bb-warning-fg">{item.name}: {item.reason}</p>
        ))}
      </div>
    </MovableResizableDialogFrame>,
    document.body,
  );
}
