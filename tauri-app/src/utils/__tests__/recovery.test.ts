import { describe, expect, it, vi } from 'vitest';
import { discardRecoveryBatch } from '../recovery';

describe('discardRecoveryBatch', () => {
  it('keeps track of failed recoveries instead of hiding them all', async () => {
    const discardRecovery = vi.fn(async (path: string) => {
      if (path === '/tmp/two') {
        throw new Error('delete failed');
      }
    });

    const result = await discardRecoveryBatch(
      [
        { path: '/tmp/one', project_name: 'One', saved_at: '2026-04-01T00:00:00Z' },
        { path: '/tmp/two', project_name: 'Two', saved_at: '2026-04-01T00:00:00Z' },
      ],
      discardRecovery,
    );

    expect([...result.discardedPaths]).toEqual(['/tmp/one']);
    expect(result.failedProjectNames).toEqual(['Two']);
  });
});
