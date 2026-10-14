import { Loader2 } from 'lucide-react';

import { cn } from '@/lib/utils';

/** The one block-level loading state. Inside a button, use
 * `<Loader2 className="size-4 animate-spin" />` directly instead. */
export function PendingBlock({ className }: { className?: string }) {
  return (
    <div className={cn('grid min-h-40 flex-1 place-items-center', className)}>
      <Loader2 className="size-5 animate-spin text-muted-foreground" />
    </div>
  );
}
