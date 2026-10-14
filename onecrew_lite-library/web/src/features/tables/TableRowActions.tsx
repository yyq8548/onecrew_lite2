import { useState } from 'react';
import { Mail, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';

export function TableRowActions({
  busy,
  onDelete,
  t,
}: {
  busy: boolean;
  onDelete: () => Promise<void>;
  t: (key: string) => string;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className="table-row-actions">
      <Tooltip>
        <TooltipTrigger render={<span className="inline-flex" />}>
          <Button
            type="button"
            size="icon-sm"
            variant="ghost"
            disabled
            aria-label={t('detail.emailComingSoon')}
          >
            <Mail size={15} />
          </Button>
        </TooltipTrigger>
        <TooltipContent>{t('detail.emailComingSoon')}</TooltipContent>
      </Tooltip>
      <Button
        type="button"
        size="icon-sm"
        variant="ghost"
        aria-label={t('detail.deleteRow')}
        title={t('detail.deleteRow')}
        disabled={busy}
        onClick={() => setOpen(true)}
      >
        <Trash2 size={15} />
      </Button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent variant="demo">
          <DialogHeader>
            <DialogTitle>{t('detail.deleteRowTitle')}</DialogTitle>
            <DialogDescription>{t('detail.deleteRowDescription')}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setOpen(false)} disabled={busy}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="destructive"
              disabled={busy}
              onClick={() =>
                void onDelete()
                  .then(() => setOpen(false))
                  .catch(() => undefined)
              }
            >
              {t('detail.deleteRow')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
