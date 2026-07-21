#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';

function parseArgs(argv) {
  const args = {};
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (!arg.startsWith('--')) continue;
    const key = arg.slice(2);
    const value = argv[i + 1] && !argv[i + 1].startsWith('--') ? argv[++i] : 'true';
    args[key] = value;
  }
  return args;
}

function requireArg(args, key) {
  const value = args[key];
  if (!value) {
    throw new Error(`Missing required --${key}`);
  }
  return value;
}

function readSignature(artifactPath) {
  const sigPath = `${artifactPath}.sig`;
  return fs.readFileSync(sigPath, 'utf8').trim();
}

function artifactUrl(baseUrl, version, artifactPath, explicitUrl) {
  if (explicitUrl) return explicitUrl;
  return `${baseUrl.replace(/\/+$/, '')}/${version}/${encodeURIComponent(path.basename(artifactPath))}`;
}

function extractReleaseNotes(changelogPath, version) {
  const text = fs.readFileSync(changelogPath, 'utf8');
  const escaped = version.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const heading = new RegExp(`^##\\s+\\[?v?${escaped}\\]?\\s*$`, 'im');
  const match = heading.exec(text);
  if (!match) {
    throw new Error(`No CHANGELOG.md section found for version ${version}`);
  }

  const start = match.index + match[0].length;
  const rest = text.slice(start);
  const nextHeading = rest.search(/^##\s+/m);
  return (nextHeading === -1 ? rest : rest.slice(0, nextHeading)).trim();
}

const args = parseArgs(process.argv.slice(2));
const version = requireArg(args, 'version').replace(/^v/, '');
const baseUrl = requireArg(args, 'base-url');
const macArtifact = args['mac-artifact'];
const windowsArtifact = args['windows-artifact'];
const linuxArtifact = args['linux-artifact'];
const changelogPath = args.changelog ?? '../CHANGELOG.md';
const outPath = args.out ?? 'latest.json';
const pubDate = args['pub-date'] ?? new Date().toISOString();
const notes = args.notes ?? extractReleaseNotes(changelogPath, version);

if (!macArtifact && !windowsArtifact && !linuxArtifact) {
  throw new Error('At least one of --mac-artifact, --windows-artifact, or --linux-artifact is required');
}

const platforms = {};

if (macArtifact) {
  const macUrl = artifactUrl(baseUrl, version, macArtifact, args['mac-url']);
  const macSignature = readSignature(macArtifact);
  platforms['darwin-aarch64'] = {
    signature: macSignature,
    url: macUrl,
  };
  platforms['darwin-x86_64'] = {
    signature: macSignature,
    url: macUrl,
  };
}

if (windowsArtifact) {
  const windowsUrl = artifactUrl(baseUrl, version, windowsArtifact, args['windows-url']);
  const windowsSignature = readSignature(windowsArtifact);
  platforms['windows-x86_64'] = {
    signature: windowsSignature,
    url: windowsUrl,
  };
}

if (linuxArtifact) {
  const linuxUrl = artifactUrl(baseUrl, version, linuxArtifact, args['linux-url']);
  const linuxSignature = readSignature(linuxArtifact);
  platforms['linux-x86_64'] = {
    signature: linuxSignature,
    url: linuxUrl,
  };
}

const manifest = {
  version,
  notes,
  pub_date: pubDate,
  platforms,
};

fs.writeFileSync(outPath, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`Wrote ${outPath}`);
