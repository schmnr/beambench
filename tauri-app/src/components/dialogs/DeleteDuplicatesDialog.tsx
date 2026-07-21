import { useTranslation } from 'react-i18next';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';

interface DeleteDuplicatesDialogProps {
  duplicateCount: number;
  onCancel: () => void;
  onConfirm: () => void;
}

export function DeleteDuplicatesDialog({
  duplicateCount,
  onCancel,
  onConfirm,
}: DeleteDuplicatesDialogProps) {
  const { t } = useTranslation();
  const hasDuplicates = duplicateCount > 0;

  return (
    <MovableResizableDialogFrame
      title={t('dialog.delete_duplicates.title')}
      titleId="delete-duplicates-title"
      testId="delete-duplicates-dialog"
      initialWidth={360}
      initialHeight={190}
      minWidth={320}
      minHeight={170}
      onRequestClose={onCancel}
      footer={(
        <div className="flex justify-end gap-2 px-4 py-3">
          {hasDuplicates ? (
            <>
              <button
                type="button"
                onClick={onCancel}
                className="rounded border border-bb-border px-3 py-1.5 text-xs font-medium text-bb-text hover:bg-bb-hover"
              >
                {t('common.cancel')}
              </button>
              <button
                type="button"
                data-testid="delete-duplicates-confirm"
                onClick={onConfirm}
                className="rounded bg-bb-error px-3 py-1.5 text-xs font-semibold text-bb-on-error hover:bg-bb-error-hover"
              >
                {t('dialog.delete_duplicates.delete_button', { count: duplicateCount })}
              </button>
            </>
          ) : (
            <button
              type="button"
              data-testid="delete-duplicates-ok"
              onClick={onCancel}
              className="rounded border border-bb-border px-3 py-1.5 text-xs font-medium text-bb-text hover:bg-bb-hover"
            >
              {t('common.ok')}
            </button>
          )}
        </div>
      )}
    >
      <div className="flex flex-1 items-center px-5 py-4 text-sm text-bb-text">
        {hasDuplicates ? (
          <p>{t('dialog.delete_duplicates.detected', { count: duplicateCount })}</p>
        ) : (
          <p>{t('dialog.delete_duplicates.none_detected')}</p>
        )}
      </div>
    </MovableResizableDialogFrame>
  );
}
