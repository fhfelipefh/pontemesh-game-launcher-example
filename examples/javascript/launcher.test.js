import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { replaceInstallation, validateManifest } from './launcher.js';

function manifest() {
  return {
    schemaVersion: 1,
    version: '1.0.0',
    files: [{
      bucket: 'game-updates',
      key: 'releases/1.0.0/game/update.pak',
      path: 'game/update.pak',
      sizeBytes: 8,
      sha256: 'a'.repeat(64),
      order: 10,
    }],
  };
}

test('accepts a safe release', () => {
  assert.equal(validateManifest(manifest()).version, '1.0.0');
});

test('rejects parent traversal', () => {
  const value = manifest();
  value.files[0].path = '../outside.pak';
  assert.throws(() => validateManifest(value), /unsafe release path/);
});

test('rejects duplicate paths', () => {
  const value = manifest();
  value.files.push({ ...value.files[0] });
  assert.throws(() => validateManifest(value), /duplicate release path/);
});

test('replaces an existing installation', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'pontemesh-js-test-'));
  try {
    const installed = path.join(root, 'installed');
    const staging = path.join(root, 'staging');
    fs.mkdirSync(installed);
    fs.mkdirSync(staging);
    fs.writeFileSync(path.join(installed, 'old.txt'), 'old');
    fs.writeFileSync(path.join(staging, 'new.txt'), 'new');
    replaceInstallation(installed, staging);
    assert.equal(fs.existsSync(path.join(installed, 'old.txt')), false);
    assert.equal(fs.readFileSync(path.join(installed, 'new.txt'), 'utf8'), 'new');
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
