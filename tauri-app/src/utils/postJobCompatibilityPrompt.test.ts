import { describe, expect, it } from 'vitest';
import {
  postJobPromptFingerprint,
  recordPostJobPromptOutcome,
  shouldOfferPostJobCompatibility,
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

  it('does not repeat after the one-time notification has been offered', () => {
    const storage = memoryStorage();
    const fingerprint = 'profile-1:ruida';
    expect(shouldShowPostJobPrompt(fingerprint, storage)).toBe(true);

    recordPostJobPromptOutcome(fingerprint, 'offered', storage);
    expect(shouldShowPostJobPrompt(fingerprint, storage)).toBe(false);
  });

  it('treats legacy Not now records as permanently dismissed', () => {
    const storage = memoryStorage();
    const fingerprint = 'profile-1:ruida';
    storage.setItem(
      'beambench.post-job-compatibility.v1',
      JSON.stringify({
        [fingerprint]: { outcome: 'not_now', recorded_at: '2026-07-01T00:00:00Z' },
      }),
    );

    expect(shouldShowPostJobPrompt(fingerprint, storage)).toBe(false);
  });

  it('reports unavailable storage so callers can avoid repeat notifications', () => {
    const storage = memoryStorage();
    storage.setItem = () => { throw new Error('storage disabled'); };

    expect(recordPostJobPromptOutcome('profile-1:ruida', 'offered', storage)).toBe(false);
  });

  it('targets beta and experimental non-GRBL-family controllers only', () => {
    expect(shouldOfferPostJobCompatibility({
      controller_driver: 'ruida',
      product_tier: 'experimental',
    })).toBe(true);
    expect(shouldOfferPostJobCompatibility({
      controller_driver: 'marlin',
      product_tier: 'beta',
    })).toBe(true);
    expect(shouldOfferPostJobCompatibility({
      controller_driver: 'grbl',
      product_tier: 'experimental',
    })).toBe(false);
    expect(shouldOfferPostJobCompatibility({
      controller_driver: 'fluid_nc',
      product_tier: 'experimental',
    })).toBe(false);
    expect(shouldOfferPostJobCompatibility({
      controller_driver: 'grbl_hal',
      product_tier: 'beta',
    })).toBe(false);
    expect(shouldOfferPostJobCompatibility({
      controller_driver: 'lihuiyu',
      product_tier: 'supported',
    })).toBe(false);
    expect(shouldOfferPostJobCompatibility({
      controller_driver: null,
      product_tier: null,
    })).toBe(false);
  });
});
