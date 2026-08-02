interface SelectProps {
  label: string;
  value: string;
  options: { value: string; label: string }[];
  onChange: (value: string) => void;
  disabled?: boolean;
  selectClassName?: string;
}

export function Select({ label, value, options, onChange, disabled, selectClassName }: SelectProps) {
  return (
    <label className="flex min-h-8 items-center justify-between gap-3 text-xs">
      <span className="text-bb-text-muted shrink-0">{label}</span>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        disabled={disabled}
        className={`${selectClassName ?? 'w-24'} h-8 rounded-lg border border-bb-control-border bg-bb-input px-2 text-xs text-bb-text transition-colors focus:border-bb-accent focus:outline-none focus:ring-1 focus:ring-bb-accent/25 disabled:cursor-not-allowed disabled:opacity-50`}
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </label>
  );
}
