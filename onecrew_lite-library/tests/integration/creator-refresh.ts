import assert from 'node:assert/strict';
import { subscribeCreatorRefresh } from '../../web/src/features/tables/creator-refresh.js';

const savedWindow = Object.getOwnPropertyDescriptor(globalThis, 'window');
const savedDocument = Object.getOwnPropertyDescriptor(globalThis, 'document');
const browser = new EventTarget();
const page = new EventTarget();
let visibility = 'visible';
Object.defineProperty(page, 'visibilityState', { get: () => visibility });
Object.defineProperty(globalThis, 'window', { value: browser, configurable: true });
Object.defineProperty(globalThis, 'document', { value: page, configurable: true });
const tick = () => new Promise<void>((resolve) => setImmediate(resolve));
const sessions: {
  signal: AbortSignal;
  changed: () => void;
  finish: () => void;
  fail: () => void;
}[] = [];
let refreshes = 0;
const stop = subscribeCreatorRefresh(
  (signal, changed) =>
    new Promise<void>((resolve, reject) => {
      sessions.push({ signal, changed, finish: resolve, fail: () => reject(new Error('offline')) });
      signal.addEventListener('abort', () => resolve(), { once: true });
    }),
  () => refreshes++,
);
try {
  assert.equal(sessions.length, 1);
  await new Promise((resolve) => setTimeout(resolve, 1100));
  assert.equal(refreshes, 0, 'idle view must not reload periodically');
  sessions[0].finish();
  await tick();
  browser.dispatchEvent(new Event('focus'));
  await tick();
  assert.equal(sessions.length, 2, 'returning after EOF must reconnect updates');
  const afterFocus = refreshes;
  sessions[1].changed();
  assert.equal(refreshes, afterFocus + 1, 'reconnected stream must refresh new publications');
  browser.dispatchEvent(new Event('online'));
  browser.dispatchEvent(new Event('focus'));
  assert.equal(sessions.length, 2, 'active stream must not be duplicated');
  visibility = 'hidden';
  sessions[1].changed();
  const hiddenRefreshes = refreshes;
  sessions[1].fail();
  await tick();
  browser.dispatchEvent(new Event('online'));
  assert.equal(sessions.length, 2, 'hidden page waits until visible');
  visibility = 'visible';
  page.dispatchEvent(new Event('visibilitychange'));
  await tick();
  assert.equal(sessions.length, 3, 'visible page reconnects after rejection');
  assert.equal(refreshes, hiddenRefreshes + 1, 'hidden dirty updates are not lost to throttling');
  sessions[2].fail();
  await tick();
  browser.dispatchEvent(new Event('online'));
  await tick();
  assert.equal(sessions.length, 4, 'online event recovers disconnected visible stream');
  sessions[3].changed();
  sessions[3].fail();
  await tick();
  await new Promise((resolve) => setTimeout(resolve, 1100));
  assert.equal(sessions.length, 5, 'visible disconnected stream retries without a focus event');
  stop();
  assert.ok(sessions[4].signal.aborted);
  const stoppedRefreshes = refreshes;
  sessions[4].changed();
  browser.dispatchEvent(new Event('focus'));
  browser.dispatchEvent(new Event('online'));
  page.dispatchEvent(new Event('visibilitychange'));
  await tick();
  assert.equal(sessions.length, 5, 'unmounted view must not reconnect');
  assert.equal(refreshes, stoppedRefreshes, 'unmounted view ignores late notifications');
  console.log(
    'Creator refresh: idle, EOF/error recovery, single stream, visibility and cleanup passed.',
  );
} finally {
  stop();
  if (savedWindow) Object.defineProperty(globalThis, 'window', savedWindow);
  else Reflect.deleteProperty(globalThis, 'window');
  if (savedDocument) Object.defineProperty(globalThis, 'document', savedDocument);
  else Reflect.deleteProperty(globalThis, 'document');
}
