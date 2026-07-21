import { expect, test } from 'vitest';

import { assetNameCandidates, selectRelease } from './upload-release-assets.mjs';

test('selectRelease reuses the only matching draft', () => {
  const draft = { id: 12, tag_name: 'v0.1.9', draft: true };
  expect(selectRelease([{ id: 11, tag_name: 'v0.1.8', draft: false }, draft], 'v0.1.9')).toBe(
    draft,
  );
});

test('selectRelease reuses a published release', () => {
  const published = { id: 12, tag_name: 'v0.1.9', draft: false };
  expect(selectRelease([published], 'v0.1.9')).toBe(published);
});

test('selectRelease returns null when the tag is absent', () => {
  expect(selectRelease([{ id: 11, tag_name: 'v0.1.8', draft: false }], 'v0.1.9')).toBeNull();
});

test('selectRelease fails closed for duplicate same-tag releases', () => {
  expect(() =>
    selectRelease(
      [
        { id: 11, tag_name: 'v0.1.9', draft: true },
        { id: 12, tag_name: 'v0.1.9', draft: true },
      ],
      'v0.1.9',
    ),
  ).toThrow(/Multiple GitHub Releases/);
});

test("assetNameCandidates includes GitHub's space-sanitized name", () => {
  expect(assetNameCandidates('Beam Bench_0.1.9_x64.exe')).toEqual([
    'Beam Bench_0.1.9_x64.exe',
    'Beam.Bench_0.1.9_x64.exe',
  ]);
  expect(assetNameCandidates('Beam.Bench.app.tar.gz')).toEqual(['Beam.Bench.app.tar.gz']);
});
