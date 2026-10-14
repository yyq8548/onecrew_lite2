import { useCallback, useState } from 'react';
import { api, downloadFile, request } from '@/lib/api';
import type { Column, Table } from '@/features/tables/model';

export function useTableDetailActions({
  table,
  tableId,
  language,
  loadRows,
  run,
  setBusy,
  setError,
  onTableUpdate,
}: {
  table: Table | null;
  tableId: string;
  language: string;
  loadRows: (reset?: boolean) => Promise<void>;
  run: (action: () => Promise<void>) => Promise<void>;
  setBusy: (busy: boolean) => void;
  setError: (error: string) => void;
  onTableUpdate: (table: Table) => void;
}) {
  const [renamingColumn, setRenamingColumn] = useState<Column | null>(null);
  const [renamedColumn, setRenamedColumn] = useState('');

  const exportTable = () =>
    void run(async () => {
      const locale = language.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en';
      const { blob, filename } = await downloadFile(
        api['dynamic-tables'][':tableId'].export.$get({
          param: { tableId },
          query: { locale },
        }),
      );
      const objectUrl = URL.createObjectURL(blob);
      const anchor = document.createElement('a');
      anchor.href = objectUrl;
      anchor.download = filename ?? `${table?.name || 'table'}.xlsx`;
      document.body.append(anchor);
      anchor.click();
      anchor.remove();
      window.setTimeout(() => URL.revokeObjectURL(objectUrl), 1000);
    });

  const deleteRow = useCallback(
    (rowId: string) => async () => {
      setBusy(true);
      try {
        await request(
          api['dynamic-tables'][':tableId'].rows[':rowId'].$delete({
            param: { tableId, rowId },
          }),
        );
        await loadRows(true);
      } catch (error) {
        setError(error instanceof Error ? error.message : String(error));
        throw error;
      } finally {
        setBusy(false);
      }
    },
    [loadRows, setBusy, setError, tableId],
  );

  const renameColumn = () =>
    void run(async () => {
      if (!renamingColumn || !renamedColumn.trim()) return;
      const updated = await request(
        api['dynamic-tables'][':tableId'].columns[':columnId'].$patch({
          param: { tableId, columnId: renamingColumn.id },
          json: { name: renamedColumn.trim() },
        }),
      );
      onTableUpdate(updated);
      setRenamingColumn(null);
    });

  return {
    renamingColumn,
    renamedColumn,
    setRenamingColumn,
    setRenamedColumn,
    exportTable,
    deleteRow,
    renameColumn,
  };
}
