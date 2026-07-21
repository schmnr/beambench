#!/usr/bin/env node

import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const root = resolve(scriptDir, '..');
const tauriApp = join(root, 'tauri-app');
const output = join(root, 'THIRD_PARTY_LICENSES.md');
const licenseName = /^(licen[cs]e|copying|notice|copyright)([._-].*)?$/i;

function textLicenseFiles(directory) {
  if (!existsSync(directory)) return [];
  return readdirSync(directory, { withFileTypes: true })
    .filter((entry) => entry.isFile() && licenseName.test(entry.name))
    .map((entry) => join(directory, entry.name))
    .filter((path) => {
      const contents = readFileSync(path);
      return contents.length <= 1024 * 1024 && !contents.includes(0);
    })
    .sort();
}

function normalizedText(path) {
  return readFileSync(path, 'utf8').replace(/\r\n/g, '\n').trim();
}

function collectCargo() {
  const metadata = JSON.parse(
    execFileSync('cargo', ['metadata', '--format-version', '1', '--locked'], {
      cwd: root,
      encoding: 'utf8',
      maxBuffer: 64 * 1024 * 1024,
    }),
  );
  return metadata.packages
    .filter((pkg) => pkg.source)
    .map((pkg) => ({
      ecosystem: 'Cargo',
      name: pkg.name,
      version: pkg.version,
      license: pkg.license ?? 'Not declared in package metadata',
      homepage: pkg.repository ?? pkg.homepage ?? '',
      files: textLicenseFiles(dirname(pkg.manifest_path)),
    }));
}

function collectNpm() {
  const lock = JSON.parse(readFileSync(join(tauriApp, 'package-lock.json'), 'utf8'));
  return Object.entries(lock.packages ?? {})
    .filter(([path]) => path.startsWith('node_modules/'))
    .map(([path, pkg]) => ({
      ecosystem: 'npm',
      name: path.replace(/^.*node_modules\//, ''),
      version: pkg.version ?? 'unknown',
      license: pkg.license ?? 'Not declared in package metadata',
      homepage: pkg.homepage ?? pkg.repository?.url ?? '',
      files: textLicenseFiles(join(tauriApp, path)),
    }));
}

const packages = [...collectCargo(), ...collectNpm()].sort((a, b) =>
  `${a.ecosystem}:${a.name}:${a.version}`.localeCompare(`${b.ecosystem}:${b.name}:${b.version}`),
);
const licenseGroups = new Map();

for (const pkg of packages) {
  for (const file of pkg.files) {
    const contents = normalizedText(file);
    if (!contents) continue;
    const digest = createHash('sha256').update(contents).digest('hex');
    const group = licenseGroups.get(digest) ?? { contents, packages: [] };
    group.packages.push(`${pkg.ecosystem}:${pkg.name}@${pkg.version}`);
    licenseGroups.set(digest, group);
  }
}

const lines = [
  '# Third-Party Package Licenses',
  '',
  'This generated report accompanies Beam Bench binary distributions. It was',
  'created from `Cargo.lock`, `tauri-app/package-lock.json`, and the license',
  'files shipped by the resolved packages. Package authors retain their own',
  'copyrights. Regenerate it with `node scripts/generate-license-report.mjs`.',
  '',
  '## Package inventory',
  '',
  '| Ecosystem | Package | Version | Declared license |',
  '| --- | --- | --- | --- |',
];

for (const pkg of packages) {
  const name = pkg.homepage ? `[${pkg.name}](${pkg.homepage})` : pkg.name;
  lines.push(`| ${pkg.ecosystem} | ${name} | ${pkg.version} | ${pkg.license.replaceAll('|', '\\|')} |`);
}

lines.push('', '## License and notice texts', '');
for (const [digest, group] of [...licenseGroups.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
  lines.push(
    `### ${digest.slice(0, 12)}`,
    '',
    `Packages: ${group.packages.sort().join(', ')}`,
    '',
    '```text',
    group.contents.replaceAll('```', '`` `'),
    '```',
    '',
  );
}

writeFileSync(output, `${lines.join('\n')}\n`);
console.log(`Wrote ${output} with ${packages.length} packages and ${licenseGroups.size} distinct notices.`);
