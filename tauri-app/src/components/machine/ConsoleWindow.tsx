import React from 'react';
import { useTranslation } from 'react-i18next';
import { useConsoleStore } from '../../stores/consoleStore';

export function ConsoleWindow(): React.ReactElement {
  const { t } = useTranslation();
  const { entries, sendCommand, refreshLog, clearLog, historyUp, historyDown } =
    useConsoleStore();
  const logRef = React.useRef<HTMLDivElement>(null);
  // Smart autoscroll: only follow new entries while the user is already at
  // (or within ~8px of) the bottom; preserve their position otherwise.
  const stickToBottomRef = React.useRef(true);
  const [value, setValue] = React.useState('');

  React.useEffect(() => {
    void refreshLog();
    const interval = window.setInterval(() => {
      void refreshLog();
    }, 500);
    return () => window.clearInterval(interval);
  }, [refreshLog]);

  React.useEffect(() => {
    const el = logRef.current;
    if (el && stickToBottomRef.current) el.scrollTop = el.scrollHeight;
  }, [entries]);

  function handleScroll(): void {
    const el = logRef.current;
    if (!el) return;
    stickToBottomRef.current = el.scrollHeight - el.scrollTop - el.clientHeight <= 8;
  }

  async function handleSubmit() {
    const sent = await sendCommand(value);
    if (sent) {
      setValue('');
    }
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLInputElement>): void {
    if (e.key === 'Enter') {
      void handleSubmit();
    } else if (e.key === 'ArrowUp') {
      const prev = historyUp();
      if (prev) setValue(prev);
    } else if (e.key === 'ArrowDown') {
      const next = historyDown();
      setValue(next);
    }
  }

  return (
    <div className="px-2 pb-2 flex flex-col gap-1">
      <div
        ref={logRef}
        onScroll={handleScroll}
        data-testid="console-log"
        className="h-40 overflow-y-auto bg-bb-bg border border-bb-border rounded p-1 text-xs font-mono space-y-0.5"
      >
        {(entries ?? []).map((entry, i) => (
          <div
            key={i}
            // backend ConsoleEntry has no is_error field — derive
            // error styling from the payload content (GRBL error responses
            // start with "error:" / "ALARM:").
            className={
              /^(error|alarm)[:\s]/i.test(entry.content) ? 'text-bb-error-fg' : 'text-bb-text'
            }
          >
            <span className="text-bb-text-muted">{entry.timestamp}</span>{' '}
            <span>{entry.direction === 'sent' ? '→' : '←'}</span>{' '}
            {entry.content}
          </div>
        ))}
      </div>
      <div className="flex gap-1">
        <input
          placeholder={t('panels.machine.console.gcode_placeholder')}
          className="flex-1 px-1.5 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text font-mono focus:outline-none focus:border-bb-accent"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={handleKeyDown}
        />
        <button
          className="px-2 py-0.5 text-xs bg-bb-accent text-bb-on-accent rounded hover:bg-bb-accent-hover"
          onClick={() => { void handleSubmit(); }}
        >
          {t('panels.machine.console.send')}
        </button>
        <button
          className="px-2 py-0.5 text-xs bg-bb-bg border border-bb-border rounded hover:bg-bb-hover text-bb-text"
          onClick={() => { void clearLog(); }}
        >
          {t('panels.machine.console.clear')}
        </button>
      </div>
    </div>
  );
}
