import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { execFile, spawn, type ChildProcess } from 'node:child_process';
import { createConnection, createServer } from 'node:net';

const root = process.cwd();
const temporaryRoot = await mkdtemp(path.join(tmpdir(), 'onecrew-port-'));
const dbDir = path.join(temporaryRoot, 'db');
const runtimeFile = path.join(temporaryRoot, 'runtime.json');
let placeholder: ChildProcess | undefined;
let server: ChildProcess | undefined;

const waitExit = (child: ChildProcess, timeout = 10_000) =>
  new Promise<{ code: number | null; signal: NodeJS.Signals | null }>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('child process did not exit')), timeout);
    child.once('exit', (code, signal) => {
      clearTimeout(timer);
      resolve({ code, signal });
    });
  });
const waitLine = (child: ChildProcess, timeout = 5_000) =>
  new Promise<string>((resolve, reject) => {
    let output = '';
    const timer = setTimeout(
      () => reject(new Error(`child did not report a port: ${output}`)),
      timeout,
    );
    child.stdout?.on('data', (chunk: Buffer) => {
      output += chunk.toString();
      const line = output.split(/\r?\n/)[0];
      if (line) {
        clearTimeout(timer);
        resolve(line);
      }
    });
  });
const waitFor = async (predicate: () => Promise<boolean>, timeout = 60_000) => {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  throw new Error('timed out waiting for condition');
};
const stop = async (child: ChildProcess | undefined) => {
  if (!child || child.exitCode !== null) return;
  child.kill('SIGTERM');
  try {
    await waitExit(child, 8_000);
  } catch {
    child.kill('SIGKILL');
    await waitExit(child, 3_000).catch(() => undefined);
  }
};
const fetchJson = async <T>(url: string, init?: RequestInit) => {
  const response = await fetch(url, init);
  const body = (await response.json()) as T;
  if (!response.ok) throw new Error(`${response.status}: ${JSON.stringify(body)}`);
  return body;
};
const freePort = () =>
  new Promise<number>((resolve, reject) => {
    const probe = createServer();
    probe.once('error', reject);
    probe.listen(0, '127.0.0.1', () => {
      const port = (probe.address() as { port: number }).port;
      probe.close(() => resolve(port));
    });
  });
const runService = (command: string, env: NodeJS.ProcessEnv) =>
  new Promise<{ stdout: string; stderr: string }>((resolve, reject) =>
    execFile(
      process.execPath,
      ['dist/cli.js', command],
      { cwd: root, env },
      (error, stdout, stderr) =>
        error ? reject(Object.assign(error, { stdout, stderr })) : resolve({ stdout, stderr }),
    ),
  );

try {
  placeholder = spawn(
    process.execPath,
    [
      '-e',
      "require('net').createServer(socket => socket.end('placeholder')).listen(0, '127.0.0.1', function(){ console.log(this.address().port) })",
    ],
    { stdio: ['ignore', 'pipe', 'pipe'] },
  );
  const startPort = Number(await waitLine(placeholder));
  if (!Number.isInteger(startPort) || startPort < 1)
    throw new Error('placeholder did not provide a valid port');
  const env = {
    ...process.env,
    PORT: String(startPort),
    ONECREW_LITE_DATA_DIR: dbDir,
    ONECREW_LITE_RUNTIME_FILE: runtimeFile,
  };
  server = spawn(process.execPath, ['--import', 'tsx', 'src/index.ts'], {
    cwd: root,
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  server.stderr?.resume();
  const actualPort = startPort + 1;
  await waitFor(async () => {
    try {
      const response = await fetch(`http://127.0.0.1:${actualPort}/healthz`);
      return response.ok;
    } catch {
      return false;
    }
  });
  const health = await fetchJson<{
    ok: boolean;
    service: string;
    protocol_version: number;
    base_id: string;
  }>(`http://127.0.0.1:${actualPort}/healthz`);
  if (!health.ok || health.service !== 'onecrew_lite' || health.protocol_version !== 1)
    throw new Error('healthz protocol identity was invalid');
  const createWhileSignedOut = await fetch(`http://127.0.0.1:${actualPort}/api/v1/dynamic-tables`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      name: 'Port integration',
      columns: [{ name: 'Name', data_type: 'string' }],
    }),
  });
  if (createWhileSignedOut.status !== 401)
    throw new Error(`unsigned table creation returned ${createWhileSignedOut.status}`);
  const second = spawn(process.execPath, ['--import', 'tsx', 'src/index.ts'], {
    cwd: root,
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let secondErr = '';
  second.stderr?.on('data', (chunk: Buffer) => {
    secondErr += chunk.toString();
  });
  const secondResult = await waitExit(second, 15_000);
  if (secondResult.code === 0 || !/already running/i.test(secondErr))
    throw new Error(
      `second start was not rejected: code=${secondResult.code}, stderr=${secondErr}`,
    );
  await stop(server);
  server = undefined;
  await waitFor(async () => {
    try {
      await fetch(`http://127.0.0.1:${actualPort}/healthz`);
      return false;
    } catch {
      return true;
    }
  });
  const placeholderResponse = await new Promise<string>((resolve, reject) => {
    const socket = createConnection({ host: '127.0.0.1', port: startPort }, () => socket.end());
    let body = '';
    socket.on('data', (chunk) => {
      body += chunk.toString();
    });
    socket.on('end', () => resolve(body));
    socket.on('error', reject);
  });
  if (placeholderResponse !== 'placeholder')
    throw new Error('placeholder service was affected by stop');
  await waitFor(async () => {
    try {
      await readFile(runtimeFile);
      return false;
    } catch {
      return true;
    }
  });
  console.log(
    `port integration ok: occupied ${startPort}, selected ${actualPort}, runtime cleanup and isolated stop verified`,
  );
  const serviceRoot = path.join(temporaryRoot, 'service');
  const serviceRuntime = path.join(serviceRoot, 'server.json');
  const serviceEnv = {
    ...process.env,
    PORT: String(await freePort()),
    ONECREW_LITE_DATA_DIR: path.join(temporaryRoot, 'service-db'),
    ONECREW_LITE_RUNTIME_FILE: serviceRuntime,
  };
  await runService('start', serviceEnv);
  const status = await runService('status', serviceEnv);
  if (!status.stdout.includes('running') || !status.stdout.includes('http://127.0.0.1:'))
    throw new Error(`service status omitted runtime descriptor: ${status.stdout}`);
  await runService('stop', serviceEnv);
  try {
    await readFile(serviceRuntime);
    throw new Error('service stop left runtime descriptor behind');
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
  }
  console.log('CLI integration ok: start/status/stop used runtime descriptor');
} finally {
  await stop(server);
  await stop(placeholder);
  await rm(temporaryRoot, { recursive: true, force: true });
}
