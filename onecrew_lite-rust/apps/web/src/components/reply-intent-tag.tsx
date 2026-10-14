// 来信意向标签:跟 creator 表里的通道标签同一种形状,放在收件箱行的
// 时间下方。标签本身只说结论,hover 才展开 agent 的判读理由和它草拟
// 的话术——列表要能一眼扫过,细节按需展开。

import { Badge } from "@/components/ui/badge";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useTranslation } from "react-i18next";
import {
  REPLY_INTENT_NEEDS_ATTENTION,
  type ReplyTriageBrief,
} from "@/lib/creator";
import { cn } from "@/lib/utils";

/** 语气按「要不要人动手」分,而不是按情绪好坏:已拒绝是个明确答案,
 * 不该染成警报色。 */
const INTENT_TONE: Record<ReplyTriageBrief["intent"], string> = {
  interested: "bg-[var(--tone-success-bg)] text-[var(--tone-success-fg)]",
  negotiating: "bg-[var(--tone-info-bg)] text-[var(--tone-info-fg)]",
  question: "bg-[var(--tone-warning-bg)] text-[var(--tone-warning-fg)]",
  declined: "bg-muted text-muted-foreground",
  irrelevant: "bg-muted text-muted-foreground",
  other: "bg-muted text-muted-foreground",
};

function formatTriageDate(epoch: number, locale: string) {
  return new Intl.DateTimeFormat(locale, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  }).format(new Date(epoch * 1000));
}

export function ReplyIntentTag({
  triage,
  className,
}: {
  triage: ReplyTriageBrief;
  className?: string;
}) {
  const { i18n, t } = useTranslation("inbox");
  const needsAttention = REPLY_INTENT_NEEDS_ATTENTION[triage.intent] && !triage.acknowledged;
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Badge
          variant="secondary"
          className={cn(
            "shrink-0 gap-1 border-transparent font-normal",
            INTENT_TONE[triage.intent],
            // 已处理的收敛掉,免得读过的判读继续抢注意力。
            triage.acknowledged && "opacity-60",
            className,
          )}
        >
          {needsAttention && (
            <span className="size-1.5 shrink-0 rounded-full bg-current" aria-hidden />
          )}
          {t(`intent.labels.${triage.intent}`)}
        </Badge>
      </TooltipTrigger>
      <TooltipContent side="bottom" align="start" className="max-w-80 space-y-1.5">
        <p className="text-xs leading-5">{triage.summary}</p>
        {triage.suggested_reply && (
          <div className="rounded-md bg-background/10 p-1.5">
            <p className="text-[10px] uppercase opacity-70">{t("intent.draftReply")}</p>
            <p className="mt-0.5 whitespace-pre-wrap text-xs leading-5">
              {triage.suggested_reply}
            </p>
          </div>
        )}
        <p className="text-[10px] opacity-70">
          {t("intent.judgedAt", {
            date: formatTriageDate(
              triage.created_at,
              i18n.resolvedLanguage ?? i18n.language,
            ),
          })}
          {triage.acknowledged ? t("intent.handledSuffix") : ""}
        </p>
      </TooltipContent>
    </Tooltip>
  );
}
