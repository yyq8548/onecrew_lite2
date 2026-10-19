import { useEffect, useMemo, useState, type ComponentType } from "react";
import {
  AtSign,
  Cable,
  Check,
  RotateCcw,
  Rocket,
  X,
} from "lucide-react";
import { useTranslation } from "react-i18next";

import type { View } from "@/components/AppSidebar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Progress } from "@/components/ui/progress";
import { useSidebar } from "@/components/ui/sidebar";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

type StepId = "account" | "lead" | "mcp";

type TutorialStep = {
  id: StepId;
  view: View;
  icon: ComponentType<{ className?: string }>;
};

const STORAGE_KEY = "reacher.getting-started.v1";

const STEPS: TutorialStep[] = [
  {
    id: "account",
    view: "accounts",
    icon: AtSign,
  },
  {
    id: "mcp",
    view: "connect",
    icon: Cable,
  },
];

function readCompleted(): StepId[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const value = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "[]");
    return Array.isArray(value)
      ? value.filter((id): id is StepId => STEPS.some((step) => step.id === id))
      : [];
  } catch {
    return [];
  }
}

export function GettingStarted({ onNavigate }: { onNavigate: (view: View) => void }) {
  const { t } = useTranslation("shell");
  const { isMobile } = useSidebar();
  const [open, setOpen] = useState(false);
  const [selectedId, setSelectedId] = useState<StepId>(STEPS[0].id);
  const [completed, setCompleted] = useState<StepId[]>(readCompleted);

  useEffect(() => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(completed));
  }, [completed]);

  const selected = useMemo(
    () => STEPS.find((step) => step.id === selectedId) ?? STEPS[0],
    [selectedId],
  );
  const progress = Math.round((completed.length / STEPS.length) * 100);

  function completeSelected() {
    setCompleted((current) =>
      current.includes(selected.id) ? current : [...current, selected.id],
    );
    onNavigate(selected.view);
    setOpen(false);
  }

  function selectNext() {
    const index = STEPS.findIndex((step) => step.id === selected.id);
    setSelectedId(STEPS[(index + 1) % STEPS.length].id);
  }

  const SelectedIcon = selected.icon;
  const selectedCopyBase = `gettingStarted.steps.${selected.id}` as const;

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          size="icon"
          variant="outline"
          className="relative size-9 shrink-0 rounded-full bg-background p-0 shadow-xs hover:bg-background"
          aria-label={t("gettingStarted.triggerAriaLabel", { progress })}
          title={t("gettingStarted.triggerTitle", { progress })}
        >
          <svg className="absolute inset-0 size-full -rotate-90" viewBox="0 0 56 56" aria-hidden="true">
            <circle cx="28" cy="28" r="25" fill="none" stroke="currentColor" strokeWidth="3" className="text-muted/80" />
            <circle
              cx="28"
              cy="28"
              r="25"
              fill="none"
              pathLength="100"
              stroke="currentColor"
              strokeWidth="3"
              strokeLinecap="round"
              strokeDasharray={`${progress} 100`}
              className={progress === 100 ? "text-[var(--tone-success-solid)]" : "text-primary"}
            />
          </svg>
          <span className="relative grid size-6 place-items-center rounded-full bg-primary text-primary-foreground">
            <Rocket className="size-3.5" />
          </span>
        </Button>
      </PopoverTrigger>

      <PopoverContent
        side="bottom"
        align="end"
        sideOffset={10}
        className={cn(
          "grid max-h-[calc(100vh-2rem)] w-[650px] max-w-[calc(100vw-5rem)] grid-cols-[320px_minmax(0,1fr)] gap-0 overflow-hidden rounded-xl p-0 shadow-xl",
          isMobile && "w-[calc(100vw-2rem)] max-w-none grid-cols-1",
        )}
      >
        <section className="flex min-h-0 flex-col border-r bg-muted/25">
          <div className="border-b p-4">
            <div className="flex items-start gap-3">
              <div className="min-w-0 flex-1">
                <h3 className="text-sm font-semibold">{t("gettingStarted.title")}</h3>
                <p className="mt-1 text-xs text-muted-foreground">
                  <strong className="font-semibold text-foreground">
                    {t("gettingStarted.completed", { progress })}
                  </strong>
                  {" · "}{t("gettingStarted.encouragement")}
                </p>
                <p className="mt-1.5 text-xs font-medium text-[var(--tone-success-fg)]">
                  {t("gettingStarted.goal")}
                </p>
              </div>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="size-8"
                    onClick={() => {
                      setCompleted([]);
                      setSelectedId(STEPS[0].id);
                    }}
                    aria-label={t("gettingStarted.startOver")}
                  >
                    <RotateCcw />
                  </Button>
                </TooltipTrigger>
                <TooltipContent side="top">{t("gettingStarted.startOver")}</TooltipContent>
              </Tooltip>
              <Button
                variant="ghost"
                size="icon"
                className="size-8"
                onClick={() => setOpen(false)}
                aria-label={t("gettingStarted.close")}
              >
                <X />
              </Button>
            </div>
            <Progress value={progress} className="mt-3 h-1.5" />
          </div>

          <div className="min-h-0 flex-1 space-y-1 overflow-y-auto p-2">
            {STEPS.map((step, index) => {
              const StepIcon = step.icon;
              const done = completed.includes(step.id);
              const active = selected.id === step.id;
              return (
                <Button
                  key={step.id}
                  type="button"
                  variant="ghost"
                  onClick={() => setSelectedId(step.id)}
                  className={cn(
                    "h-12 w-full justify-start gap-3 px-2.5 text-left font-normal",
                    active && "bg-accent text-accent-foreground",
                  )}
                >
                  <span className="grid size-8 shrink-0 place-items-center rounded-md border bg-background">
                    {done ? <Check className="text-[var(--tone-success-solid)]" /> : <StepIcon />}
                  </span>
                  <span className="min-w-0 flex-1 truncate text-sm">
                    {t(`gettingStarted.steps.${step.id}.title`)}
                  </span>
                  <Badge
                    variant="secondary"
                    className={cn(
                      "border-transparent text-[10px] tabular-nums",
                      done && "bg-[var(--tone-success-bg)] text-[var(--tone-success-fg)]",
                    )}
                  >
                    {done
                      ? t("gettingStarted.done")
                      : t("gettingStarted.stepCount", { current: index + 1, total: STEPS.length })}
                  </Badge>
                </Button>
              );
            })}
          </div>
        </section>

        <section className={cn("flex min-h-[430px] min-w-0 flex-col p-4", isMobile && "hidden")}>
          <div className="grid min-h-40 flex-1 place-items-center rounded-lg border bg-primary/5">
            <div className="grid size-20 place-items-center rounded-2xl bg-primary/10 text-primary">
              <SelectedIcon className="size-10" />
            </div>
          </div>
          <div className="pt-4">
            <h4 className="text-sm font-semibold">{t(`${selectedCopyBase}.title`)}</h4>
            <p className="mt-2 min-h-16 text-sm leading-6 text-muted-foreground">
              {t(`${selectedCopyBase}.description`)}
            </p>
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="outline" onClick={selectNext}>{t("gettingStarted.skip")}</Button>
              <Button onClick={completeSelected}>{t(`${selectedCopyBase}.action`)}</Button>
            </div>
          </div>
        </section>
      </PopoverContent>
    </Popover>
  );
}
