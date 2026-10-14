import assert from 'node:assert/strict';
import {
  discoveryIdentitySchema,
  identitySourceConflict,
  identityVersionConflict,
} from '../../src/modules/tables/discovery-identity.js';
import type { ProjectionRow } from '../../src/modules/tables/discovery-projection.js';

const fixture = () => ({
  table_id: 'table',
  campaign_id: 'campaign',
  projection_format: 1 as const,
  criteria_revision: 1,
  source_revision: 4,
  source_hash: 'a'.repeat(64),
  identity_revision: 2,
  identity_hash: 'b'.repeat(64),
  source_rows: [
    {
      search_hit_id: 'youtube-hit',
      candidate_id: 'candidate-y',
      native_author_id: 'channel',
      evidence_version: 'c'.repeat(64),
    },
    {
      search_hit_id: 'x-hit',
      candidate_id: 'candidate-x',
      native_author_id: '123',
      evidence_version: 'd'.repeat(64),
    },
    {
      search_hit_id: 'unbound',
      candidate_id: null,
      native_author_id: null,
      evidence_version: 'e'.repeat(64),
    },
  ],
  groups: [
    {
      group_id: 'f'.repeat(64),
      has_same_author_review: true,
      decision_ids: ['review'],
      members: [
        {
          candidate_id: 'candidate-y',
          platform: 'youtube',
          native_author_id: 'channel',
          display_name: 'Author',
          profile_url: 'https://www.youtube.com/channel/channel',
          source_hit_ids: ['youtube-hit'],
        },
        {
          candidate_id: 'candidate-x',
          platform: 'twitter',
          native_author_id: '123',
          display_name: 'Author',
          profile_url: 'https://x.com/author',
          source_hit_ids: ['x-hit'],
        },
      ],
    },
  ],
});
const input = discoveryIdentitySchema.parse(fixture());
const rows: ProjectionRow[] = input.source_rows.map((source, index) => ({
  ...source,
  row_id: `row-${index}`,
  criteria_revision: 1,
  source_revision: 4,
  verdict: source.candidate_id ? 'qualified' : 'unverified',
  reasons: [],
  contacts: [],
}));
assert.equal(identitySourceConflict(input, rows), null);
assert.equal(identitySourceConflict(input, rows.slice(0, 2)), 'data.identity_sources_incomplete');
assert.equal(identitySourceConflict(input, [...rows, rows[0]]), 'data.identity_sources_incomplete');
for (const patch of [
  { criteria_revision: 2 },
  { source_revision: 5 },
  { candidate_id: 'other' },
  { native_author_id: 'other' },
  { evidence_version: '0'.repeat(64) },
]) {
  assert.equal(
    identitySourceConflict(input, [{ ...rows[0], ...patch }, ...rows.slice(1)]),
    'data.identity_sources_changed',
  );
}
// A partially updated unbound source invalidates the whole published projection.
assert.equal(
  identitySourceConflict(input, [rows[0], rows[1], { ...rows[2], source_revision: 5 }]),
  'data.identity_sources_changed',
);

const invalid: Array<(value: ReturnType<typeof fixture>) => void> = [
  (value) => {
    value.source_rows.push(value.source_rows[0]);
  },
  (value) => {
    value.groups.push(value.groups[0]);
  },
  (value) => {
    value.groups[0].members.push(value.groups[0].members[0]);
  },
  (value) => {
    value.groups[0].members[0].source_hit_ids.push('youtube-hit');
  },
  (value) => {
    value.groups[0].members[0].source_hit_ids = ['x-hit'];
  },
  (value) => {
    value.groups[0].members[0].native_author_id = 'other';
  },
  (value) => {
    value.groups[0].members[0].source_hit_ids = ['unbound'];
  },
  (value) => {
    value.groups[0].members.pop();
  },
  (value) => {
    value.groups[0].has_same_author_review = false;
    value.groups[0].decision_ids = [];
  },
  (value) => {
    value.groups[0].decision_ids = ['review', 'review'];
  },
  (value) => {
    value.source_rows[2].native_author_id = 'partial';
  },
];
for (const mutate of invalid) {
  const value = fixture();
  mutate(value);
  assert.equal(discoveryIdentitySchema.safeParse(value).success, false);
}
assert.equal(
  discoveryIdentitySchema.safeParse({ ...fixture(), unauthorized_field: true }).success,
  false,
);
assert.equal(identityVersionConflict(input, discoveryIdentitySchema.parse(fixture())), null);
assert.equal(
  identityVersionConflict(input, { ...input, identity_revision: 1 }),
  'data.identity_stale',
);
assert.equal(
  identityVersionConflict(input, { ...input, source_revision: 3, identity_revision: 3 }),
  'data.identity_stale',
);
assert.equal(
  identityVersionConflict(input, { ...input, campaign_id: 'other' }),
  'data.identity_owner_conflict',
);
assert.equal(
  identityVersionConflict(input, { ...input, identity_hash: '0'.repeat(64) }),
  'data.identity_version_conflict',
);
assert.equal(
  identityVersionConflict(input, { ...input, groups: [] }),
  'data.identity_version_conflict',
);
// A corrected owner decision is accepted only under a later identity version.
// That new identity version cannot rewrite a frozen source version.
for (const patch of [
  { source_hash: '0'.repeat(64) },
  { criteria_revision: 2 },
  { source_rows: input.source_rows.slice(1) },
  {
    source_rows: input.source_rows.map((row) => ({ ...row, evidence_version: '0'.repeat(64) })),
  },
]) {
  assert.equal(
    identityVersionConflict(input, { ...input, identity_revision: 3, ...patch }),
    'data.identity_source_version_conflict',
  );
}
assert.equal(
  identityVersionConflict(input, {
    ...input,
    identity_revision: 3,
    source_rows: [...input.source_rows].reverse(),
  }),
  null,
);
assert.equal(
  identityVersionConflict(input, {
    ...input,
    source_revision: 5,
    source_hash: '0'.repeat(64),
    identity_revision: 3,
  }),
  null,
);
assert.equal(
  identityVersionConflict(input, { ...input, identity_revision: 3, identity_hash: '0'.repeat(64) }),
  null,
);
console.log(
  'Identity projection source completeness, membership binding and monotonic version contract passed',
);
