const STORAGE_KEY = 'beambench.post-job-compatibility.v1';
const NOT_NOW_SNOOZE_MS = 14 * 24 * 60 * 60 * 1000;

export type PostJobPromptOutcome = 'completed' | 'problem' | 'not_now';

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
  now = Date.now(),
): boolean {
  if (!fingerprint) return false;
  const record = readRecords(storage)[fingerprint];
  if (!record) return true;
  if (record.outcome !== 'not_now') return false;
  const recordedAt = Date.parse(record.recorded_at);
  return !Number.isFinite(recordedAt) || now - recordedAt >= NOT_NOW_SNOOZE_MS;
}

export function recordPostJobPromptOutcome(
  fingerprint: string,
  outcome: PostJobPromptOutcome,
  storage: Storage,
  now = new Date(),
): void {
  const records = readRecords(storage);
  records[fingerprint] = { outcome, recorded_at: now.toISOString() };
  try {
    storage.setItem(STORAGE_KEY, JSON.stringify(records));
  } catch {
    // A blocked/full local store should never interfere with running a job.
  }
}

export const postJobPromptStorageKey = STORAGE_KEY;
export const postJobPromptNotNowSnoozeMs = NOT_NOW_SNOOZE_MS;
