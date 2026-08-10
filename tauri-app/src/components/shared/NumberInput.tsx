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
}: NumberInputProps) {
  return (
    <label className="flex min-h-8 items-center justify-between gap-3 text-xs">
      <span className="text-bb-text-muted shrink-0">{label}</span>
      <NumberStepper
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        min={min}
        max={max}
        step={step}
        disabled={disabled}
        onKeyDown={onKeyDown}
        className={`${inputWidthClassName} h-8 rounded-lg border border-bb-control-border bg-bb-input px-2 text-right text-xs tabular-nums text-bb-text transition-colors focus:border-bb-accent focus:outline-none focus:ring-1 focus:ring-bb-accent/25 disabled:cursor-not-allowed disabled:opacity-50`}
      />
    </label>
  );
}
