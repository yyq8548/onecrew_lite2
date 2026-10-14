import { cva, type VariantProps } from "class-variance-authority";
import * as React from "react";

import { cn } from "@/lib/utils";

const alertVariants = cva(
  "relative grid w-full gap-1 rounded-lg border px-4 py-3 text-sm [&>svg]:size-4",
  {
    variants: {
      variant: {
        default: "border-border bg-background text-foreground",
        destructive:
          "border-destructive/25 bg-destructive/5 text-destructive [&_[data-slot=alert-description]]:text-destructive/90",
        warning:
          "border-amber-500/30 bg-amber-500/5 text-amber-950 [&_[data-slot=alert-description]]:text-amber-900",
      },
    },
    defaultVariants: { variant: "default" },
  },
);

export function Alert({
  className,
  variant,
  ...props
}: React.ComponentProps<"div"> & VariantProps<typeof alertVariants>) {
  return (
    <div
      data-slot="alert"
      role="alert"
      className={cn(alertVariants({ variant }), className)}
      {...props}
    />
  );
}

export function AlertTitle({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="alert-title"
      className={cn("font-medium leading-none", className)}
      {...props}
    />
  );
}

export function AlertDescription({
  className,
  ...props
}: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="alert-description"
      className={cn("text-sm leading-6 text-muted-foreground", className)}
      {...props}
    />
  );
}
