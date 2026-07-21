#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const API_VERSION = '2022-11-28';

function parseArgs(argv) {
  const files = [];
  let tag;

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--tag') {
      tag = argv[++index];
    } else if (argument === '--file') {
      files.push(argv[++index]);
    } else {
      throw new Error(`Unknown argument: ${argument}`);
    }
  }

  if (!tag) throw new Error('Missing required --tag');
  if (files.length === 0) throw new Error('At least one --file is required');
  if (files.some((file) => !file)) throw new Error('--file requires a path');

  return { tag, files };
}

export function selectRelease(releases, tag) {
  const matching = releases.filter((release) => release.tag_name === tag);
  const published = matching.filter((release) => !release.draft);
  const drafts = matching.filter((release) => release.draft);

  if (published.length > 1 || drafts.length > 1 || (published.length && drafts.length)) {
    throw new Error(`Multiple GitHub Releases already use tag ${tag}`);
  }

  return published[0] ?? drafts[0] ?? null;
}

export function assetNameCandidates(fileName) {
  return [...new Set([fileName, fileName.replaceAll(' ', '.')])];
}

function requireEnvironment(name) {
  const value = process.env[name];
  if (!value) throw new Error(`Missing required environment variable ${name}`);
  return value;
}

async function githubRequest(url, token, options = {}) {
  const response = await fetch(url, {
    ...options,
    headers: {
      Authorization: `Bearer ${token}`,
      Accept: 'application/vnd.github+json',
      'X-GitHub-Api-Version': API_VERSION,
      ...options.headers,
    },
  });

  if (!response.ok) {
    const detail = (await response.text()).trim();
    throw new Error(`GitHub API ${response.status} for ${url}: ${detail || response.statusText}`);
  }

  if (response.status === 204) return null;
  return response.json();
}

async function listReleases(repository, token) {
  const releases = [];

  for (let page = 1; ; page += 1) {
    const batch = await githubRequest(
      `https://api.github.com/repos/${repository}/releases?per_page=100&page=${page}`,
      token,
    );
    releases.push(...batch);
    if (batch.length < 100) return releases;
  }
}

async function resolveRelease(repository, token, tag) {
  const existing = selectRelease(await listReleases(repository, token), tag);
  if (existing) {
    console.log(`Using ${existing.draft ? 'draft' : 'published'} release ${existing.id} for ${tag}`);
    return existing;
  }

  const created = await githubRequest(`https://api.github.com/repos/${repository}/releases`, token, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      tag_name: tag,
      target_commitish: process.env.GITHUB_SHA,
      name: tag,
      draft: true,
      prerelease: false,
      generate_release_notes: true,
    }),
  });
  console.log(`Created draft release ${created.id} for ${tag}`);
  return created;
}

async function deleteExistingAsset(repository, token, release, fileName) {
  const candidates = new Set(assetNameCandidates(fileName));
  const existing = release.assets.filter((asset) => candidates.has(asset.name));

  for (const asset of existing) {
    console.log(`Deleting existing release asset ${asset.name}`);
    await githubRequest(
      `https://api.github.com/repos/${repository}/releases/assets/${asset.id}`,
      token,
      { method: 'DELETE' },
    );
    release.assets = release.assets.filter((candidate) => candidate.id !== asset.id);
  }
}

async function uploadAsset(repository, token, release, filePath) {
  if (!fs.statSync(filePath).isFile()) throw new Error(`Release artifact is not a file: ${filePath}`);

  const fileName = path.basename(filePath);
  await deleteExistingAsset(repository, token, release, fileName);

  const data = fs.readFileSync(filePath);
  const uploaded = await githubRequest(
    `https://uploads.github.com/repos/${repository}/releases/${release.id}/assets?name=${encodeURIComponent(fileName)}`,
    token,
    {
      method: 'POST',
      headers: {
        'Content-Type': 'application/octet-stream',
        'Content-Length': String(data.length),
      },
      body: data,
    },
  );
  release.assets.push(uploaded);
  console.log(`Uploaded ${uploaded.name} (${uploaded.size} bytes)`);
}

async function main() {
  const { tag, files } = parseArgs(process.argv.slice(2));
  const repository = requireEnvironment('GITHUB_REPOSITORY');
  const token = process.env.GH_TOKEN || requireEnvironment('GITHUB_TOKEN');
  const release = await resolveRelease(repository, token, tag);

  for (const file of files) await uploadAsset(repository, token, release, file);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(`::error::${error.message}`);
    process.exitCode = 1;
  });
}
