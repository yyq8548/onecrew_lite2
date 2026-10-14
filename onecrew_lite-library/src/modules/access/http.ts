import { Hono } from 'hono';
import { AccessError } from './client.js';
import type { AccessService } from './service.js';

export function createAccessRoutes(
  service: AccessService,
  onLogout: (userId: string | null) => void = () => {},
) {
  return new Hono()
    .use('*', async (c, next) => {
      c.header('Cache-Control', 'no-store');
      const url = new URL(c.req.url);
      const origin = c.req.header('Origin');
      if (url.hostname !== '127.0.0.1' || (origin && origin !== url.origin))
        return c.json(
          { error: { code: 'access.origin', message: 'Request origin rejected' } },
          403,
        );
      if (
        c.req.method !== 'GET' &&
        (c.req.header('X-OneCrew-Request') !== '1' ||
          !c.req.header('Content-Type')?.startsWith('application/json'))
      )
        return c.json({ error: { code: 'access.request', message: 'Invalid request' } }, 403);
      await next();
    })
    .onError((error, c) =>
      c.json(
        {
          error: {
            code: error instanceof AccessError ? error.code : 'access.unavailable',
            message: 'Account operation could not be completed',
          },
        },
        error instanceof AccessError ? (error.status as 400 | 409 | 422 | 503) : 503,
      ),
    )
    .get('/status', async (c) => c.json(await service.status()))
    .post('/login', async (c) => c.json(await service.begin(Number(new URL(c.req.url).port))))
    .post('/logout', async (c) => {
      const current = await service.status();
      const status = await service.logout();
      onLogout(current.state === 'signed_in' && current.user ? current.user.id : null);
      return c.json(status);
    });
}

export function createAccessCallback(service: AccessService) {
  return new Hono().get('/creator-sync/pairing-callback', async (c) => {
    c.header('Cache-Control', 'no-store');
    c.header('Referrer-Policy', 'no-referrer');
    const url = new URL(c.req.url);
    const params = [...url.searchParams.keys()];
    if (
      url.hostname !== '127.0.0.1' ||
      params.length !== 2 ||
      !params.includes('code') ||
      !params.includes('state')
    )
      return c.text('Invalid authorization callback', 400);
    try {
      await service.complete(url.searchParams.get('code')!, url.searchParams.get('state')!);
    } catch {
      return c.text('Authorization failed. Return to OneCrew and start login again.', 400);
    }
    return c.redirect('/auth/complete', 303);
  });
}
