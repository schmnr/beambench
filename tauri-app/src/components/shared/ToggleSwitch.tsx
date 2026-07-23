interface ToggleSwitchProps {
  active: boolean;
  activeColor?: string;
  onClick: () => void;
  testId?: string;
  'aria-label'?: string;
  title?: string;
}

/** Pill-style switch — the app's boolean control for panel headers and rows. */
export function ToggleSwitch({
  active,
  activeColor,
  onClick,
  testId,
  'aria-label': ariaLabel,
  title,
}: ToggleSwitchProps) {
  return (
    <button
      type="button"
      onClick={(e) => { e.stopPropagation(); onClick(); }}
      onContextMenu={(e) => { e.preventDefault(); e.stopPropagation(); }}
      className={`relative h-4 w-8 shrink-0 rounded-full transition-colors ${
        active ? (activeColor ?? 'bg-green-500') : 'bg-bb-text/20'
      }`}
      data-testid={testId}
      aria-label={ariaLabel}
      aria-pressed={active}
      title={title}
    >
      <span className={`absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform ${
        active ? 'translate-x-4' : 'translate-x-0.5'
      }`} />
    </button>
  );
}
