#!/usr/bin/env node

import { execFileSync, spawnSync } from 'node:child_process';
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  statSync,
} from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const APP_DIR = resolve(SCRIPT_DIR, '..');
const ROOT_DIR = resolve(APP_DIR, '..');
const DEFAULT_OUTPUT_ROOT = join(ROOT_DIR, 'target', 'release-staging', 'cli');
const LEGAL_FILES = [
  'LICENSE',
  'COPYRIGHT',
  'SOURCE.md',
  'THIRD_PARTY_NOTICES.md',
  'THIRD_PARTY_LICENSES.md',
];

const SUPPORTED_TARGETS = new Set([
  'universal-apple-darwin',
  'x86_64-pc-windows-msvc',
  'x86_64-unknown-linux-gnu',
]);

function usage() {
  console.log(`Usage: node scripts/build-cli-release.mjs --target <target> [options]

Builds and verifies the standalone Beam Bench CLI binary used by release workflows.

Required:
  --target <target>       universal-apple-darwin,
                          x86_64-pc-windows-msvc, or
                          x86_64-unknown-linux-gnu

Options:
  --version <version>     Require the source version to match this release version.
  --out-dir <directory>   Stage beambench-cli here (default: target release staging).
  --verify-only           Verify an already-staged binary without rebuilding it.
  -h, --help              Show this help.
`);
}

function fail(message) {
  console.error(`error: ${message}`);
  process.exit(1);
}

function parseArgs(argv) {
  const args = { target: '', version: '', outDir: '', verifyOnly: false };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '-h' || arg === '--help') {
      usage();
      process.exit(0);
    }
    if (arg === '--verify-only') {
      args.verifyOnly = true;
      continue;
    }
    if (arg === '--target' || arg === '--version' || arg === '--out-dir') {
      const value = argv[index + 1];
      if (!value || value.startsWith('--')) fail(`Missing value for ${arg}`);
      index += 1;
      if (arg === '--target') args.target = value;
      if (arg === '--version') args.version = value;
      if (arg === '--out-dir') args.outDir = value;
      continue;
    }
    fail(`Unknown argument: ${arg}`);
  }
  return args;
}

function workspaceVersion() {
  const cargoToml = readFileSync(join(ROOT_DIR, 'Cargo.toml'), 'utf8');
  const workspacePackage = cargoToml.match(
    /\[workspace\.package\][\s\S]*?(?=\n\[|$)/,
  )?.[0];
  const version = workspacePackage?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  if (!version) fail('Could not read workspace.package.version from Cargo.toml');
  return version;
}

function verifiedSourceVersion(requiredVersion) {
  const cargoVersion = workspaceVersion();
  const packageVersion = JSON.parse(
    readFileSync(join(APP_DIR, 'package.json'), 'utf8'),
  ).version;
  const tauriVersion = JSON.parse(
    readFileSync(join(APP_DIR, 'src-tauri', 'tauri.conf.json'), 'utf8'),
  ).version;

  const versions = new Set([cargoVersion, packageVersion, tauriVersion]);
  if (versions.size !== 1) {
    fail(
      `Source version mismatch: Cargo.toml=${cargoVersion}, package.json=${packageVersion}, tauri.conf.json=${tauriVersion}`,
    );
  }
  if (requiredVersion && requiredVersion !== cargoVersion) {
    fail(`Release version ${requiredVersion} does not match source version ${cargoVersion}`);
  }
  return cargoVersion;
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: ROOT_DIR,
    env: { ...process.env, ...options.env },
    stdio: options.capture ? ['ignore', 'pipe', 'pipe'] : 'inherit',
    encoding: options.capture ? 'utf8' : undefined,
  });
  if (result.error) fail(`Could not run ${command}: ${result.error.message}`);
  if (result.status !== 0) {
    if (options.capture && result.stderr) process.stderr.write(result.stderr);
    fail(`${command} exited with status ${result.status}`);
  }
  return options.capture ? result.stdout.trim() : '';
}

function buildTarget(target, extraEnv = {}) {
  run(
    'cargo',
    ['build', '--locked', '--release', '--package', 'beambench-cli', '--target', target],
    { env: extraEnv },
  );
  const extension = target.includes('windows') ? '.exe' : '';
  const binary = join(ROOT_DIR, 'target', target, 'release', `beambench-cli${extension}`);
  if (!existsSync(binary)) fail(`Cargo did not produce ${binary}`);
  return binary;
}

function stageFile(source, destination) {
  mkdirSync(dirname(destination), { recursive: true });
  const temporary = `${destination}.tmp-${process.pid}`;
  rmSync(temporary, { force: true });
  copyFileSync(source, temporary);
  if (process.platform !== 'win32') chmodSync(temporary, 0o755);
  rmSync(destination, { force: true });
  renameSync(temporary, destination);
}

function stageLegalFiles(destinationDirectory) {
  mkdirSync(destinationDirectory, { recursive: true });
  for (const name of LEGAL_FILES) {
    const source = join(ROOT_DIR, name);
    if (!existsSync(source)) fail(`Required legal notice is missing: ${source}`);
    copyFileSync(source, join(destinationDirectory, name));
  }
}

function verifyLegalFiles(destinationDirectory) {
  for (const name of LEGAL_FILES) {
    if (!existsSync(join(destinationDirectory, name))) {
      fail(`CLI package is missing required legal notice: ${name}`);
    }
  }
}

function buildAndStage(target, destination) {
  if (target === 'universal-apple-darwin') {
    if (process.platform !== 'darwin') {
      fail('universal-apple-darwin must be built on macOS with lipo');
    }
    const tauriConfig = JSON.parse(
      readFileSync(join(APP_DIR, 'src-tauri', 'tauri.conf.json'), 'utf8'),
    );
    const minimumSystemVersion = tauriConfig.bundle?.macOS?.minimumSystemVersion;
    if (!minimumSystemVersion) {
      fail('tauri.conf.json is missing bundle.macOS.minimumSystemVersion');
    }
    const env = { MACOSX_DEPLOYMENT_TARGET: minimumSystemVersion };
    const arm64 = buildTarget('aarch64-apple-darwin', env);
    const x86_64 = buildTarget('x86_64-apple-darwin', env);

    mkdirSync(dirname(destination), { recursive: true });
    const temporary = `${destination}.tmp-${process.pid}`;
    rmSync(temporary, { force: true });
    run('lipo', ['-create', arm64, x86_64, '-output', temporary]);
    chmodSync(temporary, 0o755);
    rmSync(destination, { force: true });
    renameSync(temporary, destination);
    return;
  }

  stageFile(buildTarget(target), destination);
}

function verifyPeX86_64(binary) {
  const bytes = readFileSync(binary);
  if (bytes.length < 0x40 || bytes.toString('ascii', 0, 2) !== 'MZ') {
    fail(`${binary} is not a PE executable`);
  }
  const peOffset = bytes.readUInt32LE(0x3c);
  if (peOffset + 6 > bytes.length || bytes.toString('ascii', peOffset, peOffset + 4) !== 'PE\0\0') {
    fail(`${binary} has an invalid PE header`);
  }
  const machine = bytes.readUInt16LE(peOffset + 4);
  if (machine !== 0x8664) {
    fail(`${binary} has PE machine 0x${machine.toString(16)}; expected x86_64 (0x8664)`);
  }
}

function verifyElfX86_64(binary) {
  const bytes = readFileSync(binary);
  if (
    bytes.length < 20 ||
    bytes[0] !== 0x7f ||
    bytes.toString('ascii', 1, 4) !== 'ELF'
  ) {
    fail(`${binary} is not an ELF executable`);
  }
  if (bytes[4] !== 2 || bytes[5] !== 1) {
    fail(`${binary} is not a 64-bit little-endian ELF executable`);
  }
  const machine = bytes.readUInt16LE(18);
  if (machine !== 62) {
    fail(`${binary} has ELF machine ${machine}; expected x86_64 (62)`);
  }
}

function verifyArchitecture(target, binary) {
  if (target === 'universal-apple-darwin') {
    const architectures = run('lipo', ['-archs', binary], { capture: true })
      .split(/\s+/)
      .filter(Boolean);
    if (!architectures.includes('arm64') || !architectures.includes('x86_64')) {
      fail(`${binary} is not universal; lipo reported: ${architectures.join(' ')}`);
    }
    return;
  }
  if (target === 'x86_64-pc-windows-msvc') {
    verifyPeX86_64(binary);
    return;
  }
  if (target === 'x86_64-unknown-linux-gnu') {
    verifyElfX86_64(binary);
  }
}

function canRunTarget(target) {
  return (
    (target === 'universal-apple-darwin' && process.platform === 'darwin') ||
    (target === 'x86_64-pc-windows-msvc' && process.platform === 'win32') ||
    (target === 'x86_64-unknown-linux-gnu' && process.platform === 'linux' && process.arch === 'x64')
  );
}

function verifyVersion(binary, target, expectedVersion) {
  if (!canRunTarget(target)) return;
  let stdout;
  try {
    stdout = execFileSync(binary, ['version', '--json'], {
      cwd: ROOT_DIR,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    }).trim();
  } catch (error) {
    fail(`Could not execute staged CLI: ${error.message}`);
  }
  let reported;
  try {
    reported = JSON.parse(stdout).version;
  } catch {
    fail(`Staged CLI returned invalid version JSON: ${stdout}`);
  }
  if (reported !== expectedVersion) {
    fail(`Staged CLI reports version ${reported}; expected ${expectedVersion}`);
  }
}

function verifyBinary(target, binary, expectedVersion) {
  if (!existsSync(binary)) fail(`Staged CLI is missing: ${binary}`);
  const size = statSync(binary).size;
  if (size < 1024) fail(`Staged CLI is unexpectedly small: ${size} bytes`);
  verifyLegalFiles(dirname(binary));
  verifyArchitecture(target, binary);
  verifyVersion(binary, target, expectedVersion);
  console.log(`Verified ${target} CLI ${expectedVersion}: ${binary} (${size} bytes)`);
}

const args = parseArgs(process.argv.slice(2));
if (!args.target) fail('--target is required');
if (!SUPPORTED_TARGETS.has(args.target)) fail(`Unsupported release target: ${args.target}`);

const version = verifiedSourceVersion(args.version);
const outDir = args.outDir
  ? resolve(process.cwd(), args.outDir)
  : join(DEFAULT_OUTPUT_ROOT, version, args.target);
const extension = args.target.includes('windows') ? '.exe' : '';
const destination = join(outDir, `beambench-cli${extension}`);

if (!args.verifyOnly) {
  buildAndStage(args.target, destination);
  stageLegalFiles(outDir);
}
verifyBinary(args.target, destination, version);
