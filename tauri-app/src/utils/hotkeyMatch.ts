// parse a user-entered hotkey spec (e.g. "Ctrl+Shift+H", "Cmd+1")
// and match it against a KeyboardEvent.
//
// Modifier tokens are case-insensitive and treat Ctrl/Cmd as equivalent
// (so a hotkey of "Ctrl+1" fires on macOS Cmd+1 without requiring separate
// specs). The non-modifier token matches `event.key` case-insensitively.
//
// Returns false for empty/missing specs or specs with no non-modifier key.

export interface ParsedHotkey {
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  key: string;
}

const NAMED_KEY_ALIASES: Record<string, string> = {
  enter: 'Enter',
  return: 'Enter',
  escape: 'Escape',
  esc: 'Escape',
  backspace: 'Backspace',
  delete: 'Delete',
  del: 'Delete',
  tab: 'Tab',
  space: 'Space',
  ' ': 'Space',
  arrowup: 'ArrowUp',
  up: 'ArrowUp',
  arrowdown: 'ArrowDown',
  down: 'ArrowDown',
  arrowleft: 'ArrowLeft',
  left: 'ArrowLeft',
  arrowright: 'ArrowRight',
  right: 'ArrowRight',
  pageup: 'PageUp',
  pagedown: 'PageDown',
  home: 'Home',
  end: 'End',
  period: '.',
  comma: ',',
};

export function normalizeHotkeyKey(key: string): string {
  const trimmed = key.trim();
  if (!trimmed) return '';
  const lower = trimmed.toLowerCase();
  if (NAMED_KEY_ALIASES[lower]) return NAMED_KEY_ALIASES[lower];
  if (/^f([1-9]|1[0-9]|2[0-4])$/i.test(trimmed)) return trimmed.toUpperCase();
  if (trimmed.length === 1) return trimmed.toLowerCase();
  return trimmed;
}

export function parseHotkey(spec: string | null | undefined): ParsedHotkey | null {
  if (!spec) return null;
  const tokens = spec
    .split('+')
    .map((t) => t.trim())
    .filter(Boolean);
  if (tokens.length === 0) return null;

  let ctrl = false;
  let shift = false;
  let alt = false;
  let key = '';
  for (const t of tokens) {
    switch (t.toLowerCase()) {
      case 'ctrl':
      case 'control':
      case 'cmd':
      case 'command':
      case 'meta':
        ctrl = true;
        break;
      case 'shift':
        shift = true;
        break;
      case 'alt':
      case 'option':
      case 'opt':
        alt = true;
        break;
      default:
        key = normalizeHotkeyKey(t);
    }
  }
  if (!key) return null;
  return { ctrl, shift, alt, key };
}

export function normalizeHotkey(spec: string | null | undefined): string | null {
  const parsed = parseHotkey(spec);
  if (!parsed) return null;

  const tokens: string[] = [];
  if (parsed.ctrl) tokens.push('Ctrl');
  if (parsed.shift) tokens.push('Shift');
  if (parsed.alt) tokens.push('Alt');
  tokens.push(parsed.key);
  return tokens.join('+');
}

export function hotkeysConflict(
  specA: string | null | undefined,
  specB: string | null | undefined,
): boolean {
  const normalizedA = normalizeHotkey(specA);
  const normalizedB = normalizeHotkey(specB);
  if (!normalizedA || !normalizedB) return false;
  return normalizedA === normalizedB;
}

export function matchesHotkey(spec: string | null | undefined, e: KeyboardEvent): boolean {
  const parsed = parseHotkey(spec);
  return parsed ? matchesParsedHotkey(parsed, e) : false;
}

export function hotkeyFromKeyboardEvent(e: KeyboardEvent): string | null {
  if (['Control', 'Shift', 'Alt', 'Meta', 'OS'].includes(e.key)) return null;
  const key = normalizeHotkeyKey(e.key);
  if (!key || key === 'Dead' || key === 'Unidentified') return null;
  const tokens: string[] = [];
  if (e.ctrlKey || e.metaKey) tokens.push('Ctrl');
  if (e.shiftKey) tokens.push('Shift');
  if (e.altKey) tokens.push('Alt');
  tokens.push(key);
  return normalizeHotkey(tokens.join('+'));
}

export function matchesParsedHotkey(parsed: ParsedHotkey, e: KeyboardEvent): boolean {
  if (!parsed) return false;
  const ctrl = e.ctrlKey || e.metaKey;
  if (parsed.ctrl !== ctrl) return false;
  if (parsed.shift !== e.shiftKey) return false;
  if (parsed.alt !== e.altKey) return false;
  return normalizeHotkeyKey(e.key) === parsed.key;
}
