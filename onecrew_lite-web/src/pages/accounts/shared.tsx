import { Check } from "lucide-react";
import { useTranslation } from "react-i18next";

import { cn } from "@/lib/utils";
import { PlatformIcon } from "@/components/platform";
import type { Platform } from "@/types";

/** Per-platform accent for the glyph tile. Token/palette colors only. */
export const PLATFORM_TONE: Record<Platform, string> = {
  x: "text-foreground",
  instagram: "text-fuchsia-600 dark:text-fuchsia-400",
  telegram: "text-sky-600 dark:text-sky-400",
};

/** Platform glyph tile — used by the grid, the detail header, and table rows. */
export function PlatformTile({ platform, className }: { platform: Platform; className?: string }) {
  return (
    <span
      className={cn(
        "grid place-items-center rounded-lg border bg-card",
        PLATFORM_TONE[platform],
        className,
      )}
    >
      <PlatformIcon platform={platform} className="size-5" />
    </span>
  );
}

/** Custom step indicator — shadcn has no official stepper, so this is composed
 *  from primitives with only token colors. Reused by the create wizard (4 steps)
 *  and the re-login flow (2 steps). */
export function Stepper({ step, steps }: { step: number; steps: readonly string[] }) {
  const { t } = useTranslation("accounts");
  return (
    <ol className="flex items-center gap-2" aria-label={t("stepper.ariaLabel")}>
      {steps.map((label, i) => {
        const done = i < step;
        const current = i === step;
        return (
          <li key={label} className="flex flex-1 items-center gap-2">
            <span className="flex items-center gap-1.5">
              <span
                aria-current={current ? "step" : undefined}
                className={cn(
                  "grid size-6 shrink-0 place-items-center rounded-full border text-xs font-medium",
                  done && "border-transparent bg-primary text-primary-foreground",
                  current && "border-primary text-primary",
                  !done && !current && "border-border text-muted-foreground",
                )}
              >
                {done ? <Check className="size-3.5" /> : i + 1}
              </span>
              <span
                className={cn(
                  "text-xs",
                  current ? "font-medium text-foreground" : "text-muted-foreground",
                )}
              >
                {label}
                <span className="sr-only">
                  {done
                    ? t("stepper.completed")
                    : current
                      ? t("stepper.current")
                      : t("stepper.notStarted")}
                </span>
              </span>
            </span>
            {i < steps.length - 1 && <span className="h-px flex-1 bg-border" />}
          </li>
        );
      })}
    </ol>
  );
}
