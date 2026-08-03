import { useEffect, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { useFocusTrap } from '../../hooks/useFocusTrap';
import {
  INSPECTOR_TITLE_BAR_CLASS,
  INSPECTOR_TITLE_BAR_ICON_CLASS,
  INSPECTOR_TITLE_BAR_LABEL_CLASS,
} from '../shared/panelAppearance';

interface ContextualToolSectionProps {
  title: string;
  icon: ReactNode;
  children: ReactNode;
  testId: string;
}

/**
 * Shared in-panel shell for canvas tools and selection modifiers. It mirrors
 * the Node Editing section so every contextual workflow has the same place,
 * hierarchy, collapse behavior, and active accent.
 */
export function ContextualToolSection({
  title,
  icon,
  children,
  testId,
}: ContextualToolSectionProps) {
  const [expanded, setExpanded] = useState(true);
  const sectionRef = useRef<HTMLElement>(null);

  // Contextual controls are appended after the object's regular properties and
  // can otherwise open below the visible part of a long inspector. Reveal the
  // section whenever a new tool workflow mounts (or replaces another one).
  useEffect(() => {
    const section = sectionRef.current;
    if (!section || typeof section.scrollIntoView !== 'function') return;
    const frame = window.requestAnimationFrame(() => {
      section.scrollIntoView({
        behavior: window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth',
        block: 'start',
        inline: 'nearest',
      });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [testId]);

  return (
    <section
      ref={sectionRef}
      data-testid={testId}
      className="scroll-mt-2 overflow-hidden rounded-lg border border-bb-border bg-bb-bg/40"
    >
      <button
        type="button"
        aria-expanded={expanded}
        aria-label={title}
        onClick={() => setExpanded((current) => !current)}
        className={`${INSPECTOR_TITLE_BAR_CLASS} text-left outline-none transition-[filter] hover:brightness-110 focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-bb-accent`}
      >
        <span className={INSPECTOR_TITLE_BAR_ICON_CLASS}>{icon}</span>
        <span className={INSPECTOR_TITLE_BAR_LABEL_CLASS}>
          {title}
        </span>
        {expanded
          ? <ChevronDown className="ml-auto h-3.5 w-3.5 text-bb-text-dim" />
          : <ChevronRight className="ml-auto h-3.5 w-3.5 text-bb-text-dim" />}
      </button>

      {expanded && <div className="flex flex-col gap-2.5 p-3">{children}</div>}
    </section>
  );
}

export function ContextualToolActions({
  onCancel,
  onApply,
  cancelLabel,
  applyLabel,
  applyDisabled = false,
  applyTestId,
}: {
  onCancel: () => void;
  onApply: () => void;
  cancelLabel: string;
  applyLabel: string;
  applyDisabled?: boolean;
  applyTestId?: string;
}) {
  return (
    <div className="flex justify-end gap-1.5 border-t border-bb-border pt-2.5">
      <button
        type="button"
        onClick={onCancel}
        className="h-8 rounded-lg border border-bb-border bg-bb-surface px-3 text-[11px] font-medium text-bb-text outline-none transition-colors hover:bg-bb-hover focus-visible:ring-1 focus-visible:ring-bb-accent"
      >
        {cancelLabel}
      </button>
      <button
        type="button"
        data-testid={applyTestId}
        onClick={onApply}
        disabled={applyDisabled}
        className="h-8 rounded-lg bg-bb-accent px-3 text-[11px] font-semibold text-bb-on-accent outline-none transition-colors hover:bg-bb-accent-hover focus-visible:ring-1 focus-visible:ring-bb-accent disabled:cursor-default disabled:opacity-50"
      >
        {applyLabel}
      </button>
    </div>
  );
}

export function ContextualToolPresentation({
  presentation,
  title,
  icon,
  testId,
  minWidthClass,
  onClose,
  children,
}: {
  presentation: 'dialog' | 'properties';
  title: string;
  icon: ReactNode;
  testId: string;
  minWidthClass: string;
  onClose: () => void;
  children: ReactNode;
}) {
  const dialogRef = useRef<HTMLDivElement>(null);
  useFocusTrap(dialogRef, presentation === 'dialog');

  if (presentation === 'properties') {
    return (
      <ContextualToolSection title={title} icon={icon} testId={testId}>
        {children}
      </ContextualToolSection>
    );
  }

  return createPortal(
    <div
      ref={dialogRef}
      role="dialog"
      aria-modal="true"
      aria-labelledby="dialog-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={(event) => { if (event.target === event.currentTarget) onClose(); }}
    >
      <div className={`max-h-[80vh] overflow-y-auto rounded-lg border border-bb-border bg-bb-panel p-4 shadow-xl ${minWidthClass}`}>
        <h2 id="dialog-title" className="mb-3 text-sm font-semibold text-bb-text">{title}</h2>
        {children}
      </div>
    </div>,
    document.body,
  );
}
