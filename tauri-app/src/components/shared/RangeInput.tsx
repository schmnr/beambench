import { NumberInput } from './NumberInput';

interface RangeInputProps {
  label: string;
  value: number;
  onChange: (value: number) => void;
  min: number;
  max: number;
  step?: number;
  disabled?: boolean;
  inputWidthClassName?: string;
  testId?: string;
}

export function rangeTrackBackground(value: number, min: number, max: number): string {
  const percent = max > min
    ? Math.min(100, Math.max(0, ((value - min) / (max - min)) * 100))
    : 0;
  return `linear-gradient(to right, rgb(var(--bb-accent)) 0%, #f59e0b ${percent}%, rgb(var(--bb-surface-3)) ${percent}%, rgb(var(--bb-surface-3)) 100%)`;
}

/** Number field plus the cut-settings gradient slider for bounded parameters. */
export function RangeInput({
  label,
  value,
  onChange,
  min,
  max,
  step = 1,
  disabled,
  inputWidthClassName,
  testId,
}: RangeInputProps) {
  return (
    <div className="flex flex-col gap-1.5">
      <NumberInput
        label={label}
        value={value}
        onChange={onChange}
        min={min}
        max={max}
        step={step}
        disabled={disabled}
        inputWidthClassName={inputWidthClassName}
      />
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
        disabled={disabled}
        aria-label={label}
        className="bb-range w-full disabled:cursor-not-allowed disabled:opacity-50"
        style={{ background: rangeTrackBackground(value, min, max) }}
        data-testid={testId}
      />
    </div>
  );
}
