import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parse } from 'smol-toml';
import { NativeClient } from './native-sdk.js';

const MAX_RELEASE_BYTES = 20 * 1024 * 1024 * 1024;
const MAX_RELEASE_FILES = 10_000;
export const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

export function loadConfig(configPath = path.join(repoRoot, 'launcher.toml')) {
  const config = parse(fs.readFileSync(configPath, 'utf8'));
  config.origin_url = process.env.PONTEMESH_ORIGIN_URL || config.origin_url || 'http://127.0.0.1:8080';
  config.application_token = process.env.PONTEMESH_APPLICATION_TOKEN || config.application_token || '';
  const origin = new URL(config.origin_url);
  if (!['http:', 'https:'].includes(origin.protocol) || !origin.hostname) {
    throw new Error('origin_url must be an HTTP or HTTPS URL with a host');
  }
  for (const name of ['application_token', 'release_bucket', 'release_manifest_key']) {
    if (typeof config[name] !== 'string' || !config[name].trim()) throw new Error(`${name} cannot be empty`);
  }
  return config;
}

export function validateManifest(value) {
  if (!value || typeof value !== 'object' || value.schemaVersion !== 1) {
    throw new Error('release descriptor must use schemaVersion 1');
  }
  if (typeof value.version !== 'string' || !value.version.trim()) throw new Error('release version cannot be empty');
  if (!Array.isArray(value.files) || value.files.length === 0 || value.files.length > MAX_RELEASE_FILES) {
    throw new Error('release must contain between 1 and 10,000 files');
  }
  const seen = new Set();
  let total = 0;
  for (const item of value.files) {
    if (!item || typeof item !== 'object') throw new Error('every release file must be an object');
    for (const field of ['bucket', 'key', 'path', 'sha256']) {
      if (typeof item[field] !== 'string' || !item[field].trim()) throw new Error(`release file ${field} cannot be empty`);
    }
    const parts = item.path.split('/');
    if (path.posix.isAbsolute(item.path) || item.path.includes('\\') || parts.includes('..')) {
      throw new Error(`unsafe release path: ${item.path}`);
    }
    if (seen.has(item.path)) throw new Error(`duplicate release path: ${item.path}`);
    seen.add(item.path);
    if (!Number.isSafeInteger(item.sizeBytes) || item.sizeBytes < 0) throw new Error('sizeBytes must be a non-negative integer');
    if (!/^[a-fA-F0-9]{64}$/.test(item.sha256)) throw new Error('sha256 must contain 64 hexadecimal characters');
    total += item.sizeBytes;
  }
  if (total > MAX_RELEASE_BYTES) throw new Error('release exceeds the 20 GiB example limit');
  value.files.sort((left, right) => (left.order || 0) - (right.order || 0));
  return value;
}

export function replaceInstallation(installRoot, staging) {
  const rollback = `${installRoot}.rollback`;
  fs.rmSync(rollback, { recursive: true, force: true });
  if (fs.existsSync(installRoot)) fs.renameSync(installRoot, rollback);
  try {
    fs.renameSync(staging, installRoot);
  } catch (error) {
    if (fs.existsSync(rollback)) fs.renameSync(rollback, installRoot);
    throw error;
  }
  fs.rmSync(rollback, { recursive: true, force: true });
}

function addSummary(total, current) {
  for (const [name, value] of Object.entries(current)) total[name] = Number(total[name] || 0) + Number(value || 0);
}

export function install() {
  const config = loadConfig();
  const installRoot = path.join(repoRoot, 'runtime', 'installations', 'javascript');
  fs.mkdirSync(path.dirname(installRoot), { recursive: true });
  const work = fs.mkdtempSync(path.join(path.dirname(installRoot), '.pontemesh-js-'));
  const client = new NativeClient(config.origin_url, config.application_token);
  const total = {};
  try {
    const descriptor = path.join(work, 'release.json');
    client.syncObject(config.release_bucket, config.release_manifest_key, descriptor);
    const manifest = validateManifest(JSON.parse(fs.readFileSync(descriptor, 'utf8')));
    const staging = path.join(work, 'installation');
    fs.mkdirSync(staging);
    for (const item of manifest.files) {
      const destination = path.join(staging, ...item.path.split('/'));
      const summary = client.syncObject(item.bucket, item.key, destination, (fragment, downloaded, size, source) => {
        const percent = size ? Math.floor(downloaded * 100 / size) : 100;
        console.log(`${item.path}: ${String(percent).padStart(3)}% (fragment ${fragment + 1}, ${source})`);
      });
      const contents = fs.readFileSync(destination);
      const digest = crypto.createHash('sha256').update(contents).digest('hex');
      if (contents.length !== item.sizeBytes || digest.toLowerCase() !== item.sha256.toLowerCase()) {
        throw new Error(`release verification failed for ${item.path}`);
      }
      addSummary(total, summary);
    }
    fs.writeFileSync(path.join(staging, '.pontemesh-version'), `${manifest.version}\n`);
    replaceInstallation(installRoot, staging);
    console.log(`\nUpdate ${manifest.version} installed in ${installRoot}`);
    console.log(`Origin: ${total.bytes_from_origin || 0} bytes; Replica/Edge: ${total.bytes_from_replica || 0}; Peers: ${total.bytes_from_peer || 0}`);
    console.log('Game status: READY TO PLAY');
    return installRoot;
  } finally {
    client.close();
    fs.rmSync(work, { recursive: true, force: true });
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    install();
  } catch (error) {
    console.error(`Update failed: ${error.message}`);
    process.exitCode = 1;
  }
}
