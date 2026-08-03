import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { ChevronDown, Minus, Plus } from 'lucide-react';
import { useAppStore } from '../../stores/appStore';
import { useUiStore } from '../../stores/uiStore';
import {
  displayToMm,
  labelWithUnit,
  lengthStep,
  lengthUnitLabel,
  mmToDisplay,
  roundDisplayLength,
} from '../../utils/lengthUnits';

const GRID_PRESETS_MM = [1, 2.5, 5, 10, 25] as const;
const GRID_PRESETS_INCHES = [0.05, 0.1, 0.25, 0.5, 1] as const;
const MIN_GRID_SPACING_MM = 0.1;

function formatSpacing(value: number, displayUnit: 'mm' | 'inches'): string {
  return String(roundDisplayLength(value, displayUnit));
}

export function GridSpacingControl({ label }: { label: string }) {
  const settings = useAppStore((state) => state.settings);
  const gridSpacingMm = useUiStore((state) => state.gridSpacingMm);
  const setGridSpacing = useUiStore((state) => state.setGridSpacing);
  const displayUnit = settings?.display_unit === 'inches' ? 'inches' : 'mm';
  const unit = lengthUnitLabel(displayUnit);
  const displayedSpacing = roundDisplayLength(mmToDisplay(gridSpacingMm, displayUnit), displayUnit);
  const presets = displayUnit === 'inches' ? GRID_PRESETS_INCHES : GRID_PRESETS_MM;
  const step = lengthStep(displayUnit, 1, 0.05);
  const minimum = mmToDisplay(MIN_GRID_SPACING_MM, displayUnit);
  const inputLabel = labelWithUnit(label, unit);

  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<string | null>(null);
  const [position, setPosition] = useState({ top: 0, right: 0 });
  const triggerRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);

  const reposition = () => {
    const rect = triggerRef.current?.getBoundingClientRect();
    if (!rect) return;
    setPosition({
      top: rect.bottom + 6,
      right: Math.max(8, window.innerWidth - rect.right),
    });
  };

  useEffect(() => {
    if (!open) return;
    reposition();

    const close = () => {
      setOpen(false);
      setDraft(null);
    };
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (triggerRef.current?.contains(target) || popoverRef.current?.contains(target)) return;
      close();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      close();
      triggerRef.current?.focus();
    };

    document.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    window.addEventListener('resize', reposition);
    window.addEventListener('scroll', reposition, true);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('resize', reposition);
      window.removeEventListener('scroll', reposition, true);
    };
  }, [open]);

  const applyDisplayedSpacing = (displayValue: number) => {
    if (!Number.isFinite(displayValue) || displayValue <= 0) {
      setDraft(null);
      return;
    }
    const spacingMm = Math.max(MIN_GRID_SPACING_MM, displayToMm(displayValue, displayUnit));
    setDraft(null);
    setGridSpacing(spacingMm);
    void useAppStore.getState().updateSettings({ grid_spacing_mm: spacingMm });
  };

  const nudgeSpacing = (direction: -1 | 1) => {
    const next = Math.max(minimum, displayedSpacing + direction * step);
    applyDisplayedSpacing(roundDisplayLength(next, displayUnit));
  };

  const togglePopover = () => {
    if (open) {
      setOpen(false);
      setDraft(null);
      return;
    }
    reposition();
    setOpen(true);
  };

  return (
    <div className="shrink-0">
      <button
        ref={triggerRef}
        type="button"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={label}
        title={label}
        data-testid="toolbar-grid-spacing-trigger"
        onClick={togglePopover}
        className={`flex h-8 min-w-[68px] items-center justify-center gap-1 rounded-lg border px-1.5 text-xs tabular-nums outline-none transition-colors focus-visible:ring-1 focus-visible:ring-bb-accent ${
          open
            ? 'border-bb-accent/40 bg-bb-accent/10 text-bb-text'
            : 'border-transparent text-bb-text-muted hover:border-bb-control-border hover:bg-bb-surface hover:text-bb-text'
        }`}
      >
        <span>{formatSpacing(displayedSpacing, displayUnit)} {unit}</span>
        <ChevronDown size={11} className="text-bb-text-dim" aria-hidden />
      </button>

      {open && createPortal(
        <div
          ref={popoverRef}
          role="dialog"
          aria-label={label}
          className="fixed z-[80] w-64 rounded-xl border border-bb-control-border bg-bb-surface-elevated p-3 shadow-md"
          style={{ top: position.top, right: position.right }}
        >
          <div className="flex items-center gap-2">
            <button
              type="button"
              aria-label={`${label}: −${formatSpacing(step, displayUnit)} ${unit}`}
              title={`−${formatSpacing(step, displayUnit)} ${unit}`}
              disabled={displayedSpacing <= minimum}
              onClick={() => nudgeSpacing(-1)}
              className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-bb-text-muted outline-none transition-colors hover:bg-bb-hover hover:text-bb-text focus-visible:ring-1 focus-visible:ring-bb-accent disabled:cursor-default disabled:opacity-35"
            >
              <Minus size={15} />
            </button>

            <label className="flex h-8 min-w-0 flex-1 items-center rounded-lg border border-bb-control-border bg-bb-input px-2 focus-within:border-bb-accent">
              <input
                type="number"
                min={minimum}
                step={lengthStep(displayUnit, 0.1, 0.005)}
                value={draft ?? formatSpacing(displayedSpacing, displayUnit)}
                aria-label={inputLabel}
                data-testid="toolbar-grid-spacing"
                onChange={(event) => setDraft(event.currentTarget.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter') event.currentTarget.blur();
                }}
                onBlur={(event) => applyDisplayedSpacing(Number(event.currentTarget.value))}
                className="min-w-0 flex-1 bg-transparent px-0.5 text-xs tabular-nums text-bb-text outline-none [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
              />
              <span className="shrink-0 text-[10px] text-bb-text-dim" aria-hidden="true">{unit}</span>
            </label>

            <button
              type="button"
              aria-label={`${label}: +${formatSpacing(step, displayUnit)} ${unit}`}
              title={`+${formatSpacing(step, displayUnit)} ${unit}`}
              onClick={() => nudgeSpacing(1)}
              className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-bb-text-muted outline-none transition-colors hover:bg-bb-hover hover:text-bb-text focus-visible:ring-1 focus-visible:ring-bb-accent"
            >
              <Plus size={15} />
            </button>
          </div>

          <div className="mt-3 grid grid-cols-5 gap-1.5">
            {presets.map((preset) => {
              const active = Math.abs(displayedSpacing - preset) < 0.0001;
              const value = formatSpacing(preset, displayUnit);
              return (
                <button
                  key={preset}
                  type="button"
                  aria-pressed={active}
                  aria-label={`${label}: ${value} ${unit}`}
                  title={`${value} ${unit}`}
                  onClick={() => applyDisplayedSpacing(preset)}
                  className={`h-8 min-w-0 rounded-lg border px-1 text-xs font-medium tabular-nums outline-none transition-colors focus-visible:ring-1 focus-visible:ring-bb-accent ${
                    active
                      ? 'border-bb-accent bg-bb-accent/15 text-bb-accent'
                      : 'border-bb-control-border bg-bb-surface text-bb-text-muted hover:border-bb-accent/50 hover:bg-bb-hover hover:text-bb-text'
                  }`}
                >
                  {value}
                </button>
              );
            })}
          </div>
        </div>,
        document.body,
      )}
    </div>
  );
}
