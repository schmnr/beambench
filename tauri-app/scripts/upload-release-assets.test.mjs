import assert from 'node:assert/strict';
import test from 'node:test';

import { assetNameCandidates, selectRelease } from './upload-release-assets.mjs';

test('selectRelease reuses the only matching draft', () => {
  const draft = { id: 12, tag_name: 'v0.1.9', draft: true };
  assert.equal(
    selectRelease(
      [
        { id: 11, tag_name: 'v0.1.8', draft: false },
        draft,
      ],
      'v0.1.9',
    ),
    draft,
  );
});

test('selectRelease reuses a published release', () => {
  const published = { id: 12, tag_name: 'v0.1.9', draft: false };
  assert.equal(selectRelease([published], 'v0.1.9'), published);
});

test('selectRelease returns null when the tag is absent', () => {
  assert.equal(selectRelease([{ id: 11, tag_name: 'v0.1.8', draft: false }], 'v0.1.9'), null);
});

test('selectRelease fails closed for duplicate same-tag releases', () => {
  assert.throws(
    () =>
      selectRelease(
        [
          { id: 11, tag_name: 'v0.1.9', draft: true },
          { id: 12, tag_name: 'v0.1.9', draft: true },
        ],
        'v0.1.9',
      ),
    /Multiple GitHub Releases/,
  );
});

test('assetNameCandidates includes GitHub\'s space-sanitized name', () => {
  assert.deepEqual(assetNameCandidates('Beam Bench_0.1.9_x64.exe'), [
    'Beam Bench_0.1.9_x64.exe',
    'Beam.Bench_0.1.9_x64.exe',
  ]);
  assert.deepEqual(assetNameCandidates('Beam.Bench.app.tar.gz'), ['Beam.Bench.app.tar.gz']);
});
