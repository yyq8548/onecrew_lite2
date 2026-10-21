import type { InferResponseType } from 'hono/client';
import { api, request } from '@/lib/api';
import type { Table } from './model';
import { preferredContacts, scoreValueSchema, type ScoreValue } from '@onecrew/contracts/tables';

export function isDiscoveryTable(table: Table | null): boolean {
  if (!table) return false;
  return ['OneCrew qualification', 'OneCrew candidate ID', 'OneCrew native author ID'].every(
    (name) => table.columns.some((column) => column.name === name && column.read_only),
  );
}

export type CreatorMaterial = InferResponseType<
  (typeof api)['dynamic-tables'][':tableId']['creators']['$get'],
  200
>;
export type VerdictFilter = 'all' | 'qualified' | 'related' | 'pending' | 'rejected';
export type CreatorAccount = {
  platform: string;
  nativeId: string;
  label: string;
  authorUrl: string;
  verdict: string;
  followers: string | null;
  pinterestMonthlyViews?: string | null;
  pinterestScalePassedBy?: Array<'followers' | 'monthly_views'>;
  bioStatus?: string;
};
export type CreatorGroup = {
  key: string;
  label: string;
  accounts: CreatorAccount[];
  verdict: string;
  reasons: string[];
  sources: Array<{ title: string; url: string; rowId: string }>;
  notes: string[];
  rating: ScoreValue | null;
  contacts: CreatorMaterial['sources'][number]['contacts'];
};

export async function readCreatorView(tableId: string): Promise<CreatorMaterial> {
  return request(api['dynamic-tables'][':tableId'].creators.$get({ param: { tableId } }));
}

const sourceColumns = new Set([
  'Campaign ID',
  'Search hit ID',
  'Platform',
  'Author label',
  'Followers',
  'Pinterest monthly views',
  'Pinterest scale passed by',
  'Biography status',
  'Author ID',
  'Author URL',
  'Content title',
  'Source URL',
  'Search keyword',
  'Status',
  'Captured at (Unix s)',
  'Match evaluation',
]);
const scoreOrder = { excellent: 0, good: 1, medium: 2, low: 3 };
const verdictOrder: Record<string, number> = { qualified: 0, related: 1, pending: 2, rejected: 3 };
export function filterCreators(creators: CreatorGroup[], filter: VerdictFilter): CreatorGroup[] {
  return creators
    .filter((creator) => filter === 'all' || creator.verdict === filter)
    .sort(
      (a, b) =>
        (a.rating ? scoreOrder[a.rating.level] : 4) - (b.rating ? scoreOrder[b.rating.level] : 4) ||
        (verdictOrder[a.verdict] ?? 3) - (verdictOrder[b.verdict] ?? 3) ||
        a.label.localeCompare(b.label),
    );
}

function profileUrl(platform: string, nativeId: string, saved: string) {
  if (saved.startsWith('https://')) return saved;
  if (platform === 'youtube' && /^UC[A-Za-z0-9_-]{22}$/.test(nativeId))
    return `https://www.youtube.com/channel/${nativeId}`;
  return '';
}

/** Consume the owner's atomic identity/source snapshot; never merge display names or infer membership from editable cells. */
export function groupedCreators(table: Table, material: CreatorMaterial) {
  const projection = material.state === 'current' ? material.projection : null;
  if (!projection || projection.table_id !== table.id) return { creators: [], unidentified: 0 };
  const sources = new Map(material.sources.map((source) => [source.search_hit_id, source]));
  const recommendations = new Map(material.recommendations.map(row => [row.id, row]));
  const field = (name: string) => table.columns.find((column) => column.name === name)?.id;
  const text = (values: Record<string, unknown>, name: string) => {
    const id = field(name);
    return id && typeof values[id] === 'string' ? (values[id] as string) : '';
  };
  const creators: CreatorGroup[] = projection.groups.map((group) => {
    const recommendation = recommendations.get(group.group_id);
    const bound = group.members.flatMap((member) =>
      member.source_hit_ids.map((hit) => sources.get(hit)!),
    );
    const accounts = group.members.map((member): CreatorAccount => {
      const verdicts = member.source_hit_ids.map((hit) => sources.get(hit)!.verdict);
      const observedCounts = [...new Set(member.source_hit_ids.map(hit =>
        text(sources.get(hit)!.row.values, 'Followers')).filter(value => /^(?:\d+|~.{1,40})$/.test(value)))];
      const first = verdicts[0];
      const monthlyViews = [...new Set(member.source_hit_ids.map(hit => text(sources.get(hit)!.row.values, 'Pinterest monthly views')).filter(Boolean))];
      const scaleBranches = [...new Set(member.source_hit_ids.flatMap(hit => text(sources.get(hit)!.row.values, 'Pinterest scale passed by').split(',')))].filter((branch): branch is 'followers' | 'monthly_views' => branch === 'followers' || branch === 'monthly_views');
      return {
        platform: member.platform,
        nativeId: member.native_author_id,
        label: member.display_name || member.native_author_id,
        followers: observedCounts.length === 1 ? observedCounts[0] : null,
        bioStatus: [...new Set(member.source_hit_ids.map(hit => text(sources.get(hit)!.row.values, 'Biography status')).filter(Boolean))].join(','),
        ...(member.platform === 'pinterest' ? { pinterestMonthlyViews: monthlyViews.length === 1 ? monthlyViews[0] : null, pinterestScalePassedBy: scaleBranches } : {}),
        authorUrl: profileUrl(member.platform, member.native_author_id, member.profile_url),
        verdict:
          verdicts.every((verdict) => verdict === first) &&
          ['qualified', 'rejected'].includes(first)
            ? first
            : 'pending',
      };
    });
    return {
      key: group.group_id,
      label: recommendation?.values.name.name ?? accounts[0].label,
      accounts,
      contacts: preferredContacts([
        ...new Map(
          bound
            .filter((source) => source.verdict === 'qualified' || source.verdict === 'pending' && source.reasons.includes('related_alternative'))
            .flatMap((source) => source.contacts)
            .map((contact) => [`${contact.kind}:${contact.normalized_value}`, contact]),
        ).values(),
      ]),
      rating: recommendation?.values.score ??
        bound
          .flatMap((source) => {
            const field = table.columns.find(
              (column) => column.name === 'Match evaluation' && column.data_type === 'score',
            );
            const score = scoreValueSchema.safeParse(field ? source.row.values[field.id] : null);
            return score.success
              ? [score.data]
              : [];
          })
          .sort((a, b) => scoreOrder[a.level] - scoreOrder[b.level])[0] ?? null,
      // A qualifying native account qualifies the reviewed person; other accounts
      // retain their own verdict and cannot borrow followers or content evidence.
      verdict: recommendation
        ? 'qualified'
        : accounts.every((account) => account.verdict === 'rejected')
          ? 'rejected'
          : bound.some(source => source.verdict === 'pending' && source.reasons.includes('related_alternative'))
            ? 'related'
            : 'pending',
      reasons: [...new Set(bound.flatMap((source) => source.reasons))],
      sources: bound.map(({ row }) => ({
        title: text(row.values, 'Content title'),
        url: text(row.values, 'Source URL'),
        rowId: row.id,
      })),
      notes: [
        ...new Set(
          bound.flatMap(({ row }) =>
            table.columns
              .filter((column) => !column.read_only && !sourceColumns.has(column.name))
              .map((column) => row.values[column.id])
              .filter(
                (value): value is string => typeof value === 'string' && value.trim().length > 0,
              ),
          ),
        ),
      ],
    };
  });
  return {
    creators,
    unidentified: projection.source_rows.filter((source) => source.candidate_id === null).length,
  };
}
