import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { release, machine, hostname } from 'node:os';
import { osInfo } from 'systeminformation';
import { Hono } from 'hono';

const exec = promisify(execFile);
const normalizeArch = (value: string) => {
  const arch = value.trim().toLowerCase();
  if (['arm64', 'aarch64'].includes(arch)) return 'arm64';
  if (['x64', 'x86_64', 'amd64'].includes(arch)) return 'x64';
  if (['x86', 'ia32', 'i386', 'i686'].includes(arch)) return 'ia32';
  return null;
};

export async function getDeviceLabel() {
  let name = hostname();
  if (process.platform === 'darwin') {
    try {
      const { stdout } = await exec('/usr/sbin/scutil', ['--get', 'ComputerName'], {
        timeout: 2000,
      });
      if (stdout.trim()) name = stdout.trim();
    } catch {
      // The hostname is available when a friendly ComputerName has not been set.
    }
  }
  const clean = name
    .normalize('NFC')
    .replace(/[\p{Cc}\u202a-\u202e\u2066-\u2069]/gu, '')
    .trim();
  let label = '';
  for (const char of clean) {
    if (label.length + char.length > 80) break;
    label += char;
  }
  return label || (process.platform === 'darwin' ? 'Mac' : 'Windows PC');
}

export async function collectSystemInfo() {
  const info = await osInfo();
  let nativeArch = normalizeArch(machine());
  let translation: 'rosetta' | 'emulated' | 'native' | 'unknown' = 'unknown';
  if (process.platform === 'darwin') {
    // uname/os.machine can describe the translated process on Apple Silicon.
    try {
      const { stdout } = await exec('/usr/sbin/sysctl', ['-n', 'hw.optional.arm64'], {
        timeout: 2000,
      });
      nativeArch = stdout.trim() === '1' ? 'arm64' : 'x64';
    } catch {
      nativeArch = process.arch === 'arm64' ? 'arm64' : null;
    }
    if (nativeArch) translation = nativeArch !== process.arch ? 'rosetta' : 'native';
  } else {
    if (process.platform === 'win32') {
      nativeArch = normalizeArch(
        process.env.PROCESSOR_ARCHITEW6432 || process.env.PROCESSOR_ARCHITECTURE || machine(),
      );
    }
    if (nativeArch) translation = nativeArch !== process.arch ? 'emulated' : 'native';
  }
  // Explicit whitelist: never expose hostname, serial number, or user home path.
  return {
    os: {
      platform: process.platform,
      name: info.distro,
      version:
        process.platform === 'win32' && info.codename
          ? `${info.codename} (${info.release})`
          : info.release,
      build: info.build || null,
      kernel: release(),
      native_arch: nativeArch,
    },
    runtime: { arch: process.arch, node: process.versions.node },
    translation,
  };
}

export function createSystemRoutes() {
  let cached: ReturnType<typeof collectSystemInfo> | undefined;
  return new Hono().get('/info', async (c) => {
    c.header('Cache-Control', 'no-store');
    const url = new URL(c.req.url);
    const origin = c.req.header('Origin');
    if (url.hostname !== '127.0.0.1' || (origin && origin !== url.origin)) {
      return c.json({ error: { code: 'system.origin', message: 'Local requests only' } }, 403);
    }
    cached ??= collectSystemInfo().catch((error: unknown) => {
      cached = undefined;
      throw error;
    });
    return c.json(await cached);
  });
}
