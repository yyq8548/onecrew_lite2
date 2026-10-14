import { api, request } from '@/lib/api';
import { query } from '@/lib/router';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ArrowLeft, Download, LockKeyhole, Pencil, Plus, Table2 } from 'lucide-react';
import {
  useTable,
  type Column as TanStackColumn,
  type CellContext,
  type ColumnDef,
  type OnChangeFn,
  type SortingState,
} from '@tanstack/react-table';
import {
  dataGridFeatures,
  DataGrid,
  DataGridContainer,
} from '@/components/reui/data-grid/data-grid';
import { DataGridColumnHeader } from '@/components/reui/data-grid/data-grid-column-header';
import { DataGridTableVirtual } from '@/components/reui/data-grid/data-grid-table-virtual';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { RenameColumnDialog, TableDialogs } from '@/features/tables/TableDialogs';
import { TableRowActions } from '@/features/tables/TableRowActions';
import { useTableDetailActions } from '@/features/tables/useTableDetailActions';
import { SemanticCell } from '@/features/tables/SemanticCell';
import { CreatorMainView } from '@/features/tables/CreatorMainView';
import { isDiscoveryTable } from '@/features/tables/creator-view';
import { subscribeCreatorRefresh } from '@/features/tables/creator-refresh';
import { watchCreatorUpdates } from '@/features/tables/creator-updates';
import { AppShell } from '@/layout/AppShell';
import type { CellValue, DataType, PeopleValue, ScoreValue } from '@onecrew/contracts/tables';
import type { Column, Row, Table } from '@/features/tables/model';
import { canSort, cellValue, isHttpUrl, isStructured } from '@/features/tables/model';
import type {
  DataGridFeatures,
  DataGridTableInstance,
} from '@/components/reui/data-grid/data-grid';

const gridI18n = (t: (key: string, options?: Record<string, unknown>) => string) => ({
  labels: {
    sortAscending: t('grid.sortAscending'),
    sortDescending: t('grid.sortDescending'),
    columnsMenu: t('grid.columnsMenu'),
    toggleColumns: t('grid.toggleColumns'),
    rowsPerPage: t('grid.rowsPerPage'),
    previousPage: t('grid.previousPage'),
    nextPage: t('grid.nextPage'),
    paginationEllipsis: t('grid.paginationEllipsis'),
    paginationInfo: ({ from, to, count }: { from: number; to: number; count: number }) =>
      t('grid.paginationInfo', { from, to, count }),
    loading: t('grid.loading'),
    empty: t('grid.empty'),
    allRowsLoaded: t('grid.allRowsLoaded'),
  },
});

type StructuredDraft = {
  name: string;
  image_url: string;
  url: string;
  level: ScoreValue['level'];
  reason: string;
};
const emptyStructuredDraft: StructuredDraft = {
  name: '',
  image_url: '',
  url: '',
  level: 'medium',
  reason: '',
};

export function TableDetailPage({
  tableId,
  listSearch,
  navigate,
}: {
  tableId: string;
  listSearch: string;
  navigate: (path: string) => void;
}) {
  const { t, i18n } = useTranslation();
  const [table, setTable] = useState<Table | null>(null);
  const [view, setView] = useState<'auto' | 'creators' | 'sources'>('auto');
  const [sourceFallback, setSourceFallback] = useState(false);
  const showSourcesWhenUnavailable = useCallback(() => {
    setSourceFallback(true);
    setView('sources');
  }, []);
  useEffect(() => {
    setView('auto');
    setSourceFallback(false);
  }, [tableId]);
  const [rows, setRows] = useState<Row[]>([]);
  const [total, setTotal] = useState(0);
  const batchSize = 50;
  const [hasMore, setHasMore] = useState(true);
  const [isFetchingMore, setIsFetchingMore] = useState(false);
  const [sorting, setSorting] = useState<SortingState>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [editingCell, setEditingCell] = useState<{
    rowId: string;
    columnId: string;
  } | null>(null);
  const [draft, setDraft] = useState('');
  const [structuredEdit, setStructuredEdit] = useState<{
    row: Row;
    column: Column;
  } | null>(null);
  const [structuredDraft, setStructuredDraft] = useState<StructuredDraft>(emptyStructuredDraft);
  const [structuredError, setStructuredError] = useState('');
  const [addColumnOpen, setAddColumnOpen] = useState(false);
  const [newColumnName, setNewColumnName] = useState('');
  const [newColumnType, setNewColumnType] = useState<DataType>('string');
  const [busy, setBusy] = useState(false);
  const skipBlur = useRef(false);
  const nextPageRef = useRef(1);
  const requestIdRef = useRef(0);
  const loadingMoreRef = useRef(false);
  const hasMoreRef = useRef(true);
  const loadTable = useCallback(async () => {
    try {
      const value = await request(api['dynamic-tables'][':tableId'].$get({ param: { tableId } }));
      setTable(value);
      setError('');
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Table could not be loaded');
    }
  }, [tableId]);
  const loadRows = useCallback(
    async (reset = false) => {
      if (!reset && (loadingMoreRef.current || !hasMoreRef.current)) return;
      const requestId = ++requestIdRef.current;
      const requestedPage = reset ? 1 : nextPageRef.current;
      if (reset) {
        loadingMoreRef.current = true;
        nextPageRef.current = 1;
        hasMoreRef.current = true;
        setRows([]);
        setTotal(0);
        setHasMore(true);
        setIsFetchingMore(false);
        setLoading(true);
      } else {
        loadingMoreRef.current = true;
        setIsFetchingMore(true);
      }
      try {
        const item = sorting[0];
        const sortColumn = table?.columns.find((column) => column.id === item?.id);
        const result = await request(
          api['dynamic-tables'][':tableId'].rows.$get({
            param: { tableId },
            query: {
              page: String(requestedPage),
              size: String(batchSize),
              ...(item && sortColumn && canSort(sortColumn.data_type)
                ? { sort_field: item.id, sort_order: item.desc ? 'desc' : 'asc' }
                : {}),
            },
          }),
        );
        if (requestId !== requestIdRef.current) return;
        setRows((current) => {
          if (reset) return result.rows;
          const merged = new Map(current.map((row) => [row.id, row]));
          result.rows.forEach((row) => merged.set(row.id, row));
          return [...merged.values()];
        });
        nextPageRef.current = requestedPage + 1;
        hasMoreRef.current = result.has_more;
        setHasMore(result.has_more);
        setTotal(result.total);
        setError('');
      } catch (e) {
        if (requestId === requestIdRef.current)
          setError(e instanceof Error ? e.message : String(e));
      } finally {
        if (requestId === requestIdRef.current) {
          setLoading(false);
          loadingMoreRef.current = false;
          setIsFetchingMore(false);
        }
      }
    },
    [batchSize, sorting, table, tableId],
  );
  useEffect(() => {
    void loadTable();
  }, [loadTable]);
  useEffect(() => {
    document
      .querySelector<HTMLElement>('[data-slot="data-grid"] [data-slot="scroll-area-viewport"]')
      ?.scrollTo({ top: 0, left: 0, behavior: 'auto' });
    void loadRows(true);
    return () => {
      requestIdRef.current += 1;
    };
  }, [loadRows]);
  useEffect(() => {
    if (!table || !isDiscoveryTable(table) || view !== 'sources'
        || editingCell || structuredEdit || busy) return;
    // Source rows carry the same publication as the creator view. Without this
    // subscription a completed identity read remains visibly "unverified".
    void loadRows(true);
    return subscribeCreatorRefresh(
      (signal, changed) => watchCreatorUpdates(tableId, signal, changed),
      () => { void loadRows(true); },
    );
  }, [table, tableId, view, editingCell, structuredEdit, busy, loadRows]);
  const run = useCallback(async (action: () => Promise<void>) => {
    setBusy(true);
    try {
      await action();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, []);
  const {
    renamingColumn,
    renamedColumn,
    setRenamingColumn,
    setRenamedColumn,
    exportTable,
    deleteRow,
    renameColumn,
  } = useTableDetailActions({
    table,
    tableId,
    language: i18n.language,
    loadRows,
    run,
    setBusy,
    setError,
    onTableUpdate: setTable,
  });
  const startEdit = useCallback((row: Row, column: Column) => {
    if (column.read_only) return;
    if (isStructured(column.data_type)) {
      const current = cellValue(row.values[column.id]);
      if (
        column.data_type === 'score' &&
        current &&
        typeof current === 'object' &&
        'level' in current
      ) {
        const score = current as ScoreValue;
        setStructuredDraft({
          ...emptyStructuredDraft,
          level: score.level,
          reason: score.reason ?? '',
        });
      } else {
        const item =
          current && typeof current === 'object' && 'name' in current
            ? (current as PeopleValue)
            : null;
        setStructuredDraft({
          ...emptyStructuredDraft,
          name: item?.name ?? '',
          image_url: item?.image_url ?? '',
          url: item?.url ?? '',
        });
      }
      setStructuredError('');
      setStructuredEdit({ row, column });
      return;
    }
    setEditingCell({ rowId: row.id, columnId: column.id });
    const value = cellValue(row.values[column.id]);
    setDraft(
      column.data_type === 'date'
        ? String(value ?? '').slice(0, 10)
        : value === null || value === undefined
          ? ''
          : String(value),
    );
  }, []);
  const commit = useCallback(
    async (row: Row, column: Column, raw: string) => {
      if (column.read_only) return;
      if (skipBlur.current) {
        skipBlur.current = false;
        return;
      }
      let value: CellValue = raw === '' ? null : raw;
      if (column.data_type === 'int') value = raw === '' ? null : Number.parseInt(raw, 10);
      if (column.data_type === 'float') value = raw === '' ? null : Number.parseFloat(raw);
      if (column.data_type === 'bool') value = raw === '' ? null : raw === 'true';
      if (column.data_type === 'date') value = raw || null;
      if (typeof value === 'number' && !Number.isFinite(value)) {
        setError(t('detail.numberError'));
        return;
      }
      await run(async () => {
        await request(
          api['dynamic-tables'][':tableId'].rows.$patch({
            param: { tableId },
            json: { rows: [{ id: row.id, values: { [column.id]: value } }] },
          }),
        );
        setRows((current) =>
          current.map((candidate) =>
            candidate.id === row.id
              ? {
                  ...candidate,
                  values: { ...candidate.values, [column.id]: value },
                }
              : candidate,
          ),
        );
      });
      setEditingCell(null);
    },
    [run, t, tableId],
  );
  const saveStructured = async () => {
    if (!structuredEdit) return;
    const { row, column } = structuredEdit;
    if (column.read_only) return;
    let value: CellValue;
    if (column.data_type === 'score')
      value = {
        level: structuredDraft.level,
        ...(structuredDraft.reason.trim() ? { reason: structuredDraft.reason.trim() } : {}),
      };
    else {
      if (!structuredDraft.name.trim()) {
        setStructuredError(t('detail.nameRequired'));
        return;
      }
      if (
        (structuredDraft.image_url && !isHttpUrl(structuredDraft.image_url)) ||
        (structuredDraft.url && !isHttpUrl(structuredDraft.url))
      ) {
        setStructuredError(t('detail.urlError'));
        return;
      }
      const current = cellValue(row.values[column.id]);
      const identity =
        current && typeof current === 'object' && 'identity' in current
          ? (current as PeopleValue).identity
          : undefined;
      value = {
        name: structuredDraft.name.trim(),
        ...(structuredDraft.image_url ? { image_url: structuredDraft.image_url.trim() } : {}),
        ...(structuredDraft.url ? { url: structuredDraft.url.trim() } : {}),
        ...(identity ? { identity } : {}),
      };
    }
    await run(async () => {
      await request(
        api['dynamic-tables'][':tableId'].rows.$patch({
          param: { tableId },
          json: { rows: [{ id: row.id, values: { [column.id]: value } }] },
        }),
      );
      setRows((current) =>
        current.map((candidate) =>
          candidate.id === row.id
            ? {
                ...candidate,
                values: { ...candidate.values, [column.id]: value },
              }
            : candidate,
        ),
      );
    });
    setStructuredEdit(null);
  };
  const addColumn = () =>
    void run(async () => {
      const value = await request(
        api['dynamic-tables'][':tableId'].columns.$post({
          param: { tableId },
          json: { columns: [{ name: newColumnName.trim(), data_type: newColumnType }] },
        }),
      );
      setTable(value);
      setNewColumnName('');
      setNewColumnType('string');
      setAddColumnOpen(false);
    });
  const addRow = () =>
    void run(async () => {
      await request(
        api['dynamic-tables'][':tableId'].rows.$post({
          param: { tableId },
          json: { rows: [{ values: {} }] },
        }),
      );
      await loadRows(true);
    });
  const onSortingChange: OnChangeFn<SortingState> = (updater) => {
    const next = typeof updater === 'function' ? updater(sorting) : updater;
    const first = next[0];
    const column = table?.columns.find((item) => item.id === first?.id);
    setSorting(first && column && canSort(column.data_type) ? [first] : []);
  };
  const columnDefs = useMemo<ColumnDef<DataGridFeatures, Row, unknown>[]>(
    () => [
      {
        id: '__row_number',
        size: 48,
        minSize: 48,
        maxSize: 48,
        enableSorting: false,
        enablePinning: false,
        header: t('detail.rowNumber', '#'),
        cell: (info: CellContext<DataGridFeatures, Row, unknown>) => (
          <div className="row-number">{info.row.index + 1}</div>
        ),
      },
      ...(table?.columns ?? []).map((column) => ({
        id: column.id,
        size: isStructured(column.data_type) ? 220 : column.data_type === 'string' ? 240 : 180,
        minSize: 160,
        maxSize: 280,
        enablePinning: false,
        accessorFn: (row: Row) => row.values[column.id],
        enableSorting: canSort(column.data_type),
        header: ({
          column: headerColumn,
        }: {
          column: TanStackColumn<DataGridFeatures, Row, unknown>;
        }) => (
          <div className="table-column-heading">
            <DataGridColumnHeader column={headerColumn} title={column.name} />
            {column.read_only ? (
              <span title={t('detail.systemReadOnly')}>
                <LockKeyhole size={14} aria-label={t('detail.systemReadOnly')} />
              </span>
            ) : (
              <Button
                type="button"
                size="icon-sm"
                variant="ghost"
                aria-label={t('detail.renameColumn', { name: column.name })}
                title={t('detail.renameColumn', { name: column.name })}
                disabled={busy}
                onClick={() => {
                  setRenamedColumn(column.name);
                  setRenamingColumn(column);
                }}
              >
                <Pencil size={14} />
              </Button>
            )}
          </div>
        ),
        cell: (info: CellContext<DataGridFeatures, Row, unknown>) => {
          const row = info.row.original;
          const isEditing = editingCell?.rowId === row.id && editingCell.columnId === column.id;
          if (!isEditing)
            return (
              <div
                className="cell-readonly"
                onDoubleClick={() => startEdit(row, column)}
                title={
                  column.read_only
                    ? t('detail.systemReadOnly')
                    : isStructured(column.data_type)
                      ? t('detail.structuredEdit')
                      : t('detail.edit')
                }
              >
                <SemanticCell column={column} value={cellValue(info.getValue())} />
              </div>
            );
          if (column.data_type === 'bool')
            return (
              <Select
                value={
                  draft === ''
                    ? t('common.unknown')
                    : draft === 'true'
                      ? t('common.yes')
                      : t('common.no')
                }
                onValueChange={(value) => {
                  const next =
                    value === t('common.unknown')
                      ? ''
                      : value === t('common.yes')
                        ? 'true'
                        : 'false';
                  setDraft(next);
                  void commit(row, column, next);
                }}
              >
                <SelectTrigger aria-label={t('detail.boolean')}>
                  <SelectValue placeholder={t('detail.select')} />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={t('common.unknown')}>{t('common.unknown')}</SelectItem>
                  <SelectItem value={t('common.yes')}>{t('common.yes')}</SelectItem>
                  <SelectItem value={t('common.no')}>{t('common.no')}</SelectItem>
                </SelectContent>
              </Select>
            );
          return (
            <Input
              autoFocus
              value={draft}
              type={
                column.data_type === 'int' || column.data_type === 'float'
                  ? 'number'
                  : column.data_type === 'date'
                    ? 'date'
                    : 'text'
              }
              step={column.data_type === 'float' ? 'any' : undefined}
              onChange={(event) => setDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Escape') {
                  skipBlur.current = true;
                  setEditingCell(null);
                }
                if (event.key === 'Enter') event.currentTarget.blur();
              }}
              onBlur={() => void commit(row, column, draft)}
            />
          );
        },
      })),
      {
        id: '__actions',
        size: 88,
        minSize: 88,
        maxSize: 88,
        enableSorting: false,
        enableHiding: false,
        enableResizing: false,
        enablePinning: true,
        meta: { headerClassName: 'px-0', cellClassName: 'px-0' },
        header: () => <div className="table-row-actions-heading">{t('detail.rowActions')}</div>,
        cell: (info: CellContext<DataGridFeatures, Row, unknown>) => (
          <TableRowActions busy={busy} onDelete={deleteRow(info.row.original.id)} t={t} />
        ),
      },
    ],
    [
      busy,
      commit,
      deleteRow,
      draft,
      editingCell,
      setRenamedColumn,
      setRenamingColumn,
      startEdit,
      table?.columns,
      t,
    ],
  );
  const tableInstance = useTable({
    features: dataGridFeatures,
    data: rows,
    columns: columnDefs,
    state: {
      sorting,
      columnPinning: { start: [], end: ['__actions'] },
      pagination: { pageIndex: 0, pageSize: Math.max(rows.length, batchSize) },
    },
    onSortingChange,
    manualSorting: true,
    enableColumnPinning: true,
    manualPagination: true,
    pageCount: 1,
    rowCount: total,
    getRowId: (row: Row) => row.id,
  });
  const back = () => navigate(`/library${query(listSearch)}`);
  return (
    <AppShell navigate={navigate} active="library">
      <section className="content detail-content">
        {error && !table ? (
          <div className="detail-error">
            <p className="eyebrow">{t('detail.eyebrow')}</p>
            <h1>{t('detail.loadError')}</h1>
            <p>{error}</p>
            <Button onClick={back}>
              <ArrowLeft size={15} /> {t('common.backToLibrary')}
            </Button>
          </div>
        ) : (
          <>
            <div className="detail-heading">
              <div className="detail-heading-main">
                <Button variant="ghost" onClick={back} aria-label={t('common.backToLibrary')}>
                  <ArrowLeft size={15} />
                </Button>
                <span className="detail-table-icon">
                  <Table2 size={15} />
                </span>
                <div>
                  <h1>{table?.name ?? t('detail.loadingTable')}</h1>
                </div>
              </div>
              <div className="detail-actions">
                {isDiscoveryTable(table) && (
                  <Button
                    variant="outline"
                    onClick={() => setView(view === 'sources' ? 'creators' : 'sources')}
                  >
                    {view === 'sources' ? t('detail.creatorView') : t('detail.sourceDetails')}
                  </Button>
                )}
                <Button variant="outline" onClick={exportTable} disabled={!table || busy}>
                  <Download size={14} /> {t('detail.exportXlsx')}
                </Button>
                <Button variant="outline" onClick={() => setAddColumnOpen(true)} disabled={!table}>
                  <Plus size={14} /> {t('detail.addColumn')}
                </Button>
                <Button onClick={addRow} disabled={!table || busy}>
                  <Plus size={14} /> {t('detail.addRow')}
                </Button>
              </div>
            </div>
            {error && <div className="error-banner">{error}</div>}
            {sourceFallback && view === 'sources' && (
              <p role="status">{t('detail.creatorViewSourceFallback')}</p>
            )}
            {table && isDiscoveryTable(table) && view !== 'sources' ? (
              <CreatorMainView
                table={table}
                tableId={tableId}
                showSources={() => setView('sources')}
                onUnavailable={view === 'auto' ? showSourcesWhenUnavailable : undefined}
              />
            ) : (
              <DataGrid
                i18n={gridI18n(t)}
                table={tableInstance as DataGridTableInstance<Row>}
                recordCount={total}
                isLoading={loading}
                tableLayout={{
                  dense: true,
                  rowBorder: true,
                  cellBorder: false,
                  headerBackground: true,
                  headerBorder: true,
                  width: 'fixed',
                  columnsResizable: true,
                  columnsResizeMode: 'onEnd',
                  headerSticky: true,
                  columnsPinnable: true,
                }}
              >
                <DataGridContainer variant="table">
                  <DataGridTableVirtual
                    height="min(560px, calc(100vh - 200px))"
                    estimateSize={56}
                    overscan={8}
                    onFetchMore={() => void loadRows(false)}
                    isFetchingMore={isFetchingMore}
                    hasMore={hasMore}
                    fetchMoreOffset={8}
                  />
                </DataGridContainer>
              </DataGrid>
            )}
            {(!isDiscoveryTable(table) || view === 'sources') && !loading && rows.length === 0 && (
              <div className="grid-empty">{t('detail.noRows')}</div>
            )}
          </>
        )}
        <TableDialogs
          addColumnOpen={addColumnOpen}
          setAddColumnOpen={setAddColumnOpen}
          newColumnName={newColumnName}
          setNewColumnName={setNewColumnName}
          newColumnType={newColumnType}
          setNewColumnType={setNewColumnType}
          addColumn={addColumn}
          busy={busy}
          structuredEdit={structuredEdit}
          setStructuredEdit={setStructuredEdit}
          structuredDraft={structuredDraft}
          setStructuredDraft={setStructuredDraft}
          structuredError={structuredError}
          saveStructured={saveStructured}
          t={t}
        />
        <RenameColumnDialog
          column={renamingColumn}
          name={renamedColumn}
          setName={setRenamedColumn}
          setColumn={setRenamingColumn}
          onRename={renameColumn}
          busy={busy}
          t={t}
        />
      </section>
    </AppShell>
  );
}
