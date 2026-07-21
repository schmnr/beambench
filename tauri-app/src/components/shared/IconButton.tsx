interface IconButtonProps {
  icon: React.ReactNode;
  label: string;
  onClick: (event?: React.MouseEvent<HTMLButtonElement>) => void;
  disabled?: boolean;
  active?: boolean;
  size?: 'xs' | 'sm' | 'md';
}

export function IconButton({ icon, label, onClick, disabled, active, size = 'md' }: IconButtonProps) {
  const dim = size === 'xs' ? 'w-7 h-7' : size === 'sm' ? 'w-9 h-9' : 'w-10 h-10';
  return (
    <button
      onClick={disabled ? undefined : onClick}
      disabled={disabled}
      aria-label={label}
      title={label}
      className={`${dim} flex items-center justify-center rounded ${
        active
          ? 'bg-bb-accent/15 border border-bb-accent/30 text-bb-text'
          : disabled
            ? 'text-bb-text-disabled cursor-default'
            : 'text-bb-text-muted hover:text-bb-text hover:bg-bb-surface'
      }`}
    >
      {icon}
    </button>
  );
}
