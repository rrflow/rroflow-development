// Shared by the SDK suites: the built binaries they drive and small stream helpers.

import { execFile } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

export const exec = promisify(execFile);
export const root = fileURLToPath(new URL('../../', import.meta.url));
const suffix = process.platform === 'win32' ? '.exe' : '';
export const binDir = process.env.RROFLOW_TEST_BIN_DIR || resolve(root, 'target/debug');
export const binary = resolve(binDir, `rroflow${suffix}`);
export const ingest = resolve(binDir, `rroflow-ingest${suffix}`);
export const projector = resolve(binDir, 'examples', `project-capture${suffix}`);
export async function* events(items) {
  yield* items;
}
export async function consume(stream) {
  const out = [];
  for await (const e of stream) out.push(e);
  return out;
}
