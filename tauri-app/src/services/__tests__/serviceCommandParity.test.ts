import { readFileSync } from 'node:fs';

import { describe, expect, it } from 'vitest';

function read(relativePath: string): string {
  return readFileSync(new URL(relativePath, import.meta.url), 'utf8');
}

function extractInvokedCommands(source: string): string[] {
  return [...source.matchAll(/invoke(?:<[^>]+>)?\(\s*'([^']+)'/g)].map((match) => match[1]);
}

describe('service command parity', () => {
  it('every vectorService command is registered in the Tauri vector command table', () => {
    const vectorServiceSource = read('../vectorService.ts');
    const mainSource = read('../../../src-tauri/src/main.rs');
    const commands = extractInvokedCommands(vectorServiceSource);

    expect(vectorServiceSource).not.toContain('weld_shapes');
    expect(new Set(commands).size).toBeGreaterThan(40);

    for (const command of commands) {
      expect(mainSource).toContain(`commands::vector::${command}`);
    }
  });

  it('every projectService command is registered in the Tauri project command table', () => {
    const projectServiceSource = read('../projectService.ts');
    const mainSource = read('../../../src-tauri/src/main.rs');
    const commands = extractInvokedCommands(projectServiceSource);

    expect(new Set(commands).size).toBeGreaterThan(40);

    for (const command of commands) {
      expect(mainSource).toMatch(new RegExp(`commands::[a-z_]+::${command}\\b`));
    }
  });

  it('every persistenceService command is registered in the Tauri invoke table', () => {
    const persistenceServiceSource = read('../persistenceService.ts');
    const mainSource = read('../../../src-tauri/src/main.rs');
    const commands = extractInvokedCommands(persistenceServiceSource);
    expect(new Set(commands).size).toBeGreaterThan(10);

    for (const command of commands) {
      expect(mainSource).toMatch(new RegExp(`commands::[a-z_]+::${command}\\b`));
    }
  });

  it('every artLibraryService command is registered in the Tauri art library command table', () => {
    const artLibraryServiceSource = read('../artLibraryService.ts');
    const mainSource = read('../../../src-tauri/src/main.rs');
    const commands = extractInvokedCommands(artLibraryServiceSource);

    expect(new Set(commands).size).toBeGreaterThan(10);

    for (const command of commands) {
      expect(mainSource).toContain(`commands::art_library::${command}`);
    }
  });
});
