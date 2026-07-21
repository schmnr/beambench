import { describe, it, expect } from 'vitest';
import { hotkeyFromKeyboardEvent, matchesHotkey, normalizeHotkey, parseHotkey } from '../hotkeyMatch';

function makeEvent(init: Partial<KeyboardEvent>): KeyboardEvent {
  return {
    key: '',
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    metaKey: false,
    ...init,
  } as KeyboardEvent;
}

describe('parseHotkey ', () => {
  it('parses modifier+key combos case-insensitively', () => {
    expect(parseHotkey('Ctrl+Shift+H')).toEqual({ ctrl: true, shift: true, alt: false, key: 'h' });
    expect(parseHotkey('cmd+1')).toEqual({ ctrl: true, shift: false, alt: false, key: '1' });
    expect(parseHotkey('ALT+F')).toEqual({ ctrl: false, shift: false, alt: true, key: 'f' });
  });

  it('returns null for empty, whitespace, or modifier-only specs', () => {
    expect(parseHotkey('')).toBeNull();
    expect(parseHotkey('   ')).toBeNull();
    expect(parseHotkey('Ctrl')).toBeNull();
    expect(parseHotkey('Ctrl+Shift')).toBeNull();
    expect(parseHotkey(null)).toBeNull();
    expect(parseHotkey(undefined)).toBeNull();
  });

  it('treats cmd/command/meta as ctrl so a single spec works cross-platform', () => {
    expect(parseHotkey('Cmd+S')?.ctrl).toBe(true);
    expect(parseHotkey('Command+S')?.ctrl).toBe(true);
    expect(parseHotkey('Meta+S')?.ctrl).toBe(true);
  });

  it('normalizes aliases to the persisted canonical format', () => {
    expect(normalizeHotkey('Command+Option+Shift+R')).toBe('Ctrl+Shift+Alt+r');
    expect(normalizeHotkey('Ctrl+Esc')).toBe('Ctrl+Escape');
    expect(hotkeyFromKeyboardEvent(makeEvent({ key: 'Q', metaKey: true }))).toBe('Ctrl+q');
  });
});

describe('matchesHotkey ', () => {
  it('matches exact modifier + key', () => {
    const e = makeEvent({ key: 'h', ctrlKey: true, shiftKey: true });
    expect(matchesHotkey('Ctrl+Shift+H', e)).toBe(true);
  });

  it('rejects missing modifiers', () => {
    const e = makeEvent({ key: 'h', ctrlKey: true }); // no shift
    expect(matchesHotkey('Ctrl+Shift+H', e)).toBe(false);
  });

  it('rejects extra modifiers', () => {
    const e = makeEvent({ key: 'h', ctrlKey: true, shiftKey: true, altKey: true });
    expect(matchesHotkey('Ctrl+Shift+H', e)).toBe(false);
  });

  it('treats metaKey as ctrl equivalent (macOS)', () => {
    const e = makeEvent({ key: '1', metaKey: true });
    expect(matchesHotkey('Ctrl+1', e)).toBe(true);
    expect(matchesHotkey('Cmd+1', e)).toBe(true);
  });

  it('is case-insensitive on the key', () => {
    const e = makeEvent({ key: 'H', ctrlKey: true, shiftKey: true });
    expect(matchesHotkey('ctrl+shift+h', e)).toBe(true);
  });

  it('returns false for empty/invalid specs without throwing', () => {
    const e = makeEvent({ key: 'a', ctrlKey: true });
    expect(matchesHotkey('', e)).toBe(false);
    expect(matchesHotkey(null, e)).toBe(false);
    expect(matchesHotkey(undefined, e)).toBe(false);
    expect(matchesHotkey('Ctrl', e)).toBe(false);
  });

  it('does not fire plain typed keys', () => {
    const e = makeEvent({ key: 'h' });
    expect(matchesHotkey('Ctrl+Shift+H', e)).toBe(false);
  });
});
