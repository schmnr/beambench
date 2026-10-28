#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Inspect the actual PE import tables, not strings that may occur in debug data.
// Windows 10/11 provide the UCRT API sets; the separately installed VC runtimes
// below must not be needed by either standalone Beam Bench executable.
export function importedDlls(bytes) {
  if (bytes.toString('ascii', 0, 2) !== 'MZ') throw new Error('Not a PE executable');
  const pe = bytes.readUInt32LE(0x3c);
  if (bytes.toString('ascii', pe, pe + 4) !== 'PE\0\0' || bytes.readUInt16LE(pe + 4) !== 0x8664) {
    throw new Error('Expected an x86_64 PE executable');
  }
  const optional = pe + 24;
  if (bytes.readUInt16LE(optional) !== 0x20b) throw new Error('Expected a PE32+ header');
  const sections = optional + bytes.readUInt16LE(pe + 20);
  const count = bytes.readUInt16LE(pe + 6);
  const offset = (rva, size) => {
    for (let i = 0; i < count; i++) {
      const section = sections + i * 40;
      const start = bytes.readUInt32LE(section + 12);
      const length = bytes.readUInt32LE(section + 16);
      if (rva >= start && rva - start + size <= length) {
        const position = bytes.readUInt32LE(section + 20) + rva - start;
        if (position + size <= bytes.length) return position;
      }
    }
    throw new Error(`Unmapped PE import address 0x${rva.toString(16)}`);
  };
  const dlls = new Set();
  const directoryCount = bytes.readUInt32LE(optional + 108);
  for (const [index, width, nameField] of [[1, 20, 12], [13, 32, 4]]) {
    if (directoryCount <= index) continue;
    const directory = optional + 112 + index * 8;
    const address = bytes.readUInt32LE(directory);
    const size = bytes.readUInt32LE(directory + 4);
    if (address === 0 && size === 0) continue;
    let terminated = false;
    for (let i = 0; i + width <= size; i += width) {
      const entry = offset(address + i, width);
      if (bytes.subarray(entry, entry + width).every(value => value === 0)) {
        terminated = true;
        break;
      }
      if (index === 13 && bytes.readUInt32LE(entry) !== 1) {
        throw new Error('Unsupported PE delay-import address mode');
      }
      const name = offset(bytes.readUInt32LE(entry + nameField), 1);
      const end = bytes.indexOf(0, name);
      if (end < name || end - name > 255) throw new Error('Invalid PE import name');
      dlls.add(bytes.toString('ascii', name, end).toLowerCase());
    }
    if (!terminated) throw new Error('Unterminated PE import table');
  }
  return [...dlls].sort();
}

export function checkWindowsRuntime(binary) {
  const dlls = importedDlls(readFileSync(binary));
  const runtime = dlls.filter(name => /^(msvcp|msvcr|vcruntime|concrt|vcomp)\d.*\.dll$/i.test(name));
  if (runtime.length) throw new Error(`${binary} requires separately installed Visual C++ runtime DLLs: ${runtime.join(', ')}`);
  if (!dlls.length) throw new Error(`${binary} has no imports; refusing an inconclusive runtime audit`);
  return dlls;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (process.argv.length < 3) throw new Error('Usage: node scripts/check-windows-runtime.mjs <executable> [...]');
    for (const binary of process.argv.slice(2)) {
      checkWindowsRuntime(binary);
      console.log(`No external Visual C++ runtime imports: ${binary}`);
    }
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
