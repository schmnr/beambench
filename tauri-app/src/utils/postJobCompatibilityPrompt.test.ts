import { describe, expect, it } from 'vitest';
import {
  postJobPromptFingerprint,
  postJobPromptNotNowSnoozeMs,
  recordPostJobPromptOutcome,
  shouldShowPostJobPrompt,
} from './postJobCompatibilityPrompt';

function memoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() { return values.size; },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => { values.delete(key); },
    setItem: (key, value) => { values.set(key, value); },
  };
}

describe('post-job compatibility prompt rate limiting', () => {
  it('requires a machine profile and includes the controller in the local fingerprint', () => {
    expect(postJobPromptFingerprint(null, 'grbl')).toBeNull();
    expect(postJobPromptFingerprint('profile-1', 'GRBL')).toBe('profile-1:grbl');
  });

  it('does not repeat after the user chooses a completed or problem outcome', () => {
    const storage = memoryStorage();
    const fingerprint = 'profile-1:grbl';
    expect(shouldShowPostJobPrompt(fingerprint, storage)).toBe(true);

    recordPostJobPromptOutcome(fingerprint, 'completed', storage);
    expect(shouldShowPostJobPrompt(fingerprint, storage)).toBe(false);
  });

  it('snoozes Not now for fourteen days', () => {
    const storage = memoryStorage();
    const fingerprint = 'profile-1:grbl';
    const now = new Date('2026-07-01T00:00:00Z');
    recordPostJobPromptOutcome(fingerprint, 'not_now', storage, now);

    expect(shouldShowPostJobPrompt(
      fingerprint,
      storage,
      now.getTime() + postJobPromptNotNowSnoozeMs - 1,
    )).toBe(false);
    expect(shouldShowPostJobPrompt(
      fingerprint,
      storage,
      now.getTime() + postJobPromptNotNowSnoozeMs,
    )).toBe(true);
  });
});
