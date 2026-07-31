import type { MachineRuntimeState } from '../types/machine';

const STORAGE_KEY = 'beambench.post-job-compatibility.v1';
const GRBL_FAMILY_DRIVERS = new Set(['grbl', 'fluid_nc', 'grbl_hal']);

export type PostJobPromptOutcome = 'offered' | 'completed';

interface PromptRecord {
  outcome: PostJobPromptOutcome;
  recorded_at: string;
}

type PromptRecords = Record<string, PromptRecord>;

function readRecords(storage: Storage): PromptRecords {
  try {
    const parsed = JSON.parse(storage.getItem(STORAGE_KEY) ?? '{}') as unknown;
    return parsed && typeof parsed === 'object' ? parsed as PromptRecords : {};
  } catch {
    return {};
  }
}

export function postJobPromptFingerprint(
  profileId: string | null,
  controllerKey: string,
): string | null {
  if (!profileId) return null;
  return `${profileId}:${controllerKey.trim().toLowerCase() || 'unknown'}`;
}

export function shouldShowPostJobPrompt(
  fingerprint: string | null,
  storage: Storage,
): boolean {
  if (!fingerprint) return false;
  return readRecords(storage)[fingerprint] === undefined;
}

/**
 * Compatibility outreach is intentionally limited to controller paths where
 * real-hardware evidence is still useful. GRBL-family sessions and unknown
 * legacy runtime responses never create a post-job interruption.
 */
export function shouldOfferPostJobCompatibility(
  runtime: Pick<MachineRuntimeState, 'controller_driver' | 'product_tier'>,
): boolean {
  const targetedTier = runtime.product_tier === 'experimental' || runtime.product_tier === 'beta';
  return targetedTier
    && runtime.controller_driver !== null
    && runtime.controller_driver !== 'unknown'
    && !GRBL_FAMILY_DRIVERS.has(runtime.controller_driver);
}

export function recordPostJobPromptOutcome(
  fingerprint: string,
  outcome: PostJobPromptOutcome,
  storage: Storage,
  now = new Date(),
): boolean {
  const records = readRecords(storage);
  records[fingerprint] = { outcome, recorded_at: now.toISOString() };
  try {
    storage.setItem(STORAGE_KEY, JSON.stringify(records));
    return true;
  } catch {
    // A blocked/full local store should never interfere with running a job.
    return false;
  }
}

export const postJobPromptStorageKey = STORAGE_KEY;
