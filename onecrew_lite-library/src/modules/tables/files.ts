import * as XLSX from 'xlsx';
import { recommendationFields, recommendationRows } from './recommendation-result.js';
import type { DiscoveryCreatorView, RowDto, TableDto } from './data.js';
import type { DataType } from '@onecrew/contracts/tables';

const scoreLabels: Record<'zh-CN' | 'en', Record<string, string>> = {
  'zh-CN': { excellent: '优秀', good: '良好', medium: '一般', low: '较低' },
  en: { excellent: 'Excellent', good: 'Good', medium: 'Medium', low: 'Low' },
};

const safeLink = (value: unknown): string | undefined => {
  if (typeof value !== 'string') return undefined;
  try {
    const url = new URL(value);
    return ['http:', 'https:'].includes(url.protocol) ? url.href : undefined;
  } catch {
    return undefined;
  }
};

const note = (cell: XLSX.CellObject, text: string) => {
  cell.c = [{ a: 'OneCrew', t: text }];
};

const toCell = (value: unknown, type: DataType, locale: 'zh-CN' | 'en'): XLSX.CellObject => {
  if (value === null || value === undefined) return { t: 'z', v: '' };
  if (type === 'people' || type === 'company') {
    const object =
      typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : {};
    const label = typeof object.name === 'string' ? object.name : String(value);
    const cell: XLSX.CellObject = { t: 's', v: label };
    const link = safeLink(object.url);
    if (link) cell.l = { Target: link, Tooltip: link };
    const details = [
      safeLink(object.image_url) ? `Image: ${safeLink(object.image_url)}` : undefined,
      object.identity && typeof object.identity === 'object'
        ? `Identity: ${JSON.stringify(object.identity)}`
        : undefined,
    ].filter(Boolean);
    if (details.length) note(cell, details.join('\n'));
    return cell;
  }
  if (type === 'score') {
    const score =
      typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : {};
    const level = typeof score.level === 'string' ? score.level : String(value);
    const cell: XLSX.CellObject = {
      t: 's',
      v: scoreLabels[locale][level] ?? level,
    };
    if (typeof score.reason === 'string' && score.reason.length) note(cell, score.reason);
    return cell;
  }
  if (type === 'int' || type === 'float') {
    return typeof value === 'number' && Number.isFinite(value)
      ? { t: 'n', v: value }
      : { t: 's', v: String(value) };
  }
  if (type === 'bool') return { t: 'b', v: Boolean(value) };
  if (type === 'date') {
    const date = value instanceof Date ? value : new Date(String(value));
    return Number.isNaN(date.getTime()) ? { t: 's', v: String(value) } : { t: 'd', v: date };
  }
  if (type === 'website') {
    const text = String(value);
    const cell: XLSX.CellObject = { t: 's', v: text };
    const link = safeLink(text);
    if (link) cell.l = { Target: link, Tooltip: link };
    return cell;
  }
  return { t: 's', v: String(value) };
};

export const buildTableWorkbook = (
  table: TableDto,
  rows: RowDto[],
  locale: 'zh-CN' | 'en',
): Uint8Array => {
  const sheet: XLSX.WorkSheet = {};
  table.columns.forEach((column, columnIndex) => {
    const address = XLSX.utils.encode_cell({ r: 0, c: columnIndex });
    sheet[address] = { t: 's', v: column.name };
  });
  rows.forEach((row, rowIndex) => {
    table.columns.forEach((column, columnIndex) => {
      const address = XLSX.utils.encode_cell({ r: rowIndex + 1, c: columnIndex });
      sheet[address] = toCell(row.values[column.id], column.data_type, locale);
    });
  });
  sheet['!ref'] = XLSX.utils.encode_range({
    s: { r: 0, c: 0 },
    e: { r: rows.length, c: Math.max(0, table.columns.length - 1) },
  });
  sheet['!cols'] = table.columns.map((column) => ({
    wch: Math.min(48, Math.max(12, column.name.length + 2)),
  }));
  const workbook = XLSX.utils.book_new();
  XLSX.utils.book_append_sheet(workbook, sheet, 'Data');
  return XLSX.write(workbook, { type: 'array', bookType: 'xlsx', cellDates: true });
};

/** Export the owner's recommended authors, without internal source/audit columns. */
export const buildCreatorWorkbook = (
  table: TableDto,
  material: DiscoveryCreatorView,
  locale: 'zh-CN' | 'en',
): Uint8Array => {
  const rows = recommendationRows(table, material);
  const followersField = table.columns.find(field => field.name === 'Followers' && field.data_type === 'string')?.id;
  const headers: string[] = recommendationFields.map(field => field.name);
  if (followersField) headers.push(locale === 'zh-CN' ? '粉丝量' : 'Followers');
  const sheet = XLSX.utils.aoa_to_sheet([headers]);
  rows.forEach((row, index) => recommendationFields.forEach((field, column) => {
    sheet[XLSX.utils.encode_cell({r: index + 1, c: column})] =
      toCell(field.id === 'platform' ? row.values.platform.split(' / ').map(p => p === 'twitter' ? 'X' : p).join(' / ') : row.values[field.id], field.semantic_type, locale);
  }));
  if (followersField) rows.forEach((row, index) => {
    const group = material.projection?.groups.find(group => group.group_id === row.id);
    const values = group?.members.map(member => {
      const counts = [...new Set(member.source_hit_ids.flatMap(id => {
        const value = material.sources.find(source => source.search_hit_id === id)?.row.values[followersField];
        return typeof value === 'string' && /^(?:\d+|~.{1,40})$/.test(value) ? [value] : [];
      }))];
      const count = counts.length === 1 ? counts[0].replace(/^~/, locale === 'zh-CN' ? '约 ' : 'About ') : locale === 'zh-CN' ? '未知' : 'Unknown';
      return `${member.platform === 'twitter' ? 'X' : member.platform} · ${member.display_name || member.native_author_id}: ${count}`;
    }).join('\n');
    sheet[XLSX.utils.encode_cell({ r: index + 1, c: 5 })] = { t: 's', v: values ?? '' };
  });
  sheet['!ref'] = XLSX.utils.encode_range({s: {r: 0, c: 0}, e: {r: rows.length, c: headers.length - 1}});
  sheet['!cols'] = [24, 20, 24, 12, 48, 32].slice(0, headers.length).map((wch) => ({ wch }));
  const workbook = XLSX.utils.book_new();
  XLSX.utils.book_append_sheet(workbook, sheet, '推荐');
  return XLSX.write(workbook, { type: 'array', bookType: 'xlsx' });
};

export const cleanFileNamePart = (value: string) =>
  Array.from(value)
    .map((character) => (character === '/' || character === '\\' ? '_' : character))
    .filter((character) => {
      const code = character.charCodeAt(0);
      return code >= 32 && code !== 127;
    })
    .join('');

export const exportFilename = (name: string) => `${cleanFileNamePart(name).trim() || 'table'}.xlsx`;
