// @vitest-environment node
import { describe, expect, it } from 'vitest';

/**
 * Notifications reach users directly, so their text must come from the
 * locale files. JSX text is covered by extraction-coverage; this catches
 * English passed straight to a notification call.
 */
const sources = import.meta.glob<string>(['../../**/*.ts', '../../**/*.tsx'], {
  eager: true,
  query: '?raw',
  import: 'default',
});

const NOTIFICATION_LITERAL =
  /\b(?:notifySuccess|notifyError|notifyWarning)\(\s*['"`][A-Za-z]|\.push\(\s*['"`][A-Z][a-z]+ [a-z][^'"`]*['"`]\s*,\s*['"](?:success|error|warning|info)['"]/g;

describe('notification-literal-guard', () => {
  it('passes no hardcoded English to notifications', () => {
    const offenders: string[] = [];
    for (const [path, source] of Object.entries(sources)) {
      if (/__tests__|\.test\.|test-utils/.test(path)) continue;
      for (const match of source.matchAll(NOTIFICATION_LITERAL)) {
        const line = source.slice(0, match.index).split('\n').length;
        offenders.push(`${path.replace('../../', 'src/')}:${line}`);
      }
    }
    expect(offenders).toEqual([]);
  });
});
