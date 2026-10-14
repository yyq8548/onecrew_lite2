import { Hono } from 'hono';
import type { Context } from 'hono';
import { zValidator } from '@hono/zod-validator';
import type { ClientErrorStatusCode, ServerErrorStatusCode } from 'hono/utils/http-status';
import {
  BrowserBridgeError,
  BrowserBridgeRegistry,
  BrowserBridgeService,
  extensionOrigin,
  isLocalOrigin,
} from './service.js';
import { connectSchema, resultSchema } from '@onecrew/contracts/browser';
import type { TenantHttpEnv } from '../tenancy/service.js';

type ErrorStatusCode = ClientErrorStatusCode | ServerErrorStatusCode;
const statusCode = (value: unknown): ErrorStatusCode => {
  const status = typeof value === 'number' ? value : Number(value);
  return Number.isInteger(status) && status >= 400 && status <= 599
    ? (status as ErrorStatusCode)
    : 400;
};
const fail = (c: Context, error: unknown) => {
  const status =
    error && typeof error === 'object' && 'status' in error ? statusCode(error.status) : 400;
  return c.json(
    {
      error: {
        code: error instanceof BrowserBridgeError ? error.code : 'validation.input',
        message: error instanceof Error ? error.message : 'Invalid request',
      },
    },
    status,
  );
};
const bearer = (c: Context) => c.req.header('Authorization')?.replace(/^Bearer\s+/i, '');
const local = (c: Context) => isLocalOrigin(c.req.header('Origin'), c.req.header('Host') ?? '');
const extensionAuth = (c: Context, service: BrowserBridgeService) =>
  service.authenticate(bearer(c), c.req.header('X-OneCrew-Extension-Id'), c.req.header('Origin'));

export const createBrowserRoutes = (browsers: BrowserBridgeRegistry) => {
  return new Hono<TenantHttpEnv>()
    .use('*', async (c, next) => {
      const origin = c.req.header('Origin');
      const id = c.req.header('X-OneCrew-Extension-Id');
      if (c.req.method === 'OPTIONS') {
        if (!origin || !/^chrome-extension:\/\/[a-p]{32}$/.test(origin)) return c.body(null, 403);
        c.header('Access-Control-Allow-Origin', origin);
        c.header(
          'Access-Control-Allow-Headers',
          'Authorization, Content-Type, X-OneCrew-Extension-Id',
        );
        c.header('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
        return c.body(null, 204);
      }
      await next();
      if (origin && /^chrome-extension:\/\//.test(origin) && origin === extensionOrigin(id ?? ''))
        c.header('Access-Control-Allow-Origin', origin);
    })
    .post(
      '/connect',
      zValidator('json', connectSchema, (result, c) => {
        if (!result.success)
          return c.json(
            { error: { code: 'validation.input', message: 'Invalid extension id' } },
            400,
          );
      }),
      async (c) => {
        try {
          const service = browsers.get(c.get('tenant'));
          const origin = c.req.header('Origin');
          const id = c.req.valid('json').extension_id;
          if (!/^(?:localhost|127\.0\.0\.1):\d+$/.test(c.req.header('Host') ?? ''))
            return fail(
              c,
              new BrowserBridgeError('browser service is local only', 403, 'browser.origin'),
            );
          if (c.req.header('X-OneCrew-Extension-Id') !== id)
            return fail(
              c,
              new BrowserBridgeError('extension id header mismatch', 403, 'browser.origin'),
            );
          if (origin && origin !== extensionOrigin(id))
            return fail(
              c,
              new BrowserBridgeError('extension origin is not allowed', 403, 'browser.origin'),
            );
          return c.json(service.connect(id, origin));
        } catch (e) {
          return fail(c, e);
        }
      },
    )
    .get('/status', async (c) => {
      if (!local(c))
        return fail(
          c,
          new BrowserBridgeError(
            'status is only available from the local page',
            403,
            'browser.origin',
          ),
        );
      return c.json(browsers.get(c.get('tenant')).status());
    })
    .get('/commands', async (c) => {
      try {
        const service = browsers.get(c.get('tenant'));
        const session = extensionAuth(c, service);
        return c.json({
          command: await service.nextCommand(session, c.req.raw.signal),
        });
      } catch (e) {
        return fail(c, e);
      }
    })
    .post('/heartbeat', async (c) => {
      try {
        const service = browsers.get(c.get('tenant'));
        return c.json(
          service.heartbeat(
            bearer(c),
            c.req.header('X-OneCrew-Extension-Id'),
            c.req.header('Origin'),
          ),
        );
      } catch (e) {
        return fail(c, e);
      }
    })
    .post(
      '/results',
      zValidator('json', resultSchema, (result, c) => {
        if (!result.success)
          return c.json(
            {
              error: {
                code: 'validation.input',
                message: 'Invalid browser result',
              },
            },
            400,
          );
      }),
      async (c) => {
        try {
          const service = browsers.get(c.get('tenant'));
          extensionAuth(c, service);
          return c.json(service.submitResult(c.req.valid('json')));
        } catch (e) {
          return fail(c, e);
        }
      },
    );
};
