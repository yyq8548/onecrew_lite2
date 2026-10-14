import * as React from "react";

import { cn } from "@mcp-ui/lib/utils";

function Skeleton({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="skeleton"
      // A placeholder has to be visible against the card it sits on. `accent`
      // is a Host-supplied palette variable and a Host that sends white for it
      // left the loading card blank, so the placeholder is drawn from the
      // foreground instead, which every theme keeps in contrast.
      className={cn("animate-pulse rounded-md bg-foreground/10 motion-reduce:animate-none", className)}
      {...props}
    />
  );
}

export { Skeleton };
