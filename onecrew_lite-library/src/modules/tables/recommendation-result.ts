import { contactDisplay, preferredContacts, scoreValueSchema } from '@onecrew/contracts/tables';
import type { PeopleValue, ScoreValue } from '@onecrew/contracts/tables';
import type { DiscoveryCreatorView, TableDto } from './data.js';

/** Stable semantic fields shared by export and the complete upload snapshot. */
export const recommendationFields = [
  { id: 'name', name: '名字', semantic_type: 'people' },
  { id: 'category', name: '类别', semantic_type: 'string' },
  { id: 'platform', name: '平台', semantic_type: 'string' },
  { id: 'score', name: '评分', semantic_type: 'score' },
  { id: 'contacts', name: '联系方式', semantic_type: 'string' },
] as const;

export type RecommendationRow = {
  id: string;
  values: { name: PeopleValue; category: string | null; platform: string; score: ScoreValue; contacts: string };
};

/** Consume only the owner's current identity and qualification projection. */
export function recommendationRows(table: TableDto, material: DiscoveryCreatorView): RecommendationRow[] {
  if (material.state !== 'current' || !material.projection || material.projection.table_id !== table.id)
    throw new Error('Creator export requires a current identity snapshot');
  const sources = new Map(material.sources.map(source => [source.search_hit_id, source]));
  const scoreId = table.columns.find(c => c.name === 'Match evaluation' && c.data_type === 'score')?.id;
  const categoryId = table.columns.find(c => c.name === 'Content category')?.id;
  const rank = { excellent: 0, good: 1, medium: 2, low: 3 };
  return material.projection.groups.flatMap(group => {
    const qualified = group.members.flatMap(member => member.source_hit_ids.flatMap(id => {
      const source = sources.get(id);
      if (!source || source.verdict !== 'qualified') return [];
      const score = scoreValueSchema.safeParse(scoreId ? source.row.values[scoreId] : null);
      if (!score.success || score.data.level === 'low' || !score.data.reason?.trim()) return [];
      return [{ member, source, score: score.data }];
    }));
    qualified.sort((a, b) => rank[a.score.level] - rank[b.score.level]
      || a.source.search_hit_id.localeCompare(b.source.search_hit_id));
    const selected = qualified[0];
    if (!selected) return [];
    const contacts = preferredContacts([...new Map(qualified.flatMap(({source}) => source.contacts)
      .map(contact => [`${contact.kind}:${contact.normalized_value}`, contact])).values()]);
    // 联系方式是否必需由账号的审核结果决定；此处保留已经核实的作者。
    const { member, source, score } = selected;
    const category = categoryId ? source.row.values[categoryId] : null;
    return [{ id: group.group_id, values: {
      name: { name: member.display_name || member.native_author_id,
        ...(member.profile_url.startsWith('https://') ? { url: member.profile_url } : {}),
        identity: { platform: member.platform, platform_id: member.native_author_id } },
      category: typeof category === 'string' ? category : null,
      platform: [...new Set(qualified.map(({member}) => member.platform))].join(' / '),
      score,
      contacts: contacts.map(contactDisplay).join('\n'),
    } }];
  });
}
