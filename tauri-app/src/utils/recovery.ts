import type { RecoveryInfo } from '../services/persistenceService';

export async function discardRecoveryBatch(
  recoveries: RecoveryInfo[],
  discardRecovery: (path: string) => Promise<void>,
): Promise<{ discardedPaths: Set<string>; failedProjectNames: string[] }> {
  const discardedPaths = new Set<string>();
  const failedProjectNames: string[] = [];

  for (const recovery of recoveries) {
    try {
      await discardRecovery(recovery.path);
      discardedPaths.add(recovery.path);
    } catch {
      failedProjectNames.push(recovery.project_name);
    }
  }

  return { discardedPaths, failedProjectNames };
}
