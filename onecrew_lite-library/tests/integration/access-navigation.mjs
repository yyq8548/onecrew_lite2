import assert from 'node:assert/strict';
import { getLoginReturnPath } from '../../web/src/lib/router.ts';

for (const value of ['/connections', '/library', '/library?q=coffee', '/library/tbl123?q=coffee']) {
  assert.equal(getLoginReturnPath(value), value);
}
for (const value of [
  null,
  '',
  'https://example.com',
  '//example.com',
  '/\\example.com',
  'javascript:alert(1)',
  '/login',
  '/auth/complete',
  '/connections/../auth/complete',
  '/library/../../login',
  '/api/v1/access/logout',
]) {
  assert.equal(getLoginReturnPath(value), '/library', `Unsafe return location: ${value}`);
}
console.log(
  'Login return navigation preserves app routes and rejects external or looping targets.',
);
