import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import type { Table } from './model';
import { SemanticCell } from './SemanticCell';
import { watchCreatorUpdates } from './creator-updates';
import { subscribeCreatorRefresh } from './creator-refresh';
import { followerDigits, sortByFollowers } from './follower-sort';
import {
  filterCreators,
  groupedCreators,
  readCreatorView,
  type CreatorMaterial,
  type VerdictFilter,
} from './creator-view';

/** One owner snapshot binds reviewed groups, qualification and notes in the same transaction. */
export function CreatorMainView({
  table,
  tableId,
  showSources,
  onUnavailable,
}: {
  table: Table;
  tableId: string;
  showSources: () => void;
  onUnavailable?: () => void;
}) {
  const { t, i18n } = useTranslation();
  const [material, setMaterial] = useState<CreatorMaterial | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [reload, setReload] = useState(0);
  const [sort, setSort] = useState('followers-desc');
  const [filter, setFilter] = useState<VerdictFilter>('qualified');
  useEffect(
    () =>
      subscribeCreatorRefresh(
        (signal, changed) => watchCreatorUpdates(tableId, signal, changed),
        () => setReload((current) => current + 1),
      ),
    [tableId],
  );
  useEffect(() => {
    let active = true;
    setLoading(true);
    setError('');
    void readCreatorView(tableId)
      .then((result) => {
        if (active) {
          setMaterial(result);
          // A publication can temporarily invalidate the projection. Keep this
          // view mounted so its bounded refresh can observe the next snapshot.
          if (result.state === 'missing') onUnavailable?.();
        }
      })
      .catch((cause: unknown) => {
        if (active) {
          setError(cause instanceof Error ? cause.message : String(cause));
        }
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [tableId, reload, onUnavailable]);
  const grouped = useMemo(
    () => (material ? groupedCreators(table, material) : { creators: [], unidentified: 0 }),
    [table, material],
  );
  const visibleCreators = useMemo(
    // The Creator owner enforces the new contact gate. Historical approvals retain their policy.
    () => {
      const rows = filterCreators(grouped.creators, filter);
      return sort === 'match' ? rows : sortByFollowers(rows, sort === 'followers-asc');
    },
    [grouped, sort, filter],
  );
  const ready =
    !error && material?.state === 'current' && material.projection?.table_id === tableId;
  return (
    <div className="creator-view">
      <div className="creator-view-heading">
        <label>
          {i18n.language.startsWith('zh') ? '核实状态 ' : 'Verification '}
          <select value={filter} onChange={event => setFilter(event.target.value as VerdictFilter)}>
            {(['qualified', 'pending', 'rejected', 'all'] as const).map(value => <option key={value} value={value}>
              {value === 'all' ? (i18n.language.startsWith('zh') ? '全部' : 'All') : t(`detail.verdict.${value}`, value)}
              {value !== 'all' && ` (${grouped.creators.filter(creator => creator.verdict === value).length})`}
            </option>)}
          </select>
        </label>
        <label>
          {i18n.language.startsWith('zh') ? '排序 ' : 'Sort '}
          <select value={sort} onChange={event => setSort(event.target.value)}>
            <option value="followers-desc">{i18n.language.startsWith('zh') ? '粉丝量：从高到低' : 'Followers: high to low'}</option>
            <option value="followers-asc">{i18n.language.startsWith('zh') ? '粉丝量：从低到高' : 'Followers: low to high'}</option>
            <option value="match">{i18n.language.startsWith('zh') ? '匹配评分' : 'Match rating'}</option>
          </select>
        </label>
        {ready && (
          <p>
            {t('detail.creatorViewSummary', {
              creators: visibleCreators.length,
              unidentified: grouped.unidentified,
            })}
          </p>
        )}
        <Button
          variant="outline"
          type="button"
          onClick={() => setReload((current) => current + 1)}
          disabled={loading}
        >
          {t('detail.refreshCreators')}
        </Button>
      </div>
      {loading && !material && <p role="status">{t('detail.creatorViewLoading')}</p>}
      {error && (
        <div className="error-banner" role="alert">
          {error}
        </div>
      )}
      {!loading && !error && material?.state !== 'current' && (
        <p role="status">
          {t(
            material?.state === 'sources_changed'
              ? 'detail.creatorViewStale'
              : 'detail.creatorViewMissing',
          )}
        </p>
      )}
      {ready && (
        <>
          <div className="creator-view-scroll">
            <table>
              <thead>
                <tr>
                  <th>{t('detail.creator')}</th>
                  <th>{t('detail.followers')}</th>
                  <th>{t('detail.qualification')}</th>
                  <th>{t('detail.matchEvaluation')}</th>
                  <th>{t('detail.contacts')}</th>
                </tr>
              </thead>
              <tbody>
                {visibleCreators.map((creator) => (
                  <tr key={creator.key}>
                    <td>
                      <strong>{creator.label}</strong>
                      <ul>
                        {creator.accounts.map((account) => (
                          <li key={JSON.stringify([account.platform, account.nativeId])}>
                            {account.platform.toLowerCase() === 'twitter' ? 'X' : account.platform} · {account.label}
                            {account.authorUrl && (
                              <>
                                {' '}
                                ·{' '}
                                <a href={account.authorUrl} target="_blank" rel="noreferrer">
                                  {t('detail.authorProfile')}
                                </a>
                              </>
                            )}
                          </li>
                        ))}
                      </ul>
                    </td>
                    <td><ul>{creator.accounts.map(account => <li key={JSON.stringify([account.platform, account.nativeId])}>
                      {creator.accounts.length > 1 && `${account.platform === 'twitter' ? 'X' : account.platform} · `}
                      {followerDigits(account.followers) ?? t('detail.followersUnknown')}
                    </li>)}</ul></td>
                    <td>
                      {t(`detail.verdict.${creator.verdict}`, creator.verdict)}
                      {creator.verdict === 'pending' && <ul>{creator.reasons
                        .filter(reason => /^(核实状态|证据缺口|下一步|最后核实时间)：/.test(reason))
                        .map(reason => <li key={reason}>{reason}</li>)}</ul>}
                    </td>
                    <td>
                      {creator.rating ? (
                        <SemanticCell
                          column={{ id: 'match', name: 'Match evaluation', data_type: 'score' }}
                          value={creator.rating}
                        />
                      ) : (
                        t('detail.unrated')
                      )}
                    </td>
                    <td>
                      <ul>
                        {creator.contacts.map((contact) => (
                          <li key={contact.normalized_value}>
                            <a
                              href={
                                contact.kind === 'email'
                                  ? `mailto:${encodeURIComponent(contact.normalized_value).replace('%40', '@')}`
                                  : contact.normalized_value
                              }
                              target="_blank"
                              rel="noreferrer"
                            >
                              {contact.kind === 'hidden_email'
                                ? '有隐藏邮箱，需手动查看 · 主页'
                                : contact.original_value}
                            </a>
                          </li>
                        ))}
                      </ul>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
      <Button className="justify-self-start" variant="outline" type="button" onClick={showSources}>
        {t('detail.editSourceDetails')}
      </Button>
    </div>
  );
}
