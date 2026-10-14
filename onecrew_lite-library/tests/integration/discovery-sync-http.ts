import assert from 'node:assert/strict';
import { contactDisplay, isSocialProfile } from '@onecrew/contracts/tables';
import { randomBytes } from 'node:crypto';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { createApp } from '../../src/app.js';
import { createDiscoverySyncRoutes } from '../../src/modules/tables/discovery-sync-http.js';
import { TestAccess } from '../helpers/test-access.js';

const root = await mkdtemp(path.join(os.tmpdir(), 'onecrew-discovery-http-'));
const access = new TestAccess({ id: 'http-producer-alice', email: 'alice@example.test' });
const runtime = await createApp(root, access);
try {
  const health = await (await runtime.app.request('/healthz')).json();
  assert.deepEqual(health.capabilities, ['discovery-contacts-v2', 'recommendation-capture-v1']);
  const token = randomBytes(32).toString('hex');
  const routes = createDiscoverySyncRoutes(runtime.data, runtime.resolveTenant, token);
  const ctx = await runtime.tenants.resolve(access.user);
  const table = await runtime.data.createTable(ctx, 'Source batch', [
    { name: 'Notes', data_type: 'string' },
  ]);
  const input = {
    table_id: table.id,
    campaign_id: 'http-source',
    criteria_revision: 1,
    source_revision: 1,
    rows: [
      {
        search_hit_id: 'hit',
        candidate_id: null,
        native_author_id: null,
        verdict: 'unverified',
        reasons: ['No bound identity'],
        evidence_version: 'a'.repeat(64),
        values: { [table.columns[0].id]: 'initial' },
      },
    ],
  };
  const headers = {
    Authorization: `Bearer ${token}`,
    'X-OneCrew-Account-Id': access.user.id,
    'Content-Type': 'application/json',
  };
  const url = 'http://127.0.0.1/_local/discovery/ingest';

  // Native Douyin profile contacts must pass the same producer endpoint as TikTok.
  const author = 'MS4wLjABAAAAabcdefghijklmnopqrstuvwxyz0123456789';
  const profile = `https://www.douyin.com/user/${author}`;
  const contact = {
    kind: 'social_profile', original_value: profile, normalized_value: profile,
    source_url: profile, source_excerpt: '作者公开主页', captured_at: 1791891841,
    native_author_id: author, attribution: 'author_published',
  };
  const douyinTable = await runtime.data.createTable(ctx, 'Douyin contact regression', [
    { name: 'Notes', data_type: 'string' },
  ]);
  const douyinInput = {
    table_id: douyinTable.id, campaign_id: 'douyin-contact-regression',
    criteria_revision: 1, source_revision: 1,
    rows: [{ search_hit_id: 'douyin-hit', candidate_id: 'douyin-candidate',
      native_author_id: author, verdict: 'qualified', reasons: ['Matches requested topic'],
      evidence_version: 'd'.repeat(64), contacts: [contact],
      values: { [douyinTable.columns[0].id]: 'verified' } }],
  };
  const postDouyin = (value = profile) => routes.fetch(new Request(url, {
    method: 'POST', headers,
    body: JSON.stringify({ ...douyinInput, rows: [{ ...douyinInput.rows[0],
      contacts: [{ ...contact, original_value: value, normalized_value: value }] }] }),
  }));
  const acceptedDouyin = await postDouyin();
  assert.equal(acceptedDouyin.status, 200, 'Native Douyin profile must sync successfully');
  assert.equal((await postDouyin()).status, 200, 'Retry must be idempotent');
  const savedDouyin = await (await routes.fetch(new Request(
    `http://127.0.0.1/_local/discovery/state?table_id=${douyinTable.id}&campaign_id=douyin-contact-regression&limit=10`,
    { headers },
  ))).json();
  assert.deepEqual(savedDouyin.rows[0].contacts, [contact]);
  assert.equal(contactDisplay(contact), `抖音主页：${profile}`);
  assert.equal(isSocialProfile('https://www.tiktok.com/@example'), true);
  for (const invalid of [
    'https://www.douyin.com/video/123456789', 'https://www.douyin.com/user/editable_handle',
    `${profile}?from=search`, `${profile}#detail`, profile.replace('douyin.com', 'douyin.com.evil.test'),
    profile.replace('https:', 'http:'), profile.replace('www.', 'user:pass@www.'),
    `https://www.douyin.com/user/MS4wLjABAAAA${'a'.repeat(89)}`,
  ]) {
    assert.equal(isSocialProfile(invalid), false);
    assert.equal((await postDouyin(invalid)).status, 400, 'Invalid profile must remain rejected');
  }

  const createUrl = 'http://127.0.0.1/_local/discovery/campaign';
  const createBody = JSON.stringify({
    campaign_id: 'http-managed',
    name: 'OneCrew Sources http-managed',
    columns: [{ name: 'Notes', data_type: 'string' }],
  });
  const createRequest = (overrides: Record<string, string> = {}, body = createBody) =>
    new Request(createUrl, { method: 'POST', headers: { ...headers, ...overrides }, body });
  assert.equal((await routes.fetch(createRequest({ Authorization: '' }))).status, 403);
  assert.equal((await routes.fetch(createRequest({ Origin: 'http://127.0.0.1' }))).status, 403);
  assert.equal(
    (await routes.fetch(createRequest({ 'X-OneCrew-Account-Id': 'other' }))).status,
    409,
  );
  assert.equal((await routes.fetch(createRequest({ 'Content-Type': 'text/plain' }))).status, 400);
  assert.equal((await routes.fetch(createRequest({}, '{bad'))).status, 400);
  assert.equal(
    (await routes.fetch(createRequest({}, JSON.stringify({ extra: 'x'.repeat(65536) })))).status,
    413,
  );
  assert.equal((await runtime.app.fetch(createRequest())).status, 404);
  const created = await routes.fetch(createRequest());
  assert.equal(created.status, 200);
  const managed = (await created.json()) as { id: string };
  const managedRetry = await routes.fetch(createRequest());
  assert.equal(managedRetry.status, 200);
  assert.equal(((await managedRetry.json()) as { id: string }).id, managed.id);
  const namedBody = JSON.stringify({
    campaign_id: 'http-managed',
    name: '虚拟货币_1002_1',
    columns: [{ name: 'Notes', data_type: 'string' }],
  });
  const renamed = await routes.fetch(createRequest({}, namedBody));
  assert.equal(renamed.status, 200);
  const renamedTable = (await renamed.json()) as { id: string; name: string };
  assert.equal(renamedTable.id, managed.id);
  assert.equal(renamedTable.name, '虚拟货币_1002_1');
  const repeated = await routes.fetch(createRequest({}, namedBody));
  assert.equal(((await repeated.json()) as { id: string }).id, managed.id);
  const laterName = await routes.fetch(createRequest({}, namedBody.replace('1002_1', '1003_9')));
  assert.equal(((await laterName.json()) as { name: string }).name, '虚拟货币_1002_1');
  const oldAutoName = '我要卖键盘，找键盘测评达人_1002_1';
  const productBody = { campaign_id: 'http-product', name: oldAutoName,
    columns: [{ name: 'Notes', data_type: 'string' }] };
  const productTable = await (await routes.fetch(createRequest({}, JSON.stringify(productBody)))).json() as { id: string };
  const conciseBody = JSON.stringify({ ...productBody, name: '键盘_1002_1', previous_auto_name: oldAutoName });
  const concise = await routes.fetch(createRequest({}, conciseBody));
  assert.equal(concise.status, 200);
  const conciseTable = await concise.json() as { id: string; name: string };
  assert.equal(conciseTable.id, productTable.id);
  assert.equal(conciseTable.name, '键盘_1002_1');
  const repeatedConcise = await (await routes.fetch(createRequest({}, conciseBody))).json() as { id: string; name: string };
  assert.equal(repeatedConcise.id, productTable.id);
  assert.equal(repeatedConcise.name, '键盘_1002_1');
  const protectedName = await (await routes.fetch(createRequest({}, JSON.stringify({
    ...productBody, name: '错误覆盖', previous_auto_name: oldAutoName,
  })))).json() as { name: string };
  assert.equal(protectedName.name, '键盘_1002_1');
  const campaignUrl = 'http://127.0.0.1/_local/discovery/campaign?campaign_id=http-source';
  assert.deepEqual(await (await routes.fetch(new Request(campaignUrl, { headers }))).json(), {
    table: null,
  });
  const request = (overrides: Record<string, string> = {}, body = JSON.stringify(input)) =>
    new Request(url, { method: 'POST', headers: { ...headers, ...overrides }, body });
  assert.equal((await routes.fetch(request({ Authorization: '' }))).status, 403);
  assert.equal((await routes.fetch(request({ Authorization: 'Bearer invalid' }))).status, 403);
  for (const Origin of ['http://127.0.0.1:8091', 'https://evil.example', 'null', ''])
    assert.equal((await routes.fetch(request({ Origin }))).status, 403);
  assert.equal((await routes.fetch(request({ 'X-OneCrew-Account-Id': '' }))).status, 400);
  assert.equal(
    (await routes.fetch(request({ 'X-OneCrew-Account-Id': 'someone-else' }))).status,
    409,
  );
  assert.equal((await routes.fetch(request({ 'Content-Type': 'text/plain' }))).status, 400);
  assert.equal((await routes.fetch(request({}, '{invalid'))).status, 400);
  assert.equal(
    (
      await routes.fetch(
        request(
          {},
          JSON.stringify({
            ...input,
            rows: [{ ...input.rows[0], row_id: 'forged-destination' }],
          }),
        ),
      )
    ).status,
    400,
  );
  const oversized = JSON.stringify({ extra: 'x'.repeat(2 * 1024 * 1024) });
  assert.equal((await routes.fetch(request({}, oversized))).status, 413);
  assert.equal(
    (await runtime.app.fetch(request())).status,
    404,
    'ordinary app must not expose producer route',
  );
  assert.equal((await runtime.data.queryRows(ctx, table.id, 1, 10)).total, 0);
  access.state = 'signed_out';
  assert.equal((await routes.fetch(request())).status, 401);
  access.state = 'signed_in';
  const response = await routes.fetch(request());
  assert.equal(response.status, 200);
  assert.equal(response.headers.get('Cache-Control'), 'no-store');
  const result = (await response.json()) as {
    inserted_rows: number;
    rows: Array<{ row_id: string }>;
  };
  assert.equal(result.inserted_rows, 1);
  const lookup = (await (await routes.fetch(new Request(campaignUrl, { headers }))).json()) as {
    table: { id: string };
  };
  assert.equal(lookup.table.id, table.id);
  const stateUrl = `http://127.0.0.1/_local/discovery/state?table_id=${table.id}&campaign_id=http-source&limit=1`;
  assert.equal((await routes.fetch(new Request(stateUrl))).status, 403);
  const stateResponse = await routes.fetch(new Request(stateUrl, { headers }));
  assert.equal(stateResponse.status, 200);
  const state = (await stateResponse.json()) as {
    bound: boolean;
    rows: Array<{ row_id: string; source_revision: number }>;
  };
  assert.equal(state.bound, true);
  assert.equal(state.rows[0].row_id, result.rows[0].row_id);
  assert.equal(state.rows[0].source_revision, 1);
  await runtime.data.updateRows(ctx, table.id, [
    { id: result.rows[0].row_id, values: { [table.columns[0].id]: 'customer note' } },
  ]);
  const retry = await routes.fetch(request());
  assert.equal(retry.status, 200);
  assert.equal(((await retry.json()) as { inserted_rows: number }).inserted_rows, 0);
  assert.equal(
    (await runtime.data.queryRows(ctx, table.id, 1, 10)).rows[0].values[table.columns[0].id],
    'customer note',
  );
  const identityUrl = 'http://127.0.0.1/_local/discovery/identity';
  const identity = {
    table_id: table.id,
    campaign_id: input.campaign_id,
    projection_format: 1,
    criteria_revision: 1,
    source_revision: 1,
    source_hash: 'b'.repeat(64),
    identity_revision: 1,
    identity_hash: 'c'.repeat(64),
    source_rows: [
      {
        search_hit_id: 'hit',
        candidate_id: null,
        native_author_id: null,
        evidence_version: 'a'.repeat(64),
      },
    ],
    groups: [],
  };
  const identityRequest = (
    overrides: Record<string, string> = {},
    body = JSON.stringify(identity),
  ) => new Request(identityUrl, { method: 'POST', headers: { ...headers, ...overrides }, body });
  const readIdentity = () =>
    routes.fetch(
      new Request(`${identityUrl}?table_id=${table.id}&campaign_id=${input.campaign_id}`, {
        headers,
      }),
    );
  assert.deepEqual(await (await readIdentity()).json(), { state: 'missing', projection: null });
  assert.equal((await routes.fetch(identityRequest({ Authorization: '' }))).status, 403);
  assert.equal(
    (await routes.fetch(identityRequest({ Origin: 'http://127.0.0.1:8091' }))).status,
    403,
  );
  assert.equal(
    (await routes.fetch(identityRequest({ 'X-OneCrew-Account-Id': 'other' }))).status,
    409,
  );
  assert.equal((await routes.fetch(identityRequest({ 'Content-Type': 'text/plain' }))).status, 400);
  assert.equal((await routes.fetch(identityRequest({}, '{bad'))).status, 400);
  assert.equal(
    (
      await routes.fetch(
        identityRequest({}, JSON.stringify({ ...identity, groups: [{ forged: true }] })),
      )
    ).status,
    400,
  );
  assert.equal((await runtime.app.fetch(identityRequest())).status, 404);
  access.state = 'signed_out';
  assert.equal((await routes.fetch(identityRequest())).status, 401);
  access.state = 'signed_in';
  for (let replay = 0; replay < 2; replay++) {
    const published = await routes.fetch(identityRequest());
    assert.equal(published.status, 200, JSON.stringify(await published.clone().json()));
    assert.deepEqual(await published.json(), { state: 'current', projection: identity });
  }
  const identityReadback = await readIdentity();
  assert.equal(identityReadback.headers.get('Cache-Control'), 'no-store');
  assert.deepEqual(await identityReadback.json(), { state: 'current', projection: identity });
  const creatorUrl = `http://127.0.0.1/api/v1/dynamic-tables/${table.id}/creators`;
  const creatorRequest = (overrides: Record<string, string> = {}) =>
    runtime.app.fetch(
      new Request(creatorUrl, {
        headers: {
          Host: '127.0.0.1',
          Origin: 'http://127.0.0.1',
          'X-OneCrew-Account-Id': access.user.id,
          ...overrides,
        },
      }),
    );
  access.state = 'signed_out';
  assert.equal((await creatorRequest()).status, 401);
  access.state = 'signed_in';
  assert.equal((await creatorRequest({ 'X-OneCrew-Account-Id': 'other' })).status, 409);
  const creatorResponse = await creatorRequest();
  assert.equal(creatorResponse.status, 200);
  assert.equal(creatorResponse.headers.get('Cache-Control'), 'no-store');
  const creatorData = (await creatorResponse.json()) as {
    state: string;
    sources: Array<{ row: { values: Record<string, unknown> } }>;
  };
  assert.equal(creatorData.state, 'current');
  const captureBody = {version_id:'d9a351fd-94f5-48b4-ae80-df503d41f10b', source_instance_id:'cd7d4b88-d3c6-47ab-b901-e4eaf164bb4e', consent_id:'5ec2446c-0b80-4b7c-a5c5-503c077390de', capture_started_at:'2026-10-09T00:00:00.000Z'};
  const captureRequest = (body: unknown = captureBody) => runtime.app.fetch(new Request(
    `http://127.0.0.1/api/v1/dynamic-tables/${table.id}/recommendation-capture`, {
      method:'POST', headers:{Host:'127.0.0.1',Origin:'http://127.0.0.1','Content-Type':'application/json','X-OneCrew-Account-Id':access.user.id}, body:JSON.stringify(body),
    }));
  assert.equal((await captureRequest({...captureBody, unexpected:true})).status,400);
  const captured = await captureRequest(); assert.equal(captured.status,200);
  assert.equal(captured.headers.get('Cache-Control'),'no-store');
  const full = await captured.json() as {archive:string;record_count:number;table_id:string};
  assert.equal(full.table_id,table.id); assert.equal(full.record_count,0);
  assert.equal(full.archive.trim().split('\n').length,2);
  access.state='signed_out';assert.equal((await captureRequest()).status,401);access.state='signed_in';

  assert.equal(creatorData.sources[0].row.values[table.columns[0].id], 'customer note');
  assert.deepEqual(
    await (
      await runtime.app.fetch(
        new Request(`http://127.0.0.1/api/v1/dynamic-tables/${managed.id}/creators`, {
          headers: { 'X-OneCrew-Account-Id': access.user.id },
        }),
      )
    ).json(),
    { state: 'missing', projection: null, sources: [], recommendations: [] },
  );
  assert.equal(
    (
      await runtime.app.fetch(
        new Request(creatorUrl, { method: 'POST', headers, body: JSON.stringify(identity) }),
      )
    ).status,
    404,
  );
  // A later identity decision cannot replace the same source version's hash.
  const conflictingSource = await routes.fetch(
    identityRequest(
      {},
      JSON.stringify({ ...identity, identity_revision: 2, source_hash: 'f'.repeat(64) }),
    ),
  );
  assert.equal(conflictingSource.status, 409);
  assert.deepEqual(await (await readIdentity()).json(), {
    state: 'current',
    projection: identity,
  });
  assert.equal(
    (
      await routes.fetch(
        identityRequest({}, JSON.stringify({ ...identity, identity_hash: 'd'.repeat(64) })),
      )
    ).status,
    409,
  );
  assert.equal(
    (
      await routes.fetch(
        identityRequest({}, JSON.stringify({ ...identity, source_rows: [], identity_revision: 2 })),
      )
    ).status,
    409,
  );
  const newerSources = await routes.fetch(
    request({}, JSON.stringify({ ...input, source_revision: 2 })),
  );
  assert.equal(newerSources.status, 200);
  assert.deepEqual(await (await readIdentity()).json(), {
    state: 'sources_changed',
    projection: null,
  });
  assert.equal((await routes.fetch(identityRequest())).status, 409);
  const newerIdentity = {
    ...identity,
    source_revision: 2,
    identity_revision: 2,
    source_hash: 'd'.repeat(64),
  };
  assert.equal(
    (await routes.fetch(identityRequest({}, JSON.stringify(newerIdentity)))).status,
    200,
  );
  assert.deepEqual(await (await readIdentity()).json(), {
    state: 'current',
    projection: newerIdentity,
  });
  assert.equal(
    (await runtime.data.queryRows(ctx, table.id, 1, 10)).rows[0].values[table.columns[0].id],
    'customer note',
  );
  const boundRow = {
    ...input.rows[0],
    candidate_id: 'candidate',
    native_author_id: 'channel',
    verdict: 'qualified',
    reasons: [],
    evidence_version: 'e'.repeat(64),
  };
  assert.equal(
    (
      await routes.fetch(
        request({}, JSON.stringify({ ...input, source_revision: 3, rows: [boundRow] })),
      )
    ).status,
    200,
  );
  const boundIdentity = {
    ...newerIdentity,
    source_revision: 3,
    identity_revision: 3,
    source_hash: 'e'.repeat(64),
    identity_hash: 'f'.repeat(64),
    source_rows: [
      {
        search_hit_id: 'hit',
        candidate_id: 'candidate',
        native_author_id: 'channel',
        evidence_version: 'e'.repeat(64),
      },
    ],
    groups: [
      {
        group_id: 'f'.repeat(64),
        has_same_author_review: false,
        decision_ids: [],
        members: [
          {
            candidate_id: 'candidate',
            native_author_id: 'channel',
            platform: 'youtube',
            display_name: 'Creator',
            profile_url: 'https://www.youtube.com/channel/channel',
            source_hit_ids: ['hit'],
          },
        ],
      },
    ],
  };
  assert.equal(
    (await routes.fetch(identityRequest({}, JSON.stringify(boundIdentity)))).status,
    200,
  );
  assert.deepEqual(await (await readIdentity()).json(), {
    state: 'current',
    projection: boundIdentity,
  });
  const boundView = (await (await creatorRequest()).json()) as {
    projection: typeof boundIdentity;
    sources: Array<{
      search_hit_id: string;
      verdict: string;
      reasons: string[];
      row: { values: Record<string, unknown> };
    }>;
  };
  assert.deepEqual(boundView.projection, boundIdentity);
  assert.equal(boundView.sources[0].search_hit_id, 'hit');
  assert.equal(boundView.sources[0].verdict, 'qualified');
  assert.deepEqual(boundView.sources[0].reasons, []);
  assert.equal(boundView.sources[0].row.values[table.columns[0].id], 'customer note');
  await runtime.data.deleteRows(ctx, table.id, [result.rows[0].row_id]);
  assert.deepEqual(await (await readIdentity()).json(), {
    state: 'sources_changed',
    projection: null,
  });
  assert.deepEqual(await (await creatorRequest()).json(), {
    state: 'sources_changed',
    projection: null,
    sources: [],
    recommendations: [],
  });
  assert.equal(
    (await routes.fetch(identityRequest({}, JSON.stringify(boundIdentity)))).status,
    409,
  );
  assert.equal(
    (
      await routes.fetch(
        identityRequest({}, JSON.stringify({ oversized: 'x'.repeat(16 * 1024 * 1024) })),
      )
    ).status,
    413,
  );
  access.user = { id: 'http-producer-bob', email: 'bob@example.test' };
  assert.equal((await captureRequest()).status,404,'capture cannot read another tenant table');
  assert.equal(
    (await creatorRequest()).status,
    404,
    'browser identity reads must preserve tenant isolation',
  );
  assert.equal(
    (await routes.fetch(identityRequest({ 'X-OneCrew-Account-Id': access.user.id }))).status,
    404,
  );
  assert.deepEqual(
    await (
      await routes.fetch(
        new Request(campaignUrl, {
          headers: {
            ...headers,
            'X-OneCrew-Account-Id': access.user.id,
          },
        }),
      )
    ).json(),
    { table: null },
  );
  assert.equal((await routes.fetch(request())).status, 409);
  assert.equal(
    (await routes.fetch(request({ 'X-OneCrew-Account-Id': access.user.id }))).status,
    404,
  );
  const rotated = createDiscoverySyncRoutes(
    runtime.data,
    runtime.resolveTenant,
    randomBytes(32).toString('hex'),
  );
  assert.equal((await rotated.fetch(request())).status, 403);
  console.log(
    'Discovery HTTP: producer/account/origin/size protection, identity publication/readback/replay, source invalidation, deleted records and tenant isolation PASS',
  );
} finally {
  await runtime.closeMcp();
  runtime.browsers.closeAll();
  await runtime.data.close();
  await rm(root, { recursive: true, force: true });
}
