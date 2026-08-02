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
    <label className="flex min-h-8 items-center justify-between gap-3 text-xs">
      <span className="text-bb-text-muted shrink-0">{label}</span>
      <input
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onBlur={onBlur}
        onKeyDown={onKeyDown}
        disabled={disabled}
        data-testid={testId}
        className="h-8 w-32 rounded-lg border border-bb-control-border bg-bb-input px-2 text-xs text-bb-text transition-colors focus:border-bb-accent focus:outline-none focus:ring-1 focus:ring-bb-accent/25 disabled:cursor-not-allowed disabled:opacity-50"
      />
    </label>
  );
}
