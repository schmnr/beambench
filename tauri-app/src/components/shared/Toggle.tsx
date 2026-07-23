interface ToggleProps {
  label?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  className?: string;
  /** Kept for call-site compatibility; label-first is now the default layout. */
  labelFirst?: boolean;
}

/**
 * Checkbox row: label on the left, control on the right — the panel-row
 * convention everywhere. (The old default centered the pair, which left
 * checkboxes floating mid-panel.)
 */
export function Toggle({
  label,
  checked,
  onChange,
  disabled,
  className = '',
}: ToggleProps) {
  return (
    <label
      className={`flex min-h-6 items-center justify-between gap-2 text-xs ${
        disabled ? 'cursor-not-allowed' : 'cursor-pointer'
      } ${className}`}
    >
      {label ? <span className="text-bb-text-muted">{label}</span> : <span />}
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        disabled={disabled}
        className="h-3 w-3 shrink-0 accent-bb-accent"
      />
    </label>
  );
}
