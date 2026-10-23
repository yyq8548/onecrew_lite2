import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import type { Dispatch, SetStateAction } from 'react';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Button } from '@/components/ui/button';
import type { DataType, ScoreValue } from '@onecrew/contracts/tables';
import { scoreValuePoints } from '@onecrew/contracts/tables';
import type { Column } from '@/features/tables/model';

type TableDialogsProps = {
  busy: boolean;
  structuredEdit: { column: { data_type: DataType } } | null;
  setStructuredEdit: (value: null) => void;
  structuredDraft: {
    level: ScoreValue['level'];
    reason: string;
    name: string;
    image_url: string;
    url: string;
  };
  setStructuredDraft: Dispatch<SetStateAction<StructuredDraft>>;
  structuredError: string;
  saveStructured: () => void;
  t: (key: string, options?: Record<string, unknown>) => string;
};
type StructuredDraft = TableDialogsProps['structuredDraft'];
export function TableDialogs(props: TableDialogsProps) {
  const {
    busy,
    structuredEdit,
    setStructuredEdit,
    structuredDraft,
    setStructuredDraft,
    structuredError,
    saveStructured,
    t,
  } = props;
  return (
    <>
      <Dialog
        open={Boolean(structuredEdit)}
        onOpenChange={(open) => {
          if (!open) setStructuredEdit(null);
        }}
      >
        <DialogContent variant="demo">
          <DialogHeader>
            <DialogTitle>
              {structuredEdit?.column.data_type === 'score'
                ? t('detail.scoreEdit')
                : structuredEdit?.column.data_type === 'company'
                  ? t('detail.companyEdit')
                  : t('detail.personEdit')}
            </DialogTitle>
            <DialogDescription>
              {structuredEdit?.column.data_type === 'score'
                ? t('detail.scoreDescription')
                : t('detail.structuredDescription')}
            </DialogDescription>
          </DialogHeader>
          {structuredEdit?.column.data_type === 'score' ? (
            <>
              <label>
                {t('detail.level')}
                <Select
                  value={t(`scores.${structuredDraft.level}`)}
                  onValueChange={(value) =>
                    setStructuredDraft((current) => ({
                      ...current,
                      level:
                        (['excellent', 'good', 'medium', 'low'] as const).find(
                          (level) => t(`scores.${level}`) === value,
                        ) ?? 'medium',
                    }))
                  }
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {(['excellent', 'good', 'medium', 'low'] as const).map((level) => (
                      <SelectItem key={level} value={t(`scores.${level}`)}>
                        {scoreValuePoints({ level })} · {t(`scores.${level}`)}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </label>
              <label>
                {t('detail.reason')}
                <Input
                  value={structuredDraft.reason}
                  onChange={(event) =>
                    setStructuredDraft((current) => ({
                      ...current,
                      reason: event.target.value,
                    }))
                  }
                  placeholder={t('detail.reasonPlaceholder')}
                />
              </label>
            </>
          ) : (
            <>
              <label>
                {t('detail.name')}
                <Input
                  autoFocus
                  value={structuredDraft.name}
                  onChange={(event) =>
                    setStructuredDraft((current) => ({
                      ...current,
                      name: event.target.value,
                    }))
                  }
                />
              </label>
              <label>
                {t('detail.imageUrl')}
                <Input
                  value={structuredDraft.image_url}
                  onChange={(event) =>
                    setStructuredDraft((current) => ({
                      ...current,
                      image_url: event.target.value,
                    }))
                  }
                  placeholder={t('detail.urlPlaceholder')}
                />
              </label>
              <label>
                {t('detail.linkUrl')}
                <Input
                  value={structuredDraft.url}
                  onChange={(event) =>
                    setStructuredDraft((current) => ({
                      ...current,
                      url: event.target.value,
                    }))
                  }
                  placeholder={t('detail.urlPlaceholder')}
                />
              </label>
            </>
          )}
          {structuredError && <div className="error-banner">{structuredError}</div>}
          <DialogFooter>
            <Button variant="outline" onClick={() => setStructuredEdit(null)}>
              {t('common.cancel')}
            </Button>
            <Button onClick={() => void saveStructured()} disabled={busy}>
              {t('common.save')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

export function RenameColumnDialog({
  column,
  name,
  setName,
  setColumn,
  onRename,
  busy,
  t,
}: {
  column: Column | null;
  name: string;
  setName: (name: string) => void;
  setColumn: (column: Column | null) => void;
  onRename: () => void;
  busy: boolean;
  t: (key: string, options?: Record<string, unknown>) => string;
}) {
  return (
    <Dialog
      open={Boolean(column)}
      onOpenChange={(open) => {
        if (!open && !busy) setColumn(null);
      }}
    >
      <DialogContent variant="demo">
        <DialogHeader>
          <DialogTitle>{t('detail.renameColumnTitle')}</DialogTitle>
          <DialogDescription>{t('detail.renameColumnDescription')}</DialogDescription>
        </DialogHeader>
        <label htmlFor="rename-column-name">{t('detail.columnName')}</label>
        <Input
          id="rename-column-name"
          autoFocus
          value={name}
          onChange={(event) => setName(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') onRename();
          }}
        />
        <DialogFooter>
          <Button variant="outline" onClick={() => setColumn(null)} disabled={busy}>
            {t('common.cancel')}
          </Button>
          <Button onClick={onRename} disabled={busy || !name.trim()}>
            {t('common.save')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
