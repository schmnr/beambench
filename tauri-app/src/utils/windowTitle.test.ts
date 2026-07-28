import { describe, expect, it } from 'vitest';
import { projectWindowTitle } from './windowTitle';

describe('projectWindowTitle', () => {
  it('shows the saved filename for POSIX and Windows paths', () => {
    expect(projectWindowTitle('/Users/test/cut.lzrproj', 'Ignored', false))
      .toBe('cut.lzrproj - Beam Bench');
    expect(projectWindowTitle('C:\\Jobs\\engrave.lzrproj', 'Ignored', false))
      .toBe('engrave.lzrproj - Beam Bench');
  });

  it('uses the project name before the project has a file path', () => {
    expect(projectWindowTitle(null, 'Untitled Project', false))
      .toBe('Untitled Project - Beam Bench');
  });

  it('marks unsaved work and falls back to the application name', () => {
    expect(projectWindowTitle(null, 'Wallet', true)).toBe('Wallet * - Beam Bench');
    expect(projectWindowTitle(null, null, false)).toBe('Beam Bench');
  });
});
