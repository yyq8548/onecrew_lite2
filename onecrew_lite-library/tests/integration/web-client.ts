import assert from 'node:assert/strict';
import { api, request, downloadFile, setBusinessAccountId } from '../../web/src/lib/api.js';

const originalFetch = globalThis.fetch;
const windowDescriptor = Object.getOwnPropertyDescriptor(globalThis, 'window');
const events = new EventTarget();
let accessChanges = 0;
events.addEventListener('onecrew:access-changed', () => accessChanges++);
Object.defineProperty(globalThis, 'window', { value: events, configurable: true });
Object.defineProperty(events, 'location', { value: { origin: 'http://127.0.0.1:8091' } });

let respond: (input: RequestInfo | URL, init?: RequestInit) => Response = () =>
  Response.json({ state: 'signed_in', user: { id: 'test-user', email: 'test@example.com' } });
globalThis.fetch = async (input, init) => respond(input, init);

try {
  const status = await request(api.access.status.$get());
  assert.equal(status.state, 'signed_in');
  setBusinessAccountId('alice');

  for (const [httpStatus, code] of [
    [401, 'access.login_required'],
    [503, 'access.unavailable'],
  ] as const) {
    respond = () =>
      Response.json({ error: { code, message: 'Sign in again' } }, { status: httpStatus });
    await assert.rejects(request(api.access.status.$get()), /Sign in again/);
  }
  assert.equal(accessChanges, 2);
  respond = () =>
    Response.json(
      { error: { code: 'validation.input', message: 'Invalid value' } },
      { status: 400 },
    );
  await assert.rejects(request(api.access.status.$get()), /Invalid value/);
  assert.equal(accessChanges, 2, 'ordinary errors must not trigger account refresh');
  respond = () => new Response('not JSON', { status: 502 });
  await assert.rejects(request(api.access.status.$get()), /HTTP 502/);

  respond = () =>
    new Response('workbook', {
      headers: { 'Content-Disposition': "attachment; filename*=UTF-8''%E5%AF%BC%E5%87%BA.xlsx" },
    });
  const exported = await downloadFile(
    api['dynamic-tables'][':tableId'].export.$get({
      param: { tableId: 'table-1' },
      query: { locale: 'en' },
    }),
  );
  assert.equal(exported.filename, '导出.xlsx');
  assert.equal(await exported.blob.text(), 'workbook');
  respond = () =>
    Response.json(
      { error: { code: 'access.login_required', message: 'Sign in again' } },
      { status: 401 },
    );
  await assert.rejects(
    downloadFile(
      api['dynamic-tables'][':tableId'].export.$get({ param: { tableId: 'table-1' }, query: {} }),
    ),
    /Sign in again/,
  );
  assert.equal(accessChanges, 3);

  respond = (_input, init) => {
    assert.equal(new Headers(init?.headers).get('X-OneCrew-Account-Id'), 'alice');
    return Response.json({ tables: [] });
  };
  await request(api['dynamic-tables'].$get({ query: {} }));
  let finish!: (response: Response) => void;
  globalThis.fetch = () =>
    new Promise<Response>((resolve) => {
      finish = resolve;
    });
  const pending = request(api['dynamic-tables'].$get({ query: {} }));
  const rejected = assert.rejects(pending, /previous account session/);
  while (!finish) await Promise.resolve();
  setBusinessAccountId('bob');
  finish(Response.json({ tables: [{ name: 'Private Alice' }] }));
  await rejected;

  let stream!: ReadableStreamDefaultController<Uint8Array>;
  globalThis.fetch = async () =>
    new Response(
      new ReadableStream<Uint8Array>({
        start(controller) {
          stream = controller;
        },
      }),
    );
  const download = downloadFile(
    api['dynamic-tables'][':tableId'].export.$get({ param: { tableId: 'table-1' }, query: {} }),
  );
  const rejectedDownload = assert.rejects(download, /previous account session/);
  await new Promise((resolve) => setTimeout(resolve, 0));
  setBusinessAccountId('alice');
  stream.enqueue(new TextEncoder().encode('Private Bob'));
  stream.close();
  await rejectedDownload;
  globalThis.fetch = async (input, init) => respond(input, init);
  respond = () =>
    Response.json(
      { error: { code: 'access.account_changed', message: 'Account changed' } },
      { status: 409 },
    );
  await assert.rejects(request(api['dynamic-tables'].$get({ query: {} })), /Account changed/);
  assert.equal(accessChanges, 4);

  respond = (_input, init) => {
    assert.ok(init?.signal?.aborted, 'the RPC client must forward the abort signal');
    throw new DOMException('Request aborted', 'AbortError');
  };
  await assert.rejects(
    request(api.browser.status.$get(undefined, { init: { signal: AbortSignal.abort() } })),
    { name: 'AbortError' },
  );
  console.log(
    'Web client passed: inferred JSON, error messages, auth notifications, downloads and aborts',
  );
} finally {
  setBusinessAccountId(null);
  globalThis.fetch = originalFetch;
  if (windowDescriptor) Object.defineProperty(globalThis, 'window', windowDescriptor);
  else Reflect.deleteProperty(globalThis, 'window');
}
