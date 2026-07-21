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
});
