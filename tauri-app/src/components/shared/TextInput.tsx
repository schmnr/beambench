interface TextInputProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  onBlur?: () => void;
  onKeyDown?: (event: React.KeyboardEvent<HTMLInputElement>) => void;
  disabled?: boolean;
  'data-testid'?: string;
}

export function TextInput({
  label,
  value,
  onChange,
  onBlur,
  onKeyDown,
  disabled,
  'data-testid': testId,
}: TextInputProps) {
  return (
    <label className="flex items-center justify-between gap-2 text-xs">
      <span className="text-bb-text-muted shrink-0">{label}</span>
      <input
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onBlur={onBlur}
        onKeyDown={onKeyDown}
        disabled={disabled}
        data-testid={testId}
        className="w-28 px-1.5 py-0.5 bg-bb-input border border-bb-control-border rounded text-xs text-bb-text focus:outline-none focus:border-bb-accent disabled:opacity-60"
      />
    </label>
  );
}
