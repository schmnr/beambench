import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { checkWindowsRuntime, importedDlls } from './check-windows-runtime.mjs';

function pe(imports = [], delayed = []) {
  const bytes = Buffer.alloc(0x1000);
  bytes.write('MZ');
  bytes.writeUInt32LE(0x80, 0x3c);
  bytes.write('PE\0\0', 0x80);
  bytes.writeUInt16LE(0x8664, 0x84);
  bytes.writeUInt16LE(1, 0x86);
  bytes.writeUInt16LE(240, 0x94);
  bytes.writeUInt16LE(0x20b, 0x98);
  bytes.writeUInt32LE(16, 0x98 + 108);
  const section = 0x98 + 240;
  bytes.writeUInt32LE(0x1000, section + 12);
  bytes.writeUInt32LE(0xe00, section + 16);
  bytes.writeUInt32LE(0x200, section + 20);
  for (const [names, index, start, width, field] of [
    [imports, 1, 0x200, 20, 12],
    [delayed, 13, 0x300, 32, 4],
  ]) {
    if (!names.length) continue;
    const directory = 0x98 + 112 + index * 8;
    bytes.writeUInt32LE(start + 0xe00, directory);
    bytes.writeUInt32LE((names.length + 1) * width, directory + 4);
    for (const [i, name] of names.entries()) {
      const nameOffset = start + 0x300 + i * 64;
      if (index === 13) bytes.writeUInt32LE(1, start + i * width);
      bytes.writeUInt32LE(nameOffset + 0xe00, start + i * width + field);
      bytes.write(name, nameOffset);
    }
  }
  return bytes;
}

test('reads ordinary and delayed imports, ignoring runtime names in unrelated data', () => {
  const bytes = pe(['KERNEL32.dll'], ['USER32.dll']);
  bytes.write('MSVCP140.dll', 0x900);
  assert.deepEqual(importedDlls(bytes), ['kernel32.dll', 'user32.dll']);
});

test('rejects every separately installed runtime, including delay-loaded variants', t => {
  const directory = mkdtempSync(join(tmpdir(), 'Beam Bench PE audit '));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const binary = join(directory, 'test.exe');
  for (const name of ['MSVCP140.dll', 'VCRUNTIME140.dll', 'vcruntime140_1.dll', 'msvcp140_atomic_wait.dll', 'concrt140.dll', 'vcomp140.dll', 'msvcr120.dll']) {
    for (const delayed of [false, true]) {
      writeFileSync(binary, delayed ? pe(['kernel32.dll'], [name]) : pe([name]));
      assert.throws(() => checkWindowsRuntime(binary), /requires separately installed Visual C\+\+ runtime DLLs/);
    }
  }
  writeFileSync(binary, pe(['kernel32.dll', 'api-ms-win-crt-runtime-l1-1-0.dll', 'msvcp_win.dll']));
  assert.equal(checkWindowsRuntime(binary).length, 3);
  writeFileSync(binary, pe());
  assert.throws(() => checkWindowsRuntime(binary), /inconclusive/);
});

test('fails closed for truncated files, wrong architecture and malformed import tables', () => {
  assert.throws(() => importedDlls(Buffer.from('MZ')));
  const wrongArchitecture = pe(['kernel32.dll']);
  wrongArchitecture.writeUInt16LE(0x14c, 0x84);
  assert.throws(() => importedDlls(wrongArchitecture), /x86_64/);
  const unmapped = pe(['kernel32.dll']);
  unmapped.writeUInt32LE(0x5000, 0x200 + 12);
  assert.throws(() => importedDlls(unmapped), /Unmapped/);
  const unterminated = pe(['kernel32.dll']);
  unterminated.writeUInt32LE(20, 0x98 + 112 + 8 + 4);
  assert.throws(() => importedDlls(unterminated), /Unterminated/);
});
