import { useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { FileText, Printer } from 'lucide-react';
import { printService, type PrintAppearance, type PrintMode } from '../../services/printService';
import { wrapBackendError } from '../../i18n/errors';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';
import {
  DIALOG_TONE,
  DialogButton,
  DialogFooter,
  DialogNotice,
  DialogSection,
} from '../shared/DialogPrimitives';

const BLACK_MODE: PrintMode = 'black';
const COLOR_MODE: PrintMode = 'color';
const OPERATION_APPEARANCE: PrintAppearance = 'operation';
const OUTLINE_APPEARANCE: PrintAppearance = 'outline';

function ChoiceGroup<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: ReadonlyArray<{ value: T; label: string }>;
  onChange: (value: T) => void;
}) {
  return (
    <fieldset className="space-y-2">
      <legend className="text-xs font-medium text-bb-text-muted">{label}</legend>
      <div className="grid grid-cols-2 gap-1.5" role="radiogroup" aria-label={label}>
        {options.map((option) => {
          const selected = option.value === value;
          return (
            <button
              key={option.value}
              type="button"
              role="radio"
              aria-checked={selected}
              data-testid={`print-option-${option.value}`}
              onClick={() => onChange(option.value)}
              className={`h-9 rounded-lg border px-3 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent ${
                selected
                  ? 'border-bb-accent/55 bg-bb-accent/12 text-bb-text'
                  : 'border-bb-border bg-bb-bg text-bb-text-muted hover:bg-bb-hover hover:text-bb-text'
              }`}
            >
              {option.label}
            </button>
          );
        })}
      </div>
    </fieldset>
  );
}

export function PrintDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const [mode, setMode] = useState<PrintMode>(BLACK_MODE);
  const [appearance, setAppearance] = useState<PrintAppearance>(OPERATION_APPEARANCE);
  const [printing, setPrinting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const print = async () => {
    try {
      setPrinting(true);
      setError(null);
      await printService.printProject(mode, appearance);
      onClose();
    } catch (printError) {
      setError(wrapBackendError(String(printError)));
    } finally {
      setPrinting(false);
    }
  };

  return createPortal(
    <MovableResizableDialogFrame
      title={t('dialog.print.title')}
      titleId="print-dialog-title"
      testId="print-dialog"
      initialWidth={430}
      initialHeight={400}
      minWidth={390}
      minHeight={360}
      onRequestClose={onClose}
      closeOnBackdropClick={!printing}
      footer={(
        <DialogFooter>
          <DialogButton tone={DIALOG_TONE.quiet} disabled={printing} onClick={onClose}>
            {t('common.cancel')}
          </DialogButton>
          <DialogButton tone={DIALOG_TONE.primary} icon={<Printer size={13} />} disabled={printing} onClick={() => { void print(); }}>
            {printing ? t('dialog.print.preparing') : t('dialog.print.print')}
          </DialogButton>
        </DialogFooter>
      )}
    >
      <div className="min-h-0 flex-1 overflow-y-auto bg-bb-bg/20 p-4">
        <DialogSection
          icon={<FileText size={14} />}
          title={t('dialog.print.output')}
          description={t('dialog.print.description')}
        >
          <div className="space-y-4">
            {error && <DialogNotice tone={DIALOG_TONE.error} role="alert">{error}</DialogNotice>}
            <ChoiceGroup
              label={t('dialog.print.color')}
              value={mode}
              onChange={setMode}
              options={[
                { value: BLACK_MODE, label: t('dialog.print.black') },
                { value: COLOR_MODE, label: t('dialog.print.layer_colors') },
              ]}
            />
            <ChoiceGroup
              label={t('dialog.print.appearance')}
              value={appearance}
              onChange={setAppearance}
              options={[
                { value: OPERATION_APPEARANCE, label: t('dialog.print.by_operation') },
                { value: OUTLINE_APPEARANCE, label: t('dialog.print.outlines') },
              ]}
            />
          </div>
        </DialogSection>
      </div>
    </MovableResizableDialogFrame>,
    document.body,
  );
}
