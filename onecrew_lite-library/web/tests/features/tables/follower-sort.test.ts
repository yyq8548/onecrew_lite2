import assert from 'node:assert/strict';
import test from 'node:test';
import { followerCount, sortByFollowers } from '../../../src/features/tables/follower-sort';

test('numeric follower order, estimates, unknowns and separate audiences', () => {
  const rows = [
    { id: 'unknown', accounts: [{ followers: null }] },
    { id: 'small', accounts: [{ followers: '900' }] },
    { id: 'large', accounts: [{ followers: '~1.2万' }] },
    { id: 'multi', accounts: [{ followers: '8000' }, { followers: '7000' }] },
  ];
  assert.deepEqual(sortByFollowers(rows).map(r => r.id), ['large', 'multi', 'small', 'unknown']);
  assert.deepEqual(sortByFollowers(rows, true).map(r => r.id), ['small', 'multi', 'large', 'unknown']);
  assert.equal(rows[0].id, 'unknown');
  assert.equal(followerCount('~4.9M'), 4900000);
  assert.equal(followerCount('10,429'), 10429);
  assert.equal(followerCount('unknown'), null);
});
