import { useRef, useCallback, useEffect, useState, useId } from 'react';
import { ChevronUp, ChevronDown } from 'lucide-react';

interface NumberStepperProps {
  value: number | string;
  onChange: (e: React.ChangeEvent<HTMLInputElement>) => void;
  onBlur?: (e: React.FocusEvent<HTMLInputElement>) => void;
  onKeyDown?: (e: React.KeyboardEvent<HTMLInputElement>) => void;
  min?: number;
  max?: number;
  step?: number | string;
  disabled?: boolean;
  className?: string;
  containerClassName?: string;
  placeholder?: string;
  'data-testid'?: string;
  'aria-label'?: string;
}

export function NumberStepper({
  value,
  onChange,
  onBlur,
  onKeyDown,
  min,
  max,
  step = 1,
  disabled,
  className = '',
  containerClassName = 'w-fit',
  placeholder,
  'data-testid': testId,
  'aria-label': explicitLabel,
}: NumberStepperProps) {
  const inputId = useId();
  // Numeric parents cannot represent an empty edit. Keep it local until the
  // user types a number or leaves the field; string parents own their drafts.
  const [empty, setEmpty] = useState(false);
  useEffect(() => { setEmpty(false); }, [value, disabled]);
  const [fieldLabel, setFieldLabel] = useState(explicitLabel ?? 'value');
  useEffect(() => {
    const labels = Array.from(inputRef.current?.labels ?? []).map(label => label.textContent?.trim()).filter(Boolean);
    setFieldLabel(explicitLabel ?? (labels.join(' ') || 'value'));
  }, [explicitLabel]);
  const inputRef = useRef<HTMLInputElement>(null);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const numStep = typeof step === 'string' ? parseFloat(step) || 1 : step;

  const doStep = useCallback(
    (direction: 1 | -1) => {
      const input = inputRef.current;
      if (!input || disabled) return;
      const cur = input.value === '' && typeof value === 'number'
        ? value
        : parseFloat(input.value) || 0;
      let next = cur + numStep * direction;
      if (min !== undefined) next = Math.max(min, next);
      if (max !== undefined) next = Math.min(max, next);
      // Round to avoid floating point drift
      const decimals = numStep < 1 ? String(numStep).split('.')[1]?.length ?? 0 : 0;
      next = parseFloat(next.toFixed(Math.max(decimals, 2)));
      // Fire a synthetic change event so the parent's onChange handler fires
      const nativeInputValueSetter = Object.getOwnPropertyDescriptor(
        window.HTMLInputElement.prototype,
        'value',
      )?.set;
      nativeInputValueSetter?.call(input, String(next));
      input.dispatchEvent(new Event('input', { bubbles: true }));
    },
    [disabled, min, max, numStep, value],
  );

  const stopRepeat = useCallback(() => {
    if (timeoutRef.current) { clearTimeout(timeoutRef.current); timeoutRef.current = null; }
    if (intervalRef.current) { clearInterval(intervalRef.current); intervalRef.current = null; }
  }, []);

  const startRepeat = useCallback(
    (direction: 1 | -1) => {
      doStep(direction);
      // After a 400ms hold, repeat every 80ms
      timeoutRef.current = setTimeout(() => {
        intervalRef.current = setInterval(() => doStep(direction), 80);
      }, 400);
    },
    [doStep],
  );

  // Make sure the repeat timer can't outlive the component
  useEffect(() => () => stopRepeat(), [stopRepeat]);

  const numericValue = typeof value === 'number' ? value : parseFloat(value);
  const atMin = min !== undefined && Number.isFinite(numericValue) && numericValue <= min;
  const atMax = max !== undefined && Number.isFinite(numericValue) && numericValue >= max;
  const enabledButtonClass =
    'text-bb-text-muted/70 hover:text-bb-text hover:bg-bb-surface active:bg-bb-accent/20';
  const disabledButtonClass = 'text-bb-text-disabled cursor-default';

  return (
    <div className={`inline-flex items-stretch relative ${containerClassName}`}>
      <input
        ref={inputRef}
        id={inputId}
        aria-label={explicitLabel ?? fieldLabel}
        type="number"
        value={empty && typeof value === 'number' ? '' : value}
        onChange={(event) => {
          if (typeof value === 'number' && (
            event.target.value === '' || !Number.isFinite(event.target.valueAsNumber)
          )) {
            if (!empty) setEmpty(true);
            return;
          }
          if (empty) setEmpty(false);
          onChange(event);
        }}
        onBlur={(event) => {
          if (empty) setEmpty(false);
          onBlur?.(event);
        }}
        onKeyDown={(event) => {
          onKeyDown?.(event);
          if (empty && !event.defaultPrevented && !event.nativeEvent.isComposing && event.key === 'Enter') {
            setEmpty(false);
          }
        }}
        min={min}
        max={max}
        step={step}
        disabled={disabled}
        placeholder={placeholder}
        data-testid={testId}
        className={`${className} pr-5`}
      />
      {!disabled && (
        <div className="absolute right-0 top-0 bottom-0 flex flex-col w-4 border-l border-bb-border">
          <button
            type="button"
            aria-label={`Increase ${fieldLabel}`}
            aria-controls={inputId}
            onClick={(event) => { if (event.detail === 0) doStep(1); }}
            disabled={atMax}
            onPointerDown={(e) => { e.preventDefault(); if (!atMax) startRepeat(1); }}
            onPointerUp={stopRepeat}
            onPointerLeave={stopRepeat}
            onPointerCancel={stopRepeat}
            className={`flex-1 flex items-center justify-center rounded-tr ${atMax ? disabledButtonClass : enabledButtonClass}`}
          >
            <ChevronUp size={10} strokeWidth={2.5} />
          </button>
          <button
            type="button"
            aria-label={`Decrease ${fieldLabel}`}
            aria-controls={inputId}
            onClick={(event) => { if (event.detail === 0) doStep(-1); }}
            disabled={atMin}
            onPointerDown={(e) => { e.preventDefault(); if (!atMin) startRepeat(-1); }}
            onPointerUp={stopRepeat}
            onPointerLeave={stopRepeat}
            onPointerCancel={stopRepeat}
            className={`flex-1 flex items-center justify-center rounded-br ${atMin ? disabledButtonClass : enabledButtonClass}`}
          >
            <ChevronDown size={10} strokeWidth={2.5} />
          </button>
        </div>
      )}
    </div>
  );
}
