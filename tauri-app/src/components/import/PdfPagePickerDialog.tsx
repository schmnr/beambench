import { useEffect, useId, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { type PdfPageRequest, usePdfPagePickerStore } from '../../stores/pdfPagePickerStore';
import { DIALOG_TONE, DialogButton, DialogFooter } from '../shared/DialogPrimitives';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';

export function PdfPagePickerDialog() {
  const request = usePdfPagePickerStore((state) => state.request);
  return request ? <PdfPageForm key={request.id} request={request} /> : null;
}

function PdfPageForm({ request }: { request: PdfPageRequest }) {
  const { t } = useTranslation();
  const formId = useId();
  const [page, setPage] = useState<number>(NaN);
  const input = useRef<HTMLInputElement>(null);
  const validPage = Number.isInteger(page) && page >= 1 && page <= request.pageCount;
  const cancel = () => request.finish(null);

  useEffect(() => {
    input.current?.focus();
  }, []);

  return (
    <MovableResizableDialogFrame
      title={t('import.pdf_page.title')}
      titleId="pdf-page-title"
      initialWidth={420}
      initialHeight={245}
      minWidth={320}
      minHeight={235}
      onRequestClose={cancel}
      footer={(
        <DialogFooter>
          <DialogButton onClick={cancel}>{t('common.cancel')}</DialogButton>
          <DialogButton tone={DIALOG_TONE.primary} type="submit" form={formId} disabled={!validPage}>
            {t('import.pdf_page.confirm')}
          </DialogButton>
        </DialogFooter>
      )}
    >
      <form
        id={formId}
        className="space-y-4 overflow-y-auto px-5 py-4 text-sm text-bb-text"
        onSubmit={(event) => {
          event.preventDefault();
          if (validPage) request.finish(page);
        }}
      >
        <p className="truncate font-medium" title={request.filename} dir="auto">{request.filename}</p>
        <div className="space-y-1.5">
          <label htmlFor="pdf-page-number" className="block font-medium">
            {t('import.pdf_page.number')}
          </label>
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
            <input
              ref={input}
              id="pdf-page-number"
              type="number"
              min={1}
              max={request.pageCount}
              step={1}
              required
              aria-describedby="pdf-page-range"
              className="h-8 w-24 rounded-md border border-bb-border bg-bb-bg px-2 text-bb-text focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent"
              onChange={(event) => setPage(event.currentTarget.valueAsNumber)}
            />
            <p id="pdf-page-range" className="text-bb-text-muted">
              {t('import.pdf_page.range', { count: request.pageCount })}
            </p>
          </div>
        </div>
      </form>
    </MovableResizableDialogFrame>
  );
}
