import { RefreshCw } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

/** The one refresh affordance. Icon-only in headers and toolbars; labelled
 * only where the surface has room and the action needs naming. */
export function RefreshButton({
  pending = false,
  label,
  labelled = false,
  variant,
  onClick,
  className,
}: {
  pending?: boolean;
  label?: string;
  labelled?: boolean;
  variant?: 'ghost' | 'outline';
  onClick: () => void;
  className?: string;
}) {
  const { t } = useTranslation('common');
  const resolvedLabel = label ?? t('actions.refresh');
  return (
    <Button
      variant={variant ?? (labelled ? 'outline' : 'ghost')}
      size={labelled ? 'sm' : 'icon'}
      className={cn(!labelled && 'size-8', className)}
      disabled={pending}
      onClick={onClick}
      aria-label={labelled ? undefined : resolvedLabel}
      title={labelled ? undefined : resolvedLabel}
    >
      <RefreshCw className={cn(pending && 'animate-spin')} />
      {labelled && resolvedLabel}
    </Button>
  );
}
