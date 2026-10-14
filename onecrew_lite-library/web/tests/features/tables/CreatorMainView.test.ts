import assert from 'node:assert/strict';
import test from 'node:test';
import {
  filterCreators,
  groupedCreators,
  type CreatorMaterial,
} from '../../../src/features/tables/creator-view';
import type { Table } from '../../../src/features/tables/model';

const table = {
  id: 'table',
  name: 'Campaign',
  columns: [
    ...['Platform', 'Author label', 'Author URL', 'Content title', 'Source URL', 'My note'].map(
      (name) => ({ id: name, name, data_type: 'string' as const }),
    ),
    { id: 'verdict', name: 'OneCrew qualification', data_type: 'string', read_only: true },
  ],
} as Table;
function material(merged = true): CreatorMaterial {
  const sources = ['hit-a', 'hit-b'].map((hit, index) => ({
    search_hit_id: hit,
    verdict: index === 0 ? 'qualified' : 'rejected',
    reasons: index === 0 ? [] : ['min_subscribers_not_met'],
    row: {
      id: 'row-' + hit,
      values: {
        Platform: 'editable-wrong-platform',
        'Author label': 'Same name',
        'Author URL': 'https://example.com/forged',
        'Content title': hit,
        'Source URL': 'https://example.com/post/' + hit,
        'My note': index === 0 ? 'Keep' : 'Call later',
        verdict: 'qualified',
      },
    },
  }));
  const members = sources.map((source, index) => ({
    candidate_id: 'candidate-' + index,
    platform: index === 0 ? 'youtube' : 'twitter',
    native_author_id: index === 0 ? 'UCZPqG0yh_xPm2AyLjffbDvw' : 'twitter-id',
    display_name: 'Same name',
    profile_url: '',
    source_hit_ids: [source.search_hit_id],
  }));
  return {
    state: 'current',
    sources,
    projection: {
      table_id: table.id,
      campaign_id: 'campaign',
      projection_format: 1,
      criteria_revision: 1,
      source_revision: 1,
      identity_revision: 1,
      source_hash: 'a'.repeat(64),
      identity_hash: 'b'.repeat(64),
      source_rows: sources.map((source, index) => ({
        search_hit_id: source.search_hit_id,
        candidate_id: members[index].candidate_id,
        native_author_id: members[index].native_author_id,
        evidence_version: 'a'.repeat(64),
      })),
      groups: merged
        ? [
            {
              group_id: 'c'.repeat(64),
              has_same_author_review: true,
              decision_ids: ['review'],
              members,
            },
          ]
        : members.map((member, index) => ({
            group_id: String(index).repeat(64),
            has_same_author_review: false,
            decision_ids: [],
            members: [member],
          })),
    },
  };
}

test('reviewed cross-platform author is one row and preserves each account qualification, source and manual note', () => {
  const result = groupedCreators(table, material());
  assert.equal(result.creators.length, 1);
  assert.equal(result.creators[0].verdict, 'qualified');
  assert.deepEqual(
    result.creators[0].accounts.map((account) => account.verdict),
    ['qualified', 'rejected'],
  );
  assert.deepEqual(
    result.creators[0].accounts.map((account) => account.platform),
    ['youtube', 'twitter'],
  );
  assert.equal(result.creators[0].sources.length, 2);
  assert.deepEqual(result.creators[0].notes, ['Keep', 'Call later']);
  assert.equal(
    result.creators[0].accounts[0].authorUrl,
    'https://www.youtube.com/channel/UCZPqG0yh_xPm2AyLjffbDvw',
  );
  assert.equal(
    result.creators[0].accounts[1].authorUrl,
    '',
    'editable source profile must not override native owner identity',
  );
});

test('same display name remains two rows without an owner same-author decision', () => {
  const result = groupedCreators(table, material(false));
  assert.equal(result.creators.length, 2);
  assert.equal(filterCreators(result.creators, 'qualified').length, 1);
  assert.equal(filterCreators(result.creators, 'rejected').length, 1);
});

test('a previous table snapshot cannot be displayed in another table during navigation', () => {
  assert.deepEqual(groupedCreators({ ...table, id: 'another-table' }, material()), {
    creators: [],
    unidentified: 0,
  });
});

test('missing and stale identity material never falls back to native account counts or stale qualification', () => {
  for (const state of ['missing', 'sources_changed'] as const) {
    assert.deepEqual(
      groupedCreators(table, { ...material(), state, projection: null, sources: [] }),
      { creators: [], unidentified: 0 },
    );
  }
});

test('mixed source qualification remains pending for that account, even if editable cells claim qualified', () => {
  const input = material(false);
  input.sources[0].verdict = 'pending';
  const result = groupedCreators(table, input);
  assert.equal(result.creators[0].verdict, 'pending');
  assert.equal(filterCreators(result.creators, 'qualified').length, 0);
});

test('unbound sources are reported separately and do not invent author rows', () => {
  const input = material();
  input.projection!.source_rows.push({
    search_hit_id: 'unbound',
    candidate_id: null,
    native_author_id: null,
    evidence_version: 'd'.repeat(64),
  });
  input.sources.push({
    search_hit_id: 'unbound',
    verdict: 'unverified',
    reasons: [],
    row: { id: 'unbound-row', values: {} },
  });
  const result = groupedCreators(table, input);
  assert.equal(result.unidentified, 1);
  assert.equal(result.creators.length, 1);
  assert.equal(result.creators[0].sources.length, 2);
});

test('initial grades sort before unrated authors and pending cells cannot invent a grade', () => {
  const scoredTable = {
    ...table,
    columns: [
      ...table.columns,
      { id: 'score', name: 'Match evaluation', data_type: 'score' as const },
    ],
  };
  const input = material(false);
  input.sources[0].row.values = {
    ...input.sources[0].row.values,
    score: { level: 'good', reason: 'Relevant saved content.' },
  };
  input.sources[1].row.values = {
    ...input.sources[1].row.values,
    score: { level: 'low', reason: 'Below the required threshold.' },
  };
  const groups = groupedCreators(scoredTable, input).creators;
  assert.deepEqual(
    filterCreators(groups.reverse(), 'all').map((group) => group.rating?.level),
    ['good', 'low'],
  );
  input.sources[0].verdict = 'pending';
  input.sources[0].row.values = {
    ...input.sources[0].row.values,
    score: { level: 'excellent', reason: 'Forged pending score.' },
  };
  const pending = groupedCreators(scoredTable, input).creators;
  assert.deepEqual(
    filterCreators(pending, 'all').map((group) => group.rating?.level ?? null),
    ['low', null],
  );
  const template = groups[0];
  const levels = ['low', 'medium', 'good', 'excellent'] as const;
  assert.deepEqual(
    filterCreators(
      levels
        .map((level) => ({ ...template, rating: { level, reason: 'Saved evidence.' } }))
        .concat({ ...template, rating: null }),
      'all',
    ).map((group) => group.rating?.level ?? null),
    ['excellent', 'good', 'medium', 'low', null],
  );
});
