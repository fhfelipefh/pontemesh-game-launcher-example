import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import koffi from 'koffi';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

export function libraryFilename(platform = process.platform) {
  if (platform === 'win32') return 'pontemesh_sdk.dll';
  if (platform === 'darwin') return 'libpontemesh_sdk.dylib';
  return 'libpontemesh_sdk.so';
}

export function findLibrary() {
  const filename = libraryFilename();
  const candidates = [
    process.env.PONTEMESH_SDK_LIBRARY,
    path.join(repoRoot, 'native', filename),
    path.join(repoRoot, '..', 'pontemesh-sdk', 'target', 'release', filename),
  ].filter(Boolean);
  const found = candidates.find(candidate => fs.existsSync(candidate));
  if (!found) {
    throw new Error(`Could not find ${filename}. Set PONTEMESH_SDK_LIBRARY or extract an SDK release into native/.`);
  }
  return path.resolve(found);
}

export class NativeClient {
  constructor(originUrl, applicationToken, libraryPath = findLibrary()) {
    const library = koffi.load(libraryPath);
    koffi.opaque('PontemeshClient');
    const Summary = koffi.struct('PontemeshTransferSummary', {
      bytes_from_peer: 'uint64_t',
      bytes_from_replica: 'uint64_t',
      bytes_from_origin: 'uint64_t',
      fragments_from_peer: 'uint64_t',
      fragments_from_replica: 'uint64_t',
      fragments_from_origin: 'uint64_t',
      peer_failures: 'uint64_t',
      peer_hash_failures: 'uint64_t',
      peer_rejected_fragments: 'uint64_t',
      fallback_activations: 'uint64_t',
    });
    const Progress = koffi.proto(
      'void PontemeshProgress(uint32_t fragment, uint64_t downloaded, uint64_t total, const char *source, void *user_data)',
    );
    this.create = library.func(
      'int pontemesh_client_create(const char *origin_url, const char *application_token, _Out_ PontemeshClient **out_client)',
    );
    this.sync = library.func(
      'int pontemesh_client_sync_object_with_summary_and_progress(PontemeshClient *client, const char *bucket, const char *key, const char *destination, _Out_ PontemeshTransferSummary *summary, PontemeshProgress *callback, void *user_data)',
    );
    this.lastError = library.func(
      'int pontemesh_client_get_last_error(PontemeshClient *client, _Out_ char *buffer, size_t buffer_len)',
    );
    this.free = library.func('void pontemesh_client_free(PontemeshClient *client)');
    this.Progress = Progress;
    this.Summary = Summary;
    const output = [null];
    const status = this.create(originUrl, applicationToken, output);
    if (status !== 0) throw new Error(`pontemesh_client_create failed with status ${status}`);
    this.client = output[0];
  }

  syncObject(bucket, key, destination, onProgress = undefined) {
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    const summary = {};
    const callback = (fragment, downloaded, total, source) => {
      if (onProgress) onProgress(Number(fragment), Number(downloaded), Number(total), source);
    };
    const status = this.sync(this.client, bucket, key, destination, summary, callback, null);
    if (status !== 0) throw new Error(this.getLastError(status));
    return summary;
  }

  getLastError(status) {
    const buffer = Buffer.alloc(2048);
    this.lastError(this.client, buffer, buffer.length);
    const end = buffer.indexOf(0);
    return buffer.toString('utf8', 0, end < 0 ? buffer.length : end) || `Ponte Mesh SDK failed with status ${status}`;
  }

  close() {
    if (this.client) {
      this.free(this.client);
      this.client = null;
    }
  }
}
