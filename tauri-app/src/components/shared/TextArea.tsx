interface TextAreaProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  rows?: number;
  monospace?: boolean;
  describedBy?: string;
}

export function TextArea({
  label,
  value,
  onChange,
  disabled,
  rows = 3,
  monospace = false,
  describedBy,
}: TextAreaProps) {
  return (
    <label className="grid gap-1 text-xs">
      <span className="text-bb-text-muted">{label}</span>
      <textarea
        value={value}
        onChange={(e) => onChange(e.target.value)}
        disabled={disabled}
        rows={rows}
        aria-describedby={describedBy}
        spellCheck={!monospace}
        className={`w-full resize-y rounded-lg border border-bb-control-border bg-bb-input px-2 py-1.5 text-xs text-bb-text transition-colors focus:border-bb-accent focus:outline-none focus:ring-1 focus:ring-bb-accent/25 disabled:cursor-not-allowed disabled:opacity-50 ${monospace ? 'font-mono' : ''}`}
      />
    </label>
  );
}
