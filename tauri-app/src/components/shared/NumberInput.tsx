import { useEffect, useState } from 'react';
import { NumberStepper } from './NumberStepper';

interface NumberInputProps {
  label: string;
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  disabled?: boolean;
  inputWidthClassName?: string;
  onKeyDown?: (event: React.KeyboardEvent<HTMLInputElement>) => void;
  /**
   * `change` reports every edit (dialogs that preview live). `blur` reports
   * once, when the field is left or Enter is pressed, clamped to min/max:
   * for fields that save to the project, so typing `0.15` does not save 0,
   * 0.1 and then 0.15 as separate undo steps. Stepper arrows commit at once.
   */
  commit?: 'change' | 'blur';
}

export function NumberInput({
  label,
  value,
  onChange,
  min,
  max,
  step = 1,
  disabled,
  inputWidthClassName = 'w-24',
  onKeyDown,
  commit = 'change',
}: NumberInputProps) {
  const [draft, setDraft] = useState<string | null>(null);
  useEffect(() => setDraft(null), [value, disabled]);

  const commitValue = (text: string) => {
    setDraft(null);
    if (text.trim() === '') return;
    let next = Number(text);
    if (!Number.isFinite(next)) return;
    if (min !== undefined) next = Math.max(min, next);
    if (max !== undefined) next = Math.min(max, next);
    if (next !== value) onChange(next);
  };

  return (
    <label className="flex min-h-8 items-center justify-between gap-3 text-xs">
      <span className="text-bb-text-muted shrink-0">{label}</span>
      <NumberStepper
        value={draft ?? value}
        onChange={(e) => {
          if (commit === 'change') {
            onChange(Number(e.target.value));
          } else if (e.target.dataset.stepping === 'true') {
            // The stepper arrows set the value programmatically: commit now.
            commitValue(e.target.value);
          } else {
            setDraft(e.target.value);
          }
        }}
        onBlur={commit === 'blur' && draft !== null ? () => commitValue(draft) : undefined}
        min={min}
        max={max}
        step={step}
        disabled={disabled}
        onKeyDown={(event) => {
          if (commit === 'blur' && draft !== null) {
            if (event.key === 'Enter') commitValue(draft);
            if (event.key === 'Escape') setDraft(null);
          }
          onKeyDown?.(event);
        }}
        className={`${inputWidthClassName} h-8 rounded-lg border border-bb-control-border bg-bb-input px-2 text-right text-xs tabular-nums text-bb-text transition-colors focus:border-bb-accent focus:outline-none focus:ring-1 focus:ring-bb-accent/25 disabled:cursor-not-allowed disabled:opacity-50`}
      />
    </label>
  );
}
