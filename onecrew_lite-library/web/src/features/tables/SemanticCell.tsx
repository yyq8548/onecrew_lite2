import { ExternalLink, Link2, Mail } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import { Badge } from '@/components/reui/badge';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import type { CellValue, PeopleValue, ScoreValue } from '@onecrew/contracts/tables';
import type { Column } from '@/features/tables/model';
import { initials, isHttpUrl } from '@/features/tables/model';

const formatCell = (column: Column, value: CellValue) => {
  if (value == null || value === '') return '—';
  if (column.data_type === 'date') return String(value).slice(0, 10);
  if (column.name === 'Platform' || column.name === '平台')
    return String(value).split(' / ').map(platform => platform.toLowerCase() === 'twitter' ? 'X' : platform).join(' / ');
  return String(value);
};

function LinkValue({ href, children }: { href: string; children: React.ReactNode }) {
  return (
    <a
      className="platform-link"
      href={href}
      target="_blank"
      rel="noreferrer"
      onDoubleClick={(event) => event.stopPropagation()}
    >
      {children}
      <ExternalLink size={13} />
    </a>
  );
}

export function SemanticCell({ column, value }: { column: Column; value: CellValue }) {
  const { t } = useTranslation();
  if (value == null || value === '') return <span className="cell-value-muted">—</span>;
  if (column.data_type === 'people' || column.data_type === 'company') {
    const item =
      typeof value === 'object' && value && 'name' in value ? (value as PeopleValue) : null;
    if (!item) return <span className="cell-value-muted">—</span>;
    const content = (
      <>
        <Avatar>
          <AvatarImage
            src={isHttpUrl(item.image_url) ? item.image_url : undefined}
            alt={item.name}
          />
          <AvatarFallback>{initials(item.name)}</AvatarFallback>
        </Avatar>
        <span className="creator-name">{item.name}</span>
      </>
    );
    return item.url && isHttpUrl(item.url) ? (
      <LinkValue href={item.url}>
        <span className="creator-name-cell">{content}</span>
      </LinkValue>
    ) : (
      <span className="creator-name-cell">{content}</span>
    );
  }
  if (column.data_type === 'score') {
    const score =
      typeof value === 'object' && value && 'level' in value ? (value as ScoreValue) : null;
    if (!score) return <span className="cell-value-muted">—</span>;
    return (
      <Tooltip>
        <TooltipTrigger>
          <Badge
            tabIndex={0}
            radius="full"
            variant={
              `score-${score.level}` as
                'score-excellent' | 'score-good' | 'score-medium' | 'score-low'
            }
          >
            {t(`scores.${score.level}`)}
          </Badge>
        </TooltipTrigger>
        <TooltipContent>{score.reason?.trim() || t('common.noExplanation')}</TooltipContent>
      </Tooltip>
    );
  }
  if (column.data_type === 'website' && isHttpUrl(value)) {
    let label = value;
    try {
      label = new URL(value).hostname.replace(/^www\./, '');
    } catch {
      /* use stored URL */
    }
    return (
      <LinkValue href={value}>
        <Link2 size={14} />
        {label}
      </LinkValue>
    );
  }
  if (
    column.data_type === 'email' &&
    typeof value === 'string' &&
    /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value)
  )
    return (
      <LinkValue href={`mailto:${encodeURIComponent(value)}`}>
        <Mail size={14} />
        {value}
      </LinkValue>
    );
  if (
    column.data_type === 'telephone' &&
    typeof value === 'string' &&
    /^[+\d][\d\s().-]{2,63}$/.test(value)
  )
    return <LinkValue href={`tel:${value.replace(/[^+\d]/g, '')}`}>{value}</LinkValue>;
  return (
    <span>
      {column.data_type === 'bool'
        ? t(value ? 'common.yes' : 'common.no')
        : formatCell(column, value)}
    </span>
  );
}
