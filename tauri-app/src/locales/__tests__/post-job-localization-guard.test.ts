import { describe, expect, it } from 'vitest';

const locales = (
  import.meta as ImportMeta & {
    glob?: (p: string, o: { eager: true; import: 'default' }) => Record<string, unknown>;
  }
).glob!('../*.json', { eager: true, import: 'default' });

const POST_JOB_COPY_KEYS = [
  'dialog.feedback.title_job_compatibility',
  'dialog.feedback.field_optional_comment',
  'dialog.feedback.job_compatibility_help',
  'feedback.post_job_title',
  'feedback.post_job_question',
  'feedback.post_job_help',
  'feedback.post_job_profile_help',
  'feedback.post_job_privacy',
  'feedback.post_job_not_now',
  'feedback.post_job_problem',
  'feedback.post_job_completed',
  'feedback.post_job_success_report_title',
  'feedback.post_job_problem_report_title',
];

function getString(bundle: unknown, path: string): string | undefined {
  const value = path.split('.').reduce<unknown>(
    (current, key) => (
      current && typeof current === 'object'
        ? (current as Record<string, unknown>)[key]
        : undefined
    ),
    bundle,
  );
  return typeof value === 'string' ? value : undefined;
}

describe('post-job feedback localization guard', () => {
  const english = locales['../en.json'];

  for (const [path, bundle] of Object.entries(locales)) {
    if (path === '../en.json') continue;
    const code = path.match(/\.\.\/(.+)\.json$/)?.[1] ?? path;

    it(`${code}: post-job feedback copy is localized`, () => {
      const copiedEnglish = POST_JOB_COPY_KEYS.filter(
        (key) => getString(bundle, key) === getString(english, key),
      );
      expect(copiedEnglish, `English post-job feedback copy left in ${code}`).toEqual([]);
    });
  }
});
