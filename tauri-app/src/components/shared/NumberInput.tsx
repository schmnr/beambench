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
}: NumberInputProps) {
  return (
    <label className="flex items-center justify-between gap-2 text-xs">
      <span className="text-bb-text-muted shrink-0">{label}</span>
      <NumberStepper
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        min={min}
        max={max}
        step={step}
        disabled={disabled}
        className={`${inputWidthClassName} px-1.5 py-0.5 bg-bb-input border border-bb-control-border rounded text-xs text-bb-text text-right focus:outline-none focus:border-bb-accent disabled:opacity-60`}
      />
    </label>
  );
}
