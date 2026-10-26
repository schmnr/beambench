import { describe, it, expect } from 'vitest';
import { shouldShowWelcome } from '../welcomeStore';
import type { AppSettings } from '../../types/commands';

describe('shouldShowWelcome', () => {
  it('shows on every startup once settings have loaded', () => {
    expect(shouldShowWelcome({} as unknown as AppSettings)).toBe(true);
  });

  it('does not show before settings have loaded', () => {
    expect(shouldShowWelcome(null)).toBe(false);
    expect(shouldShowWelcome(undefined)).toBe(false);
  });
  it('hides the startup ad for an active paid period on either connected service', () => {
    const until = new Date(Date.now() + 3_600_000).toISOString();
    expect(shouldShowWelcome({} as AppSettings, until, null)).toBe(false);
    expect(shouldShowWelcome({} as AppSettings, null, until)).toBe(false);
  });
  it('shows it for free accounts, pack buyers and expired or malformed paid periods', () => {
    const expired = new Date(Date.now() - 3_600_000).toISOString();
    expect(shouldShowWelcome({} as AppSettings, null, null)).toBe(true);
    expect(shouldShowWelcome({} as AppSettings, expired, 'invalid-date')).toBe(true);
  });
});
