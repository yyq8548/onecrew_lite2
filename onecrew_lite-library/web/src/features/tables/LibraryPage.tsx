import { api, request } from '@/lib/api';
import { query } from '@/lib/router';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ChevronRight, LibraryBig, Search, Table2, Upload } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import {
  Item,
  ItemActions,
  ItemContent,
  ItemGroup,
  ItemMedia,
  ItemSeparator,
  ItemTitle,
} from '@/components/ui/item';
import type { Table } from '@/features/tables/model';
import { AppShell } from '@/layout/AppShell';

export function LibraryPage({
  initialSearch,
  navigate,
}: {
  initialSearch: string;
  navigate: (path: string) => void;
}) {
  const { t } = useTranslation();
  const [search, setSearch] = useState(initialSearch);
  const [tables, setTables] = useState<Table[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [open, setOpen] = useState(false);
  const [name, setName] = useState('');
  const [file, setFile] = useState<File | null>(null);
  const [busy, setBusy] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const requestId = useRef(0);
  const load = useCallback(async (value: string) => {
    const current = ++requestId.current;
    setLoading(true);
    try {
      const result = await request(
        api['dynamic-tables'].$get({ query: { page: '1', size: '100', q: value || undefined } }),
      );
      if (current !== requestId.current) return;
      setTables(result.tables);
      setError('');
    } catch (e) {
      if (current === requestId.current) setError(e instanceof Error ? e.message : String(e));
    } finally {
      if (current === requestId.current) setLoading(false);
    }
  }, []);
  useEffect(() => {
    const timer = window.setTimeout(() => void load(search), 120);
    return () => window.clearTimeout(timer);
  }, [load, search]);
  const updateSearch = (value: string) => {
    setSearch(value);
    window.history.replaceState({}, '', `/library${query(value)}`);
  };
  const setImportOpen = (nextOpen: boolean) => {
    if (busy) return;
    setOpen(nextOpen);
    if (!nextOpen) {
      setFile(null);
      setName('');
      setError('');
      if (fileInputRef.current) fileInputRef.current.value = '';
    }
  };
  const importTable = async () => {
    if (!file) return;
    if (file.size > 10 * 1024 * 1024) {
      setError(t('library.fileTooLarge'));
      return;
    }
    setBusy(true);
    setError('');
    try {
      const table = await request(
        api['dynamic-tables'].import.$post({
          form: { file, ...(name.trim() ? { name: name.trim() } : {}) },
        }),
      );
      setOpen(false);
      setName('');
      setFile(null);
      if (fileInputRef.current) fileInputRef.current.value = '';
      navigate(`/library/${table.id}${query(search)}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <AppShell navigate={navigate} active="library">
      <section className="content library-content">
        <div className="page-heading">
          <div>
            <h1>{t('library.title')}</h1>
          </div>
          <Button onClick={() => setOpen(true)}>
            <Upload size={16} /> {t('library.importTable')}
          </Button>
        </div>
        <div className="library-search">
          <div className="search-box">
            <Search size={16} />
            <Input
              value={search}
              onChange={(e) => updateSearch(e.target.value)}
              placeholder={t('library.searchPlaceholder')}
              aria-label={t('library.searchPlaceholder')}
            />
          </div>
        </div>
        <div className="library-toolbar">
          <span className="result-note">
            {loading ? t('common.loading') : t('library.tableCount', { count: tables.length })}
          </span>
        </div>
        {error && <div className="error-banner">{error}</div>}
        {!loading && tables.length === 0 ? (
          <div className="empty-state">
            <span className="empty-icon">
              <LibraryBig size={25} />
            </span>
            <h2>{search ? t('library.noMatch') : t('library.noTables')}</h2>
            <p>{search ? t('library.tryAnother') : t('library.createHint')}</p>
            {!search && (
              <Button variant="outline" onClick={() => setOpen(true)}>
                <Upload size={15} /> {t('library.importTable')}
              </Button>
            )}
          </div>
        ) : (
          <ItemGroup variant="table-list">
            {tables.map((table, index) => (
              <div key={table.id}>
                <Item
                  variant="table-card"
                  render={
                    <button
                      type="button"
                      onClick={() => navigate(`/library/${table.id}${query(search)}`)}
                    />
                  }
                >
                  <ItemMedia variant="table-card-icon">
                    <Table2 size={17} />
                  </ItemMedia>
                  <ItemContent variant="table-card-copy">
                    <ItemTitle>{table.name}</ItemTitle>
                  </ItemContent>
                  <ItemActions>
                    <ChevronRight className="table-card-arrow" size={18} />
                  </ItemActions>
                </Item>
                {index < tables.length - 1 && <ItemSeparator variant="table-list" />}
              </div>
            ))}
          </ItemGroup>
        )}
        <Dialog open={open} onOpenChange={setImportOpen}>
          <DialogContent variant="demo">
            <DialogHeader>
              <DialogTitle>{t('library.importTitle')}</DialogTitle>
              <DialogDescription>{t('library.importDescription')}</DialogDescription>
            </DialogHeader>
            <label htmlFor="table-import-file">
              {t('library.fileLabel')}
              <Input
                id="table-import-file"
                ref={fileInputRef}
                type="file"
                accept=".xlsx,.csv,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet,text/csv"
                onChange={(event) => setFile(event.target.files?.[0] ?? null)}
                aria-describedby="table-import-help"
              />
            </label>
            <p id="table-import-help" className="form-hint">
              {t('library.importHelp')}
            </p>
            <label htmlFor="table-import-name">
              {t('library.tableName')}
              <Input
                id="table-import-name"
                autoFocus
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={t('library.tableNameOptional')}
              />
            </label>
            {error && <div className="error-banner">{error}</div>}
            <DialogFooter>
              <Button variant="outline" onClick={() => setImportOpen(false)} disabled={busy}>
                {t('common.cancel')}
              </Button>
              <Button onClick={() => void importTable()} disabled={busy || !file}>
                <Upload size={15} /> {busy ? t('common.loading') : t('library.importTable')}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </section>
    </AppShell>
  );
}
