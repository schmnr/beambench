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
    <label className="flex items-center justify-between gap-2 text-xs">
      <span className="text-bb-text-muted shrink-0">{label}</span>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        disabled={disabled}
        className={`${selectClassName ?? 'w-24'} px-1 py-0.5 bg-bb-input border border-bb-control-border rounded text-xs text-bb-text focus:outline-none focus:border-bb-accent disabled:opacity-60`}
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
