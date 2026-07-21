#!/usr/bin/env node
import fs from 'node:fs';

const DEFAULT_REQUIRED_PLATFORMS = [
  'darwin-aarch64',
  'darwin-x86_64',
  'windows-x86_64',
  'linux-x86_64',
];
const SEMVER = /^v?\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/;
const BASE64ISH = /^[A-Za-z0-9+/=\r\n]+$/;

function fail(message) {
  throw new Error(message);
}

async function urlReturnsOk(url) {
  let response = await fetch(url, { method: 'HEAD' });
  if (response.status === 405) {
    response = await fetch(url, { method: 'GET' });
  }
  return response.status === 200;
}

function parseFlags(flags) {
  const options = {
    checkUrls: false,
    requiredPlatforms: DEFAULT_REQUIRED_PLATFORMS,
  };

  for (let i = 0; i < flags.length; i += 1) {
    const flag = flags[i];
    if (flag === '--check-urls') {
      options.checkUrls = true;
    } else if (flag === '--platforms') {
      const value = flags[++i];
      if (!value || value.startsWith('--')) {
        fail('--platforms requires a comma-separated platform list');
      }
      options.requiredPlatforms = value
        .split(',')
        .map((platform) => platform.trim())
        .filter(Boolean);
      if (options.requiredPlatforms.length === 0) {
        fail('--platforms must include at least one platform');
      }
    } else {
      fail(`Unknown flag ${flag}`);
    }
  }

  return options;
}

const [, , manifestPath, ...flags] = process.argv;
if (!manifestPath) {
  fail(
    'Usage: node scripts/validate-update-manifest.mjs <latest.json> [--platforms darwin-aarch64,darwin-x86_64] [--check-urls]',
  );
}

const { checkUrls, requiredPlatforms } = parseFlags(flags);
const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));

if (!SEMVER.test(String(manifest.version ?? ''))) {
  fail('version must be SemVer, with optional leading v');
}
if (manifest.pub_date && Number.isNaN(Date.parse(manifest.pub_date))) {
  fail('pub_date must be RFC3339-compatible when present');
}
if (!manifest.platforms || typeof manifest.platforms !== 'object') {
  fail('platforms object is required');
}
if (Object.keys(manifest.platforms).length === 0) {
  fail('platforms must include at least one platform');
}

for (const platform of requiredPlatforms) {
  if (!manifest.platforms[platform]) fail(`missing platform ${platform}`);
}

for (const [platform, entry] of Object.entries(manifest.platforms)) {
  if (typeof entry.url !== 'string' || !entry.url.startsWith('https://')) {
    fail(`${platform}.url must be an HTTPS URL`);
  }
  if (typeof entry.signature !== 'string' || entry.signature.length === 0) {
    fail(`${platform}.signature must be an inlined signature string`);
  }
  if (entry.signature.startsWith('http://') || entry.signature.startsWith('https://')) {
    fail(`${platform}.signature must not be a URL`);
  }
  if (!BASE64ISH.test(entry.signature)) {
    fail(`${platform}.signature does not look like base64 signature content`);
  }
  if (checkUrls && !(await urlReturnsOk(entry.url))) {
    fail(`${platform}.url did not return HTTP 200: ${entry.url}`);
  }
}

console.log(`${manifestPath} is valid`);
