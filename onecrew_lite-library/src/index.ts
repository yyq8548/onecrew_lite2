import { createAdaptorServer } from '@hono/node-server';
import { readFile, unlink, mkdir, writeFile, rename } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { randomBytes, randomUUID } from 'node:crypto';
import { acquireRuntimeLock, runtimePaths } from './runtime.js';

import { createApp } from './app.js';
import { createDiscoverySyncRoutes } from './modules/tables/discovery-sync-http.js';

const configuredPort = process.env.PORT === undefined ? 8191 : Number(process.env.PORT);
if (!Number.isInteger(configuredPort) || configuredPort < 1 || configuredPort > 65535)
  throw new Error('PORT must be an integer between 1 and 65535');
const { runtimeFile, dataDir } = runtimePaths();
const runtimeDir = path.dirname(runtimeFile);
await mkdir(dataDir, { recursive: true, mode: 0o700 });
await mkdir(runtimeDir, { recursive: true, mode: 0o700 });
const lock = await acquireRuntimeLock();
if (!lock) throw new Error('onecrew_lite is already running or starting');
let lockOwned = true;
const releaseLock = async () => {
  if (!lockOwned) return;
  lockOwned = false;
  await lock.close();
};

let port = configuredPort;
let readyFetch: (request: Request, env?: unknown) => Response | Promise<Response> = () =>
  new Response('Service starting', { status: 503 });
const server = createAdaptorServer({
  fetch: (request, env) => readyFetch(request, env),
  hostname: '127.0.0.1',
});
const listen = async () => {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    const candidate = configuredPort + attempt;
    if (candidate > 65535) break;
    try {
      await new Promise<void>((resolve, reject) => {
        const onListening = () => {
          server.off('error', onError);
          resolve();
        };
        const onError = (error: NodeJS.ErrnoException) => {
          server.off('listening', onListening);
          reject(error);
        };
        server.once('error', onError);
        server.once('listening', onListening);
        server.listen(candidate, '127.0.0.1');
      });
      port = (server.address() as { port: number }).port;
      return;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'EADDRINUSE') throw error;
    }
  }
  throw new Error(
    `no available port in range ${configuredPort}-${Math.min(configuredPort + 19, 65535)}`,
  );
};
try {
  await listen();
} catch (error) {
  await releaseLock();
  throw error;
}
process.env.PORT = String(port);
const rootDir = dataDir;
let appResources: Awaited<ReturnType<typeof createApp>>;
try {
  appResources = await createApp(rootDir);
} catch (error) {
  await new Promise<void>((resolve) => server.close(() => resolve()));
  await releaseLock();
  throw error;
}
const { app, data, browsers, closeMcp } = appResources;

const runtime = {
  pid: process.pid,
  port,
  url: `http://127.0.0.1:${port}`,
  service: 'onecrew_lite',
  protocol_version: 1,
  instance_id: randomUUID(),
  control_token: randomBytes(32).toString('hex'),
  data_dir: dataDir,
};
const discoverySync = createDiscoverySyncRoutes(
  data,
  appResources.resolveTenant,
  runtime.control_token,
);
readyFetch = (request, env) => {
  const url = new URL(request.url);
  if (url.pathname.startsWith('/_local/')) {
    if (
      request.headers.has('Origin') ||
      request.headers.get('Authorization') !== `Bearer ${runtime.control_token}`
    )
      return new Response('Forbidden', { status: 403 });
    if (url.pathname === '/_local/status' && request.method === 'GET')
      return Response.json({ instance_id: runtime.instance_id });
    if (url.pathname.startsWith('/_local/discovery/')) return discoverySync.fetch(request, env);
    if (url.pathname === '/_local/shutdown' && request.method === 'POST') {
      setTimeout(shutdown, 50);
      return Response.json({ stopping: true }, { status: 202 });
    }
    return new Response('Not found', { status: 404 });
  }
  return app.fetch(request, env);
};
const temporaryRuntimeFile = `${runtimeFile}.${process.pid}.tmp`;
try {
  await writeFile(temporaryRuntimeFile, `${JSON.stringify(runtime)}\n`, {
    mode: 0o600,
  });
  await rename(temporaryRuntimeFile, runtimeFile);
} catch (error) {
  await unlink(temporaryRuntimeFile).catch(() => undefined);
  browsers.closeAll();
  await closeMcp().catch(() => undefined);
  await new Promise<void>((resolve) => server.close(() => resolve()));
  await data.close().catch(() => undefined);
  await releaseLock();
  throw error;
}
console.log(`onecrew_lite listening at ${runtime.url}`);
let stopping = false;
const shutdown = () => {
  if (stopping) return;
  stopping = true;
  readyFetch = () => new Response('Service stopping', { status: 503 });
  void shutdownAsync().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
    process.exit(1);
  });
};

async function shutdownAsync() {
  const timeout = setTimeout(() => {
    console.error('Shutdown timed out after 10 seconds');
    process.exit(1);
  }, 10_000);
  timeout.unref();
  try {
    const serverClosed = new Promise<void>((resolve, reject) => {
      server.close((error) => (error ? reject(error) : resolve()));
    });
    browsers.closeAll();
    await closeMcp();
    await serverClosed;
    await data.close();
    try {
      const current = JSON.parse(await readFile(runtimeFile, 'utf8')) as {
        instance_id?: string;
      };
      if (current.instance_id === runtime.instance_id) await unlink(runtimeFile);
    } catch {
      /* A newer owner may have replaced the discovery cache. */
    }
    await releaseLock();
  } finally {
    clearTimeout(timeout);
  }
}
process.once('SIGINT', shutdown);
process.once('SIGTERM', shutdown);
