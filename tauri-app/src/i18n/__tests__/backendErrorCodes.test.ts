// @vitest-environment node
import { describe, expect, it } from 'vitest';
import { readFileSync, readdirSync, statSync } from 'node:fs';

const repoRoot = new URL('../../../../', import.meta.url);

/** Tags in Rust strings that are not user-facing errors. */
const NOT_USER_ERRORS: Record<string, string> = {
  redacted: 'placeholder for hidden values in debug output',
  migrate_mixed_layers: 'log line prefix',
};

function rustFiles(dir: URL): URL[] {
  return readdirSync(dir).flatMap((name) => {
    if (name === 'target' || name === 'node_modules') return [];
    const path = new URL(name, dir);
    if (statSync(path).isDirectory()) return rustFiles(new URL(`${name}/`, dir));
    return name.endsWith('.rs') ? [path] : [];
  });
}

describe('backend error codes', () => {
  it('every tagged backend error has a translation path', () => {
    const sources = [new URL('crates/', repoRoot), new URL('tauri-app/src-tauri/src/', repoRoot)]
      .flatMap(rustFiles)
      .map((file) => readFileSync(file, 'utf-8'));
    const codes = new Set(sources.flatMap((source) => [...source.matchAll(/"\[([a-z_]+)\]/g)].map((m) => m[1])));
    const errorsSource = readFileSync(new URL('../errors.ts', import.meta.url), 'utf-8');
    const unhandled = [...codes].filter((code) => !(code in NOT_USER_ERRORS) && !errorsSource.includes(code));
    expect(codes.size).toBeGreaterThan(10);
    expect(unhandled).toEqual([]);
  });
});
