import { createSystemRoutes } from './system.js';
import { AccessService } from './modules/access/service.js';
import { createAccessRoutes, createAccessCallback } from './modules/access/http.js';
import { runtimePaths } from './runtime.js';
import { fileURLToPath } from 'node:url';
import { Hono } from 'hono';
import { bodyLimit } from 'hono/body-limit';
import { serveStatic } from '@hono/node-server/serve-static';

import { bootstrap } from './bootstrap.js';
import { createMcpTransport } from './mcp/transport.js';
import { createTableRoutes } from './modules/tables/http.js';
import { toErrorBody } from './modules/tables/service.js';
import { BrowserBridgeRegistry, isLocalOrigin } from './modules/browser/service.js';
import { createBrowserRoutes } from './modules/browser/http.js';
import {
  createRequestTenantResolver,
  type PrincipalResolver,
  type TenantHttpEnv,
} from './modules/tenancy/service.js';
import type { ClientErrorStatusCode, ServerErrorStatusCode } from 'hono/utils/http-status';

type ErrorStatusCode = ClientErrorStatusCode | ServerErrorStatusCode;
type ErrorDetails = { status?: unknown; code?: unknown };

export const createApp = async (
  rootDir = runtimePaths().dataDir,
  accessOverride?: AccessService,
  principalResolverOverride?: PrincipalResolver,
) => {
  const {
    data,
    service,
    database,
    access: bootstrappedAccess,
    tenants,
  } = await bootstrap(rootDir, accessOverride);
  const access = bootstrappedAccess;
  const principalResolver: PrincipalResolver =
    principalResolverOverride ??
    (async () => {
      const status = await access.status();
      if (status.state === 'signed_in' && status.user) return status.user;
      throw Object.assign(
        new Error(
          status.state === 'unavailable'
            ? 'The signed-in account is unavailable'
            : 'Sign in before accessing onecrew_lite',
        ),
        {
          status: status.state === 'unavailable' ? 503 : 401,
          code: status.state === 'unavailable' ? 'access.unavailable' : 'access.login_required',
        },
      );
    });
  const resolveTenant = createRequestTenantResolver(tenants, principalResolver);
  const app = new Hono<TenantHttpEnv>();
  const browsers = new BrowserBridgeRegistry();
  const webDist = fileURLToPath(new URL('../web/dist', import.meta.url));
  const jsonLimit = bodyLimit({
    maxSize: 2 * 1024 * 1024,
    onError: (c) =>
      c.json({ error: { code: 'request.too_large', message: 'Request body exceeds 2MB' } }, 413),
  });
  const importLimit = bodyLimit({
    maxSize: 10 * 1024 * 1024 + 64 * 1024,
    onError: (c) =>
      c.json({ error: { code: 'request.too_large', message: 'Upload exceeds 10MB' } }, 413),
  });
  const mcp = createMcpTransport(
    service,
    {
      service: access,
      onLogout: (userId) => {
        if (userId) browsers.closeForUser(userId);
      },
    },
    resolveTenant,
  );

  app.onError((error, c) => {
    const body = toErrorBody(error);
    const status =
      error && typeof error === 'object' && 'status' in error && typeof error.status === 'number'
        ? error.status
        : body.error.code.startsWith('validation.')
          ? 400
          : 500;
    return c.json(body, status as ErrorStatusCode);
  });
  const requireLogin: import('hono').MiddlewareHandler<TenantHttpEnv> = async (c, next) => {
    if (c.req.method === 'OPTIONS') return next();
    try {
      c.set('tenant', await resolveTenant(c.req.raw));
    } catch (error) {
      const details = error !== null && typeof error === 'object' ? (error as ErrorDetails) : {};
      const origin = c.req.header('Origin');
      if (
        origin &&
        /^chrome-extension:\/\/[a-p]{32}$/.test(origin) &&
        origin === `chrome-extension://${c.req.header('X-OneCrew-Extension-Id')}`
      ) {
        c.header('Access-Control-Allow-Origin', origin);
      }
      c.header('Cache-Control', 'no-store');
      const status = typeof details.status === 'number' ? details.status : 401;
      return c.json(
        {
          error: {
            code:
              typeof details.code === 'string'
                ? details.code
                : status === 503
                  ? 'access.unavailable'
                  : 'access.login_required',
            message: error instanceof Error ? error.message : 'Sign in before accessing onecrew_lite',
            ...(status === 401 || status === 503 ? { login_url: '/login' } : {}),
          },
        },
        status as ErrorStatusCode,
      );
    }
    await next();
  };
  const spa = serveStatic({ root: webDist, path: 'index.html' });
  const requireWebLogin: import('hono').MiddlewareHandler = async (c, next) => {
    if ((await access.status()).state !== 'signed_in') {
      const url = new URL(c.req.url);
      return c.redirect(`/login?returnTo=${encodeURIComponent(url.pathname + url.search)}`);
    }
    await next();
  };
  const mountedApp = app
    .route('/api/v1/system', createSystemRoutes())
    .route(
      '/api/v1/access',
      createAccessRoutes(access, (userId) => {
        if (userId) browsers.closeForUser(userId);
      }),
    )
    .route('/', createAccessCallback(access))
    .get('/healthz', (c) =>
      c.json({
        ok: true,
        service: 'onecrew_lite',
        protocol_version: 1,
        engine: 'teable-v2-pglite',
        capabilities: ['discovery-contacts-v2', 'recommendation-capture-v1'],
      }),
    )
    .get('/', (c) => c.redirect('/library'))
    .use('/api/*', requireLogin)
    .use('/api/v1/dynamic-tables', jsonLimit)
    .use('/api/v1/dynamic-tables/*', (c, next) =>
      c.req.path === '/api/v1/dynamic-tables/import' ? importLimit(c, next) : jsonLimit(c, next),
    )
    .use('/mcp', jsonLimit)
    .use('/mcp/*', jsonLimit)
    .use('/api/v1/browser', jsonLimit)
    .use('/api/v1/browser/*', jsonLimit)
    .use('/api/v1/searches', jsonLimit)
    .use('/api/v1/searches/*', jsonLimit)
    .use('/mcp', async (c, next) => {
      const origin = c.req.header('Origin');
      if (origin && !isLocalOrigin(origin, c.req.header('Host') ?? ''))
        return c.json(
          {
            error: { code: 'browser.origin', message: 'Cross-origin MCP requests are not allowed' },
          },
          403,
        );
      await next();
    })
    .use('/mcp/*', async (c, next) => {
      const origin = c.req.header('Origin');
      if (origin && !isLocalOrigin(origin, c.req.header('Host') ?? ''))
        return c.json(
          {
            error: { code: 'browser.origin', message: 'Cross-origin MCP requests are not allowed' },
          },
          403,
        );
      await next();
    })
    .use('/api/v1/searches', async (c, next) => {
      if (!isLocalOrigin(c.req.header('Origin'), c.req.header('Host') ?? ''))
        return c.json(
          { error: { code: 'browser.origin', message: 'Search requests are local only' } },
          403,
        );
      await next();
    })
    .use('/api/v1/searches/*', async (c, next) => {
      if (!isLocalOrigin(c.req.header('Origin'), c.req.header('Host') ?? ''))
        return c.json(
          { error: { code: 'browser.origin', message: 'Search requests are local only' } },
          403,
        );
      await next();
    })
    .route('/api/v1/dynamic-tables', createTableRoutes(service))
    .route('/api/v1/browser', createBrowserRoutes(browsers))
    .all('/mcp', (c) => mcp.fetch(c.req.raw))
    .all('/mcp/*', (c) => mcp.fetch(c.req.raw))
    .get('/login', spa)
    .get('/auth/complete', spa)
    .use('/library', requireWebLogin)
    .use('/library/*', requireWebLogin)
    .use('/connections', requireWebLogin)
    .get('/library', spa)
    .get('/library/*', spa)
    .get('/connections', spa)
    .use('/assets/*', serveStatic({ root: webDist }))
    .use('/onecrew-logo-on-dark.png', serveStatic({ root: webDist }))
    .notFound((c) =>
      c.req.path.startsWith('/api/') || c.req.path === '/mcp'
        ? c.json({ error: { code: 'not_found', message: 'Route not found' } }, 404)
        : c.text('Not found', 404),
    );
  return {
    app: mountedApp,
    data,
    database,
    service,
    access,
    tenants,
    browsers,
    closeMcp: mcp.close,
    resolveTenant,
  };
};

export type AppType = Awaited<ReturnType<typeof createApp>>['app'];
