import { describe, expect, it } from 'vitest';
import { isMacPlatform } from './platform';

describe('platform helpers', () => {
  it('detects macOS from navigator.platform', () => {
    expect(isMacPlatform({ platform: 'MacIntel', userAgent: '' })).toBe(true);
  });

  it('detects macOS from user agent fallback', () => {
    expect(isMacPlatform({ platform: 'Unknown', userAgent: 'Mozilla/5.0 (Mac OS X 14_0)' })).toBe(true);
  });

  it('does not mark Windows as macOS', () => {
    expect(isMacPlatform({ platform: 'Win32', userAgent: 'Windows NT 10.0' })).toBe(false);
  });
});
