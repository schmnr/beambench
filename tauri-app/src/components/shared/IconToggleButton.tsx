import type { ReactNode } from 'react';

interface IconToggleButtonProps {
  active: boolean;
  label: string;
  icon: ReactNode;
  inactiveIcon?: ReactNode;
  onClick: () => void;
  testId?: string;
}

/** Compact icon-first boolean control used by dense inspector rows. */
export function IconToggleButton({
  active,
  label,
  icon,
  inactiveIcon,
  onClick,
  testId,
}: IconToggleButtonProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      aria-pressed={active}
      title={label}
      data-testid={testId}
      className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border outline-none transition-colors focus-visible:ring-1 focus-visible:ring-bb-accent ${
        active
          ? 'border-bb-accent/50 bg-bb-accent/15 text-bb-accent hover:bg-bb-accent/20'
          : 'border-bb-border bg-bb-bg text-bb-text-muted hover:border-bb-accent/40 hover:bg-bb-hover hover:text-bb-text'
      }`}
    >
      {active ? icon : (inactiveIcon ?? icon)}
    </button>
  );
}
