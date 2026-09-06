import { useState } from 'react';

interface TextInputProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  onBlur?: () => void;
  onKeyDown?: (event: React.KeyboardEvent<HTMLInputElement>) => void;
  disabled?: boolean;
  commitOnBlur?: boolean;
  'data-testid'?: string;
}

export function TextInput({
  label,
  value,
  onChange,
  onBlur,
  onKeyDown,
  disabled,
  commitOnBlur = false,
  'data-testid': testId,
}: TextInputProps) {
  const [draft, setDraft] = useState<string | null>(null);
  return (
    <label className="flex min-h-8 items-center justify-between gap-3 text-xs">
      <span className="text-bb-text-muted shrink-0">{label}</span>
      <input
        type="text"
        value={commitOnBlur ? draft ?? value : value}
        onChange={(e) => commitOnBlur ? setDraft(e.target.value) : onChange(e.target.value)}
        onBlur={() => {
          if (commitOnBlur && draft !== null && draft !== value) onChange(draft);
          setDraft(null);
          onBlur?.();
        }}
        onKeyDown={(event) => {
          onKeyDown?.(event);
          if (commitOnBlur && !event.defaultPrevented && event.key === 'Enter' && !event.nativeEvent.isComposing) {
            event.currentTarget.blur();
          }
        }}
        disabled={disabled}
        data-testid={testId}
        className="h-8 w-32 rounded-lg border border-bb-control-border bg-bb-input px-2 text-xs text-bb-text transition-colors focus:border-bb-accent focus:outline-none focus:ring-1 focus:ring-bb-accent/25 disabled:cursor-not-allowed disabled:opacity-50"
      />
    </label>
  );
}
