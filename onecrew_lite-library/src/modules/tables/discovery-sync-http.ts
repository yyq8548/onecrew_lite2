import { timingSafeEqual } from 'node:crypto';
import { Hono } from 'hono';
import type { Context } from 'hono';
import { bodyLimit } from 'hono/body-limit';
import type { TenantContext } from '../tenancy/service.js';
import type { TableData } from './data.js';
import { notifyDiscoveryChanged } from './discovery-events.js';
import { discoveryIdentitySchema, discoveryIdentityReadSchema } from './discovery-identity.js';
import {
  discoveryIngestSchema,
  discoveryStateSchema,
  discoveryCampaignSchema,
  discoveryTableSchema,
} from './discovery-projection.js';

function failure(c: Context, error: unknown) {
  const details = error as { status?: number; code?: string } | undefined;
  const status = details?.status;
  if (
    status === 400 ||
    status === 401 ||
    status === 403 ||
    status === 404 ||
    status === 409 ||
    status === 503
  )
    return c.json({ error: { code: details?.code ?? 'discovery.sync_failed' } }, status);
  return c.json({ error: { code: 'discovery.sync_failed' } }, 500);
}

/** Private local producer route, deliberately separate from Web/MCP table editing. */
export function createDiscoverySyncRoutes(
  data: TableData,
  resolveTenant: (request: Request) => Promise<TenantContext>,
  controlToken: string,
) {
  if (!/^[0-9a-f]{64}$/.test(controlToken)) throw new Error('Invalid local control token');
  const expected = Buffer.from(`Bearer ${controlToken}`);
  return new Hono()
    .use('*', async (c, next) => {
      const supplied = Buffer.from(c.req.header('Authorization') ?? '');
      if (
        c.req.header('Origin') !== undefined ||
        supplied.length !== expected.length ||
        !timingSafeEqual(supplied, expected)
      )
        return c.json({ error: { code: 'discovery.producer_required' } }, 403);
      const account = c.req.header('X-OneCrew-Account-Id');
      if (!account || account !== account.trim())
        return c.json({ error: { code: 'access.expected_account_required' } }, 400);
      c.header('Cache-Control', 'no-store');
      await next();
    })
    .post(
      '/_local/discovery/campaign',
      bodyLimit({
        maxSize: 64 * 1024,
        onError: (c) => c.json({ error: { code: 'request.too_large' } }, 413),
      }),
      async (c) => {
        let input: unknown;
        if (!c.req.header('Content-Type')?.toLowerCase().startsWith('application/json'))
          return c.json({ error: { code: 'validation.content_type' } }, 400);
        try {
          input = await c.req.json();
        } catch {
          return c.json({ error: { code: 'validation.input' } }, 400);
        }
        const parsed = discoveryTableSchema.safeParse(input);
        if (!parsed.success) return c.json({ error: { code: 'data.projection_invalid' } }, 400);
        try {
          const ctx = await resolveTenant(c.req.raw);
          if (ctx.user.id !== c.req.header('X-OneCrew-Account-Id'))
            return c.json({ error: { code: 'access.account_changed' } }, 409);
          return c.json(await data.ensureDiscoveryCampaignTable(ctx, parsed.data));
        } catch (error) {
          return failure(c, error);
        }
      },
    )
    .get('/_local/discovery/campaign', async (c) => {
      const parsed = discoveryCampaignSchema.safeParse(c.req.query());
      if (!parsed.success) return c.json({ error: { code: 'data.projection_invalid' } }, 400);
      try {
        const ctx = await resolveTenant(c.req.raw);
        if (ctx.user.id !== c.req.header('X-OneCrew-Account-Id'))
          return c.json({ error: { code: 'access.account_changed' } }, 409);
        return c.json({
          table: await data.getDiscoveryCampaignTable(ctx, parsed.data.campaign_id),
        });
      } catch (error) {
        return failure(c, error);
      }
    })
    .get('/_local/discovery/state', async (c) => {
      const parsed = discoveryStateSchema.safeParse(c.req.query());
      if (!parsed.success) return c.json({ error: { code: 'data.projection_invalid' } }, 400);
      try {
        const ctx = await resolveTenant(c.req.raw);
        if (ctx.user.id !== c.req.header('X-OneCrew-Account-Id'))
          return c.json({ error: { code: 'access.account_changed' } }, 409);
        return c.json(await data.getDiscoveryState(ctx, parsed.data));
      } catch (error) {
        return failure(c, error);
      }
    })
    .get('/_local/discovery/identity', async (c) => {
      const parsed = discoveryIdentityReadSchema.safeParse(c.req.query());
      if (!parsed.success) return c.json({ error: { code: 'data.identity_invalid' } }, 400);
      try {
        const ctx = await resolveTenant(c.req.raw);
        if (ctx.user.id !== c.req.header('X-OneCrew-Account-Id'))
          return c.json({ error: { code: 'access.account_changed' } }, 409);
        return c.json(await data.getDiscoveryIdentity(ctx, parsed.data));
      } catch (error) {
        return failure(c, error);
      }
    })
    .post(
      '/_local/discovery/identity',
      bodyLimit({
        maxSize: 16 * 1024 * 1024,
        onError: (c) => c.json({ error: { code: 'request.too_large' } }, 413),
      }),
      async (c) => {
        if (!c.req.header('Content-Type')?.toLowerCase().startsWith('application/json'))
          return c.json({ error: { code: 'validation.content_type' } }, 400);
        let input: unknown;
        try {
          input = await c.req.json();
        } catch {
          return c.json({ error: { code: 'validation.input' } }, 400);
        }
        const parsed = discoveryIdentitySchema.safeParse(input);
        if (!parsed.success) return c.json({ error: { code: 'data.identity_invalid' } }, 400);
        try {
          const ctx = await resolveTenant(c.req.raw);
          if (ctx.user.id !== c.req.header('X-OneCrew-Account-Id'))
            return c.json({ error: { code: 'access.account_changed' } }, 409);
          const result = await data.publishDiscoveryIdentity(ctx, parsed.data);
          notifyDiscoveryChanged(ctx.tenantId, parsed.data.table_id);
          return c.json(result);
        } catch (error) {
          return failure(c, error);
        }
      },
    )
    .post(
      '/_local/discovery/ingest',
      bodyLimit({
        maxSize: 2 * 1024 * 1024,
        onError: (c) => c.json({ error: { code: 'request.too_large' } }, 413),
      }),
      async (c) => {
        let input: unknown;
        try {
          if (!c.req.header('Content-Type')?.toLowerCase().startsWith('application/json'))
            return c.json({ error: { code: 'validation.content_type' } }, 400);
          input = await c.req.json();
        } catch {
          return c.json({ error: { code: 'validation.input' } }, 400);
        }
        const parsed = discoveryIngestSchema.safeParse(input);
        if (!parsed.success) return c.json({ error: { code: 'data.projection_invalid' } }, 400);
        try {
          const ctx = await resolveTenant(c.req.raw);
          // Defense in depth: a custom resolver cannot omit the expected-account check.
          if (ctx.user.id !== c.req.header('X-OneCrew-Account-Id'))
            return c.json({ error: { code: 'access.account_changed' } }, 409);
          return c.json(await data.ingestDiscoverySources(ctx, parsed.data));
        } catch (error) {
          return failure(c, error);
        }
      },
    );
}
