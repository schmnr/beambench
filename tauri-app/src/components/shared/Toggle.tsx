interface ToggleProps {
  label?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  className?: string;
  labelFirst?: boolean;
}

export function Toggle({
  label,
  checked,
  onChange,
  disabled,
  className = '',
  labelFirst = false,
}: ToggleProps) {
  const labelText = label && <span className="text-bb-text-muted">{label}</span>;

  return (
    <label
      className={`flex min-h-6 min-w-6 shrink-0 items-center gap-1.5 text-xs ${
        labelFirst ? 'justify-between' : 'justify-center'
      } ${
        disabled ? 'cursor-not-allowed' : 'cursor-pointer'
      } ${className}`}
    >
      {labelFirst && labelText}
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        disabled={disabled}
        className="h-3 w-3 shrink-0 accent-bb-accent"
      />
      {!labelFirst && labelText}
    </label>
  );
}
