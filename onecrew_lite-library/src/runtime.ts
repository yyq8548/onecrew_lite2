import { access, open, readFile, type FileHandle } from 'node:fs/promises';
import { createRequire } from 'node:module';
import os from 'node:os';
import path from 'node:path';

type NativeLock = {
  tryLock(fd: number): boolean;
};

const require = createRequire(import.meta.url);
const nativeLock = require('fs-native-extensions') as NativeLock;

/** One profile owns one database and one server; a profile is not a tenant. */
export function runtimePaths() {
  const systemData =
    process.platform === 'win32'
      ? path.join(
          process.env.LOCALAPPDATA ?? path.join(os.homedir(), 'AppData', 'Local'),
          'onecrew_lite_tables',
        )
      : process.platform === 'darwin'
        ? path.join(os.homedir(), 'Library', 'Application Support', 'onecrew_lite_tables')
        : path.join(
            process.env.XDG_DATA_HOME ?? path.join(os.homedir(), '.local', 'share'),
            'onecrew_lite_tables',
          );
  const dataDir = path.resolve(process.env.ONECREW_LITE_DATA_DIR ?? systemData);
  const runtimeFile = path.resolve(
    process.env.ONECREW_LITE_RUNTIME_FILE ?? path.join(dataDir, 'run', 'server.json'),
  );
  return {
    dataDir,
    runtimeFile,
    lockFile: path.join(dataDir, 'server.lock'),
    logFile: path.join(dataDir, 'server.log'),
  };
}

export type RuntimeDescriptor = {
  service: 'onecrew_lite';
  protocol_version: 1;
  pid: number;
  port: number;
  url: string;
  instance_id: string;
  control_token: string;
  data_dir: string;
};

export type LockProbe = 'missing' | 'available' | 'locked';

/** Open and hold the profile lock. The descriptor is deliberately only a cache. */
export async function acquireRuntimeLock(): Promise<FileHandle | undefined> {
  const { lockFile } = runtimePaths();
  return acquireLockFile(lockFile);
}

/** Also protect direct bootstrap/database callers, not only the HTTP launcher. */
export async function acquireDatabaseLock(rootDir: string): Promise<FileHandle | undefined> {
  return acquireLockFile(path.join(path.resolve(rootDir), 'database.lock'));
}

async function acquireLockFile(lockFile: string): Promise<FileHandle | undefined> {
  const handle = await open(lockFile, 'a+', 0o600);
  try {
    if (!nativeLock.tryLock(handle.fd)) {
      await handle.close();
      return undefined;
    }
    return handle;
  } catch (error) {
    await handle.close().catch(() => undefined);
    throw error;
  }
}

/** Probe without creating the profile or its lock file. */
export async function probeRuntimeLock(): Promise<LockProbe> {
  const { lockFile } = runtimePaths();
  try {
    await access(lockFile);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return 'missing';
    throw error;
  }
  const handle = await open(lockFile, 'a+', 0o600);
  try {
    const locked = !nativeLock.tryLock(handle.fd);
    return locked ? 'locked' : 'available';
  } finally {
    await handle.close();
  }
}

export async function readRuntime(): Promise<RuntimeDescriptor | undefined> {
  let text: string;
  try {
    text = await readFile(runtimePaths().runtimeFile, 'utf8');
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return undefined;
    throw error;
  }
  const r = JSON.parse(text) as RuntimeDescriptor;
  const url = new URL(r.url);
  if (
    r.service !== 'onecrew_lite' ||
    r.protocol_version !== 1 ||
    !Number.isInteger(r.pid) ||
    r.pid <= 0 ||
    !Number.isInteger(r.port) ||
    r.port < 1 ||
    r.port > 65535 ||
    url.origin !== `http://127.0.0.1:${r.port}` ||
    url.pathname !== '/' ||
    url.search ||
    url.hash ||
    url.username ||
    url.password ||
    typeof r.instance_id !== 'string' ||
    !r.instance_id ||
    typeof r.control_token !== 'string' ||
    !r.control_token ||
    r.data_dir !== runtimePaths().dataDir
  ) {
    throw new Error('Invalid onecrew_lite runtime descriptor for this data directory');
  }
  return r;
}

export async function verifyRuntime(r: RuntimeDescriptor) {
  const response = await fetch(`${r.url}/_local/status`, {
    headers: { Authorization: `Bearer ${r.control_token}` },
    signal: AbortSignal.timeout(2000),
  });
  const info = (await response.json()) as { instance_id?: string };
  if (!response.ok || info.instance_id !== r.instance_id)
    throw new Error('onecrew_lite instance identity mismatch');
}
