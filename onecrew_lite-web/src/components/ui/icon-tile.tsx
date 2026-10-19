import type * as React from 'react';

import { cn } from '@/lib/utils';

/** The square container a glyph sits in.
 *
 * Shape carries meaning and is not an author preference: a *thing* (platform,
 * service, media, step) gets the rounded square; a *person* gets the circle. */
export function IconTile({
  subject = 'thing',
  size = 9,
  className,
  children,
  ...props
}: React.ComponentProps<'span'> & { subject?: 'thing' | 'person'; size?: 9 | 10 | 11 }) {
  return (
    <span
      className={cn(
        'grid shrink-0 place-items-center border bg-card text-muted-foreground',
        size === 9 && 'size-9',
        size === 10 && 'size-10',
        size === 11 && 'size-11',
        subject === 'person' ? 'rounded-full' : 'rounded-lg',
        className,
      )}
      {...props}
    >
      {children}
    </span>
  );
}
