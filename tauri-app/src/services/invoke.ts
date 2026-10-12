import { invoke as tauriInvoke, type InvokeArgs, type InvokeOptions } from '@tauri-apps/api/core';
import type { FeedbackSourceContext } from '../types/feedback';

const failures: { command: string; message: string; at: number }[] = [];

/** Preserve the failed command name without retaining command arguments. */
export async function invoke<T>(command: string, ...args: [InvokeArgs?, InvokeOptions?]): Promise<T> {
  try {
    return await tauriInvoke<T>(command, ...args);
  } catch (error) {
    failures.push({ command, message: String(error), at: Date.now() });
    if (failures.length > 32) failures.shift();
    throw error;
  }
}

export function invocationFailureContext(
  message: string,
  localize: (detail: string) => string,
): Partial<FeedbackSourceContext> | undefined {
  const failure = [...failures].reverse().find((entry) => {
    const age = Date.now() - entry.at;
    return age >= 0 && age <= 30_000 &&
      (message === entry.message || message === localize(entry.message));
  });
  if (!failure) return undefined;
  return {
    action: failure.command,
    correlation_ts: new Date(failure.at).toISOString(),
    error_details: { backend_error: failure.message },
  };
}
