import type { ButtonHTMLAttributes, ReactNode } from 'react';

export type DialogButtonTone = 'primary' | 'secondary' | 'danger' | 'quiet';
export const DIALOG_TONE = {
  primary: 'primary',
  secondary: 'secondary',
  danger: 'danger',
  quiet: 'quiet',
  info: 'info',
  warning: 'warning',
  error: 'error',
  success: 'success',
} as const;
export const DIALOG_TAB_ORIENTATION = {
  horizontal: 'horizontal',
  vertical: 'vertical',
} as const;

const buttonToneClasses: Record<DialogButtonTone, string> = {
  primary:
    'border-bb-accent bg-bb-accent text-bb-on-accent hover:border-bb-accent-hover hover:bg-bb-accent-hover',
  secondary:
    'border-bb-border bg-bb-bg text-bb-text hover:border-bb-accent/40 hover:bg-bb-hover',
  danger:
    'border-bb-error-border bg-bb-error-bg text-bb-error-fg hover:border-bb-error hover:bg-bb-error/20',
  quiet:
    'border-transparent bg-transparent text-bb-text-muted hover:bg-bb-hover hover:text-bb-text',
};

export function DialogButton({
  tone = 'secondary',
  icon,
  className = '',
  children,
  type = 'button',
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  tone?: DialogButtonTone;
  icon?: ReactNode;
}) {
  return (
    <button
      type={type}
      className={`inline-flex h-8 items-center justify-center gap-1.5 rounded-lg border px-3 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent disabled:cursor-not-allowed disabled:opacity-45 ${buttonToneClasses[tone]} ${className}`}
      {...props}
    >
      {icon}
      {children}
    </button>
  );
}

export function DialogFooter({
  leading,
  children,
}: {
  leading?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="flex min-h-14 flex-wrap items-center justify-between gap-2 bg-bb-surface/35 px-4 py-3">
      <div className="flex min-w-0 flex-wrap items-center gap-2">{leading}</div>
      <div className="flex flex-wrap items-center justify-end gap-2">{children}</div>
    </div>
  );
}

export function DialogSectionHeader({
  icon,
  title,
  description,
  actions,
}: {
  icon?: ReactNode;
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <div className="flex min-h-10 items-center justify-between gap-3 border-b border-bb-border bg-gradient-to-r from-bb-accent/10 via-bb-surface/45 to-transparent px-3 py-2">
      <div className="min-w-0">
        <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-[0.12em] text-bb-text">
          {icon ? <span className="text-bb-accent">{icon}</span> : null}
          <span className="truncate">{title}</span>
        </div>
        {description ? (
          <div className="mt-1 max-w-2xl text-[11px] leading-4 text-bb-text-muted">
            {description}
          </div>
        ) : null}
      </div>
      {actions ? <div className="flex shrink-0 items-center gap-1.5">{actions}</div> : null}
    </div>
  );
}

export function DialogSection({
  icon,
  title,
  description,
  actions,
  children,
  className = '',
}: {
  icon?: ReactNode;
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={`overflow-hidden rounded-xl border border-bb-border bg-bb-panel ${className}`}>
      <DialogSectionHeader icon={icon} title={title} description={description} actions={actions} />
      <div className="p-3">{children}</div>
    </section>
  );
}

export function DialogNotice({
  tone = 'info',
  icon,
  children,
  actions,
  testId,
  role = 'status',
}: {
  tone?: 'info' | 'warning' | 'error' | 'success';
  icon?: ReactNode;
  children: ReactNode;
  actions?: ReactNode;
  testId?: string;
  role?: 'status' | 'alert';
}) {
  const toneClass = {
    info: 'border-bb-accent/30 bg-bb-accent/8 text-bb-text-muted',
    warning: 'border-bb-warning-border bg-bb-warning-bg text-bb-warning-fg',
    error: 'border-bb-error-border bg-bb-error-bg text-bb-error-fg',
    success: 'border-bb-success-border bg-bb-success-bg text-bb-success-fg',
  }[tone];

  return (
    <div
      role={role}
      data-testid={testId}
      className={`flex items-start justify-between gap-3 rounded-lg border px-3 py-2 text-xs leading-5 ${toneClass}`}
    >
      <div className="flex min-w-0 items-start gap-2">
        {icon ? <span className="mt-0.5 shrink-0">{icon}</span> : null}
        <div className="min-w-0">{children}</div>
      </div>
      {actions ? <div className="flex shrink-0 flex-wrap items-center justify-end gap-1.5">{actions}</div> : null}
    </div>
  );
}

export function DialogTabs<T extends string>({
  tabs,
  activeTab,
  onChange,
  orientation = 'horizontal',
  testId = 'tab-bar',
}: {
  tabs: ReadonlyArray<{ id: T; label: ReactNode; icon?: ReactNode }>;
  activeTab: T;
  onChange: (tab: T) => void;
  orientation?: 'horizontal' | 'vertical';
  testId?: string;
}) {
  const vertical = orientation === 'vertical';
  return (
    <div
      role="tablist"
      aria-orientation={orientation}
      data-testid={testId}
      className={
        vertical
          ? 'flex w-full flex-col gap-1 p-2'
          : 'flex min-h-10 items-end gap-1 overflow-x-auto border-b border-bb-border bg-bb-surface/35 px-3 pt-1'
      }
    >
      {tabs.map((tab) => {
        const active = tab.id === activeTab;
        return (
          <button
            key={tab.id}
            type="button"
            role="tab"
            aria-selected={active}
            data-testid={`tab-${tab.id}`}
            onClick={() => onChange(tab.id)}
            className={`group flex items-center gap-2 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-bb-accent ${
              vertical
                ? `min-h-9 w-full rounded-lg border px-3 text-left ${
                    active
                      ? 'border-bb-accent/35 bg-bb-accent/10 text-bb-text'
                      : 'border-transparent text-bb-text-muted hover:bg-bb-hover hover:text-bb-text'
                  }`
                : `relative h-9 shrink-0 rounded-t-lg px-3 ${
                    active
                      ? 'bg-bb-panel text-bb-text after:absolute after:inset-x-2 after:bottom-0 after:h-0.5 after:rounded-full after:bg-bb-accent'
                      : 'text-bb-text-muted hover:bg-bb-hover/60 hover:text-bb-text'
                  }`
            }`}
          >
            {tab.icon ? (
              <span className={active ? 'text-bb-accent' : 'text-bb-text-dim group-hover:text-bb-text-muted'}>
                {tab.icon}
              </span>
            ) : null}
            <span className="truncate">{tab.label}</span>
          </button>
        );
      })}
    </div>
  );
}

export const dialogControlClassName =
  'h-8 rounded-lg border border-bb-control-border bg-bb-input px-2 text-xs text-bb-text transition-colors focus:border-bb-accent focus:outline-none focus:ring-1 focus:ring-bb-accent/25 disabled:cursor-not-allowed disabled:opacity-50';
