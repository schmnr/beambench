import { describe, expect, it } from 'vitest';
import { wrapBackendError } from './errors';

describe('wrapBackendError', () => {
  it('translates the machine-zero homing gate into a direct instruction', () => {
    expect(
      wrapBackendError('Machine-zero moves require homing in the current session first'),
    ).toBe('Home the machine first to use machine zero.');
    expect(
      wrapBackendError('Error: Machine-zero moves require homing in the current session first'),
    ).toBe('Home the machine first to use machine zero.');
    expect(
      wrapBackendError('Operation failed: Error: Machine-zero moves require homing in the current session first'),
    ).toBe('Home the machine first to use machine zero.');
  });

  it('keeps unknown backend errors wrapped with their original detail', () => {
    expect(wrapBackendError('Unexpected backend detail')).toBe(
      'Operation failed: Unexpected backend detail',
    );
  });
});
